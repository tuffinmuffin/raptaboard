//! The live NetworkTables connection, using the [`nt4`] crate.
//!
//! egui draws on the main thread and never waits, while the NT client runs on
//! tokio. [`NtConnection`] owns a small tokio runtime for the client and gives
//! the UI plain, non-blocking calls: point it at a host with
//! [`NtConnection::set_target`] each frame and read [`NtConnection::status`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use nt4::proto::structs::StructDatabase;
use nt4::{
    Client, ClientConfig, Event, Events, SubscriptionHandle, SubscriptionOptions, Topic,
    TopicValue, Value,
};

/// Client name sent to the server, so the server lists us as `/nt/raptaboard`.
pub const CLIENT_NAME: &str = "raptaboard";

/// Where publishers put struct schemas: `/.schema/struct:Pose2d` holds
/// the text `Translation2d translation;Rotation2d rotation`.
pub const SCHEMA_PREFIX: &str = "/.schema/";

/// Where the connection is, for the settings bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// No address to connect to yet (for example, waiting for the Driver Station).
    NoTarget,
    /// Trying to reach the server at this address; the client retries on its own.
    Connecting { host: String, port: u16 },
    /// Connected, with how many topics the server has announced so far.
    Connected {
        host: String,
        port: u16,
        topics: usize,
    },
}

/// One value subscription and what it was made for, so it is only redone
/// when the topic list or period changes.
#[derive(Default)]
struct Watch {
    names: Vec<String>,
    period_s: Option<f64>,
    /// Every change, not only the newest per period.
    all: bool,
    handle: Option<SubscriptionHandle>,
}

impl Watch {
    fn set(&mut self, client: &Client, names: Vec<String>, period_s: Option<f64>) {
        self.set_with(client, names, period_s, false);
    }

    fn set_with(
        &mut self,
        client: &Client,
        mut names: Vec<String>,
        period_s: Option<f64>,
        all: bool,
    ) {
        names.sort();
        names.dedup();
        if self.names == names && self.period_s == period_s && self.all == all {
            return;
        }
        // Drop the old subscription first; the new one replaces it.
        self.handle = None;
        if !names.is_empty() {
            self.handle = Some(client.subscribe_multiple(
                names.clone(),
                SubscriptionOptions {
                    periodic: period_s,
                    all,
                    ..Default::default()
                },
            ));
        }
        self.names = names;
        self.period_s = period_s;
        self.all = all;
    }
}

/// A running client and the address it was started for.
struct Running {
    host: String,
    port: u16,
    client: Client,
    // Keeps the "announce every topic" subscription open; dropping it unsubscribes.
    _all_topics: SubscriptionHandle,
    // Values for the topics the tree shows right now.
    tree_values: Watch,
    // Values for the widgets on the open tab: one subscription per update
    // period, keyed by the period's bits.
    widget_values: HashMap<u64, Watch>,
    // Keeps the `/.schema/` values coming; struct topics need them to decode.
    _schemas: SubscriptionHandle,
    // Struct schemas from this server, and the bytes each was added from.
    structs: StructDatabase,
    schema_bytes: HashMap<String, Vec<u8>>,
    // Every value the client receives, as it arrives, for recording graphs.
    events: Events,
    // Every change of the topics graphs record, not only the newest.
    traced_values: Watch,
    // The topics whose values `take_traced` returns.
    traced: HashSet<String>,
}

/// The dashboard's NT connection: a tokio runtime plus at most one client.
///
/// ```no_run
/// use raptaboard::nt::{ConnectionStatus, NtConnection};
/// let mut nt = NtConnection::new().expect("tokio runtime");
/// nt.set_target(Some("127.0.0.1"), 5810);
/// // Later, once per frame:
/// if let ConnectionStatus::Connected { topics, .. } = nt.status() {
///     println!("{topics} topics");
/// }
/// ```
pub struct NtConnection {
    // Dropped after `running`, so the client stops inside a live runtime.
    running: Option<Running>,
    runtime: tokio::runtime::Runtime,
}

impl NtConnection {
    /// Start the tokio runtime the client runs on (one worker thread).
    pub fn new() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("raptaboard-nt")
            .enable_all()
            .build()?;
        Ok(NtConnection {
            running: None,
            runtime,
        })
    }

    /// Connect to `host` on `port`, or disconnect when `host` is `None` or blank.
    ///
    /// Cheap to call every frame: the client only restarts when the address
    /// changes. While the server is unreachable the client keeps retrying.
    pub fn set_target(&mut self, host: Option<&str>, port: u16) {
        let host = host.map(str::trim).filter(|h| !h.is_empty());
        let same = match (&self.running, host) {
            (Some(r), Some(h)) => r.host == h && r.port == port,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        // Stop the old client before starting the new one.
        self.running = None;
        if let Some(host) = host {
            let _guard = self.runtime.enter();
            let mut config = ClientConfig::server(CLIENT_NAME, host, port);
            config.connect_timeout = Duration::from_secs(2);
            config.reconnect_delay = Duration::from_millis(500);
            let client = Client::start(config);
            // Ask for every topic's announcement (names and types, no values)
            // so the topic count and, later, the NT tree can list them.
            let all_topics = client.subscribe_multiple(
                vec![String::new()],
                SubscriptionOptions {
                    prefix: true,
                    topics_only: true,
                    ..Default::default()
                },
            );
            // Struct schemas are small and needed to show any struct value.
            let schemas = client.subscribe_multiple(
                vec![SCHEMA_PREFIX.into()],
                SubscriptionOptions {
                    prefix: true,
                    ..Default::default()
                },
            );
            // Every value as it arrives; taken from before any subscription
            // can deliver one.
            let events = client.events();
            self.running = Some(Running {
                host: host.to_owned(),
                port,
                client,
                _all_topics: all_topics,
                tree_values: Watch::default(),
                widget_values: HashMap::new(),
                _schemas: schemas,
                structs: StructDatabase::new(),
                schema_bytes: HashMap::new(),
                events,
                traced_values: Watch::default(),
                traced: HashSet::new(),
            });
        }
    }

    /// Where the connection is right now. Never blocks.
    pub fn status(&self) -> ConnectionStatus {
        let Some(r) = &self.running else {
            return ConnectionStatus::NoTarget;
        };
        let (host, port) = (r.host.clone(), r.port);
        if r.client.is_connected() {
            ConnectionStatus::Connected {
                host,
                port,
                topics: r.client.topics().len(),
            }
        } else {
            ConnectionStatus::Connecting { host, port }
        }
    }

    /// Every topic the server has announced, with its newest value if one
    /// has arrived. Empty with no target or before the first connection.
    pub fn topics(&self) -> Vec<Topic> {
        self.running
            .as_ref()
            .map_or_else(Vec::new, |r| r.client.topics())
    }

    /// Ask the server for the values of exactly these topics (full names),
    /// replacing the tree's previous request. Cheap to call every frame: it
    /// only resubscribes when the list changes. An empty list stops them.
    ///
    /// The tree uses this so only topics in open folders use bandwidth.
    pub fn watch_values(&mut self, names: Vec<String>) {
        let Some(r) = &mut self.running else { return };
        r.tree_values.set(&r.client, names, None);
    }

    /// Like [`watch_values`](Self::watch_values), for the widgets on the open
    /// tab. Each group is an update period in seconds (Elastic's default is
    /// 0.06) and the topics wanted that often; each period gets its own
    /// subscription, so a slow widget doesn't use bandwidth at a fast one's
    /// rate. A topic in two groups comes at the faster one. Kept apart from
    /// the tree's request so opening a folder doesn't slow a widget down.
    pub fn watch_widget_values(&mut self, groups: Vec<(f64, Vec<String>)>) {
        let Some(r) = &mut self.running else { return };
        let mut wanted = HashMap::new();
        for (period_s, names) in groups {
            wanted
                .entry(period_s.to_bits())
                .or_insert_with(Vec::new)
                .extend(names);
        }
        r.widget_values
            .retain(|period, _| wanted.contains_key(period));
        for (period, names) in wanted {
            r.widget_values.entry(period).or_default().set(
                &r.client,
                names,
                Some(f64::from_bits(period)),
            );
        }
    }

    /// Record every value of these topics (full names), at up to one every
    /// `period_s` seconds, for graphs: each change is sent, not only the
    /// newest, and [`take_traced`](Self::take_traced) returns them all.
    /// Cheap to call every frame. An empty list stops it.
    pub fn trace(&mut self, names: Vec<String>, period_s: f64) {
        let Some(r) = &mut self.running else { return };
        r.traced = names.iter().cloned().collect();
        r.traced_values
            .set_with(&r.client, names, Some(period_s), true);
    }

    /// Every value of the [`trace`](Self::trace)d topics received since the
    /// last call, oldest first, with its topic name. Call once a frame:
    /// other values are thrown away here, so nothing piles up.
    pub fn take_traced(&mut self) -> Vec<(Arc<str>, TopicValue)> {
        let Some(r) = &mut self.running else {
            return Vec::new();
        };
        let mut values = Vec::new();
        while let Some(event) = r.events.try_recv() {
            if let Event::Value { name, value, .. } = event
                && r.traced.contains(&*name)
            {
                values.push((name, value));
            }
        }
        values
    }

    /// Add any struct schemas in `topics` that are new or changed, so struct
    /// values can be decoded with [`structs`](Self::structs). Pass the list
    /// from [`topics`](Self::topics); cheap when nothing changed.
    pub fn update_structs(&mut self, topics: &[Topic]) {
        let Some(r) = &mut self.running else { return };
        for topic in topics {
            let Some(name) = topic
                .name
                .strip_prefix(SCHEMA_PREFIX)
                .and_then(|n| n.strip_prefix("struct:"))
            else {
                continue;
            };
            let Some(Value::Raw(bytes)) = topic.value.as_ref().map(|v| &v.value) else {
                continue;
            };
            if r.schema_bytes.get(name) == Some(bytes) {
                continue;
            }
            // A schema that doesn't parse is skipped; its topics show as bytes.
            let text = String::from_utf8_lossy(bytes);
            let _ = r.structs.add(name, &text);
            r.schema_bytes.insert(name.to_owned(), bytes.clone());
        }
    }

    /// Struct schemas received from this server so far.
    pub fn structs(&self) -> Option<&StructDatabase> {
        self.running.as_ref().map(|r| &r.structs)
    }

    /// The running client, for subscribing widgets. `None` with no target.
    pub fn client(&self) -> Option<&Client> {
        self.running.as_ref().map(|r| &r.client)
    }
}
