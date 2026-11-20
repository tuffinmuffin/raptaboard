//! The live NetworkTables connection, using the [`nt4`] crate.
//!
//! egui draws on the main thread and never waits, while the NT client runs on
//! tokio. [`NtConnection`] owns a small tokio runtime for the client and gives
//! the UI plain, non-blocking calls: point it at a host with
//! [`NtConnection::set_target`] each frame and read [`NtConnection::status`].
//!
//! Widgets that change values call [`NtConnection::write`]. As in Elastic,
//! only topics the server has announced can be written, with the topic's
//! own type (see [`convert`]); struct topics can't be written.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use nt4::proto::structs::StructDatabase;
use nt4::{
    Client, ClientConfig, Event, Events, GenericPublisher, NtType, Properties, SubscriptionHandle,
    SubscriptionOptions, Topic, TopicValue, Value,
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
    // Topics this dashboard has written, each published once with the
    // type it was published with, and kept until the client restarts.
    publishers: HashMap<String, (NtType, GenericPublisher)>,
}

/// Why a value couldn't be written, said so the user knows what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// Not connected to a server.
    NotConnected,
    /// The server hasn't announced the topic, so its type isn't known.
    NotPublished(String),
    /// Struct topics (and their fields) can't be written from a dashboard.
    Struct(String),
    /// The value can't be turned into the topic's type.
    WrongType {
        /// The topic's name.
        topic: String,
        /// The topic's type, such as `boolean`.
        topic_type: String,
        /// What was written, such as `text "abc"`.
        value: String,
    },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NotConnected => write!(f, "Can't send: not connected to the robot"),
            WriteError::NotPublished(topic) => {
                write!(f, "Can't send to {topic}: the robot hasn't published it")
            }
            WriteError::Struct(topic) => {
                write!(f, "Can't send to {topic}: struct values can't be changed")
            }
            WriteError::WrongType {
                topic,
                topic_type,
                value,
            } => write!(f, "Can't send {value} to {topic}: it's a {topic_type}"),
        }
    }
}

/// `value` as a value of NT type `to`, for writing to a topic of that type,
/// or `None` if it can't be one: numbers become any number type (whole
/// numbers are rounded), and text is read as the type (`"true"`, `"2.5"`).
///
/// ```
/// use nt4::{NtType, Value};
/// use raptaboard::nt::convert;
/// assert_eq!(convert(Value::Double(2.6), &NtType::Int), Some(Value::Int(3)));
/// assert_eq!(convert(Value::String("0.5".into()), &NtType::Double), Some(Value::Double(0.5)));
/// assert_eq!(convert(Value::Boolean(true), &NtType::Double), None);
/// ```
pub fn convert(value: Value, to: &NtType) -> Option<Value> {
    let number = |v: &Value| match v {
        Value::Double(n) => Some(*n),
        Value::Float(n) => Some(f64::from(*n)),
        Value::Int(n) => Some(*n as f64),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    };
    match to {
        NtType::Boolean => match value {
            Value::Boolean(b) => Some(Value::Boolean(b)),
            Value::String(text) => match text.trim().to_lowercase().as_str() {
                "true" => Some(Value::Boolean(true)),
                "false" => Some(Value::Boolean(false)),
                _ => None,
            },
            _ => None,
        },
        NtType::Double => number(&value).map(Value::Double),
        NtType::Float => number(&value).map(|n| Value::Float(n as f32)),
        NtType::Int => number(&value)
            .filter(|n| n.is_finite())
            .map(|n| Value::Int(n.round() as i64)),
        NtType::String | NtType::Json => match value {
            Value::String(text) => Some(Value::String(text)),
            _ => None,
        },
        _ => None,
    }
}

/// How a value is named in a [`WriteError`], such as `text "abc"`.
fn describe(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::Int(n) => n.to_string(),
        Value::String(text) => format!("\"{text}\""),
        _ => "that value".to_owned(),
    }
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
                publishers: HashMap::new(),
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

    /// Send `value` to topic `name` (full name), as a widget does when it's
    /// changed. The topic must be announced by the server; the value is
    /// turned into the topic's type with [`convert`]. The first write to a
    /// topic publishes it, without changing its properties, and it stays
    /// published until the connection restarts. Never blocks.
    pub fn write(&mut self, name: &str, value: Value) -> Result<(), WriteError> {
        self.write_as(name, value, None)
    }

    /// Like [`write`](Self::write), but a topic nobody has published yet is
    /// published as `ty`, as a SendableChooser's `selected` is: robot code
    /// only reads it, so the dashboard publishes it (as Elastic does).
    pub fn write_or_publish(
        &mut self,
        name: &str,
        value: Value,
        ty: NtType,
    ) -> Result<(), WriteError> {
        self.write_as(name, value, Some(ty))
    }

    fn write_as(
        &mut self,
        name: &str,
        value: Value,
        if_new: Option<NtType>,
    ) -> Result<(), WriteError> {
        let Some(running) = &mut self.running else {
            return Err(WriteError::NotConnected);
        };
        if !running.client.is_connected() {
            return Err(WriteError::NotConnected);
        }
        let ty = match (running.client.topic(name), if_new) {
            (Some(topic), _) => topic.ty,
            // Ours already (the server's announcement may still be coming).
            (None, Some(_)) if running.publishers.contains_key(name) => {
                running.publishers[name].0.clone()
            }
            (None, Some(ty)) => ty,
            (None, None) => return Err(WriteError::NotPublished(name.to_owned())),
        };
        if matches!(ty, NtType::Struct { .. }) || name.starts_with(SCHEMA_PREFIX) {
            return Err(WriteError::Struct(name.to_owned()));
        }
        let shown = describe(&value);
        let Some(value) = convert(value, &ty) else {
            return Err(WriteError::WrongType {
                topic: name.to_owned(),
                topic_type: ty.to_string(),
                value: shown,
            });
        };
        // Published again if the robot changed the topic's type.
        let stale = running
            .publishers
            .get(name)
            .is_some_and(|(published, _)| *published != ty);
        if stale || !running.publishers.contains_key(name) {
            running.publishers.remove(name);
            let publisher = running.client.publish_generic(
                name,
                ty.clone(),
                Properties::new(),
                Default::default(),
            );
            running
                .publishers
                .insert(name.to_owned(), (ty.clone(), publisher));
        }
        let (_, publisher) = &running.publishers[name];
        publisher.set(value).map_err(|_| WriteError::NotConnected)
    }

    /// Publish `bytes`, one `struct_name` struct (a `Pose2d`'s x, y and
    /// rotation), to `name`: a topic the dashboard owns, such as a Field's
    /// target. Unlike [`write`](Self::write), which never rewrites the
    /// robot's structs, this publishes struct topics, but only one nobody
    /// has published or that already holds this struct type. `schemas`
    /// (struct name, schema text, nested ones too) are published under
    /// `/.schema/` when the server doesn't have them, so other dashboards
    /// can decode it.
    pub fn publish_struct(
        &mut self,
        name: &str,
        struct_name: &str,
        bytes: Vec<u8>,
        schemas: &[(&str, &str)],
    ) -> Result<(), WriteError> {
        let Ok(ty) = format!("struct:{struct_name}").parse::<NtType>();
        let Some(running) = &mut self.running else {
            return Err(WriteError::NotConnected);
        };
        if !running.client.is_connected() {
            return Err(WriteError::NotConnected);
        }
        if let Some(topic) = running.client.topic(name)
            && topic.ty != ty
        {
            return Err(WriteError::WrongType {
                topic: name.to_owned(),
                topic_type: topic.ty.to_string(),
                value: format!("a {struct_name}"),
            });
        }
        for (schema_name, text) in schemas {
            let schema_topic = format!("{SCHEMA_PREFIX}struct:{schema_name}");
            if running.client.topic(&schema_topic).is_some()
                || running.publishers.contains_key(&schema_topic)
            {
                continue;
            }
            let Ok(schema_type) = "structschema".parse::<NtType>();
            let publisher = running.client.publish_generic(
                &schema_topic,
                schema_type.clone(),
                Properties::new(),
                Default::default(),
            );
            // Best effort: the target is still sent if a schema isn't.
            let _ = publisher.set(Value::Raw(text.as_bytes().to_vec()));
            running
                .publishers
                .insert(schema_topic, (schema_type, publisher));
        }
        if !running.publishers.contains_key(name) {
            let publisher = running.client.publish_generic(
                name,
                ty.clone(),
                Properties::new(),
                Default::default(),
            );
            running.publishers.insert(name.to_owned(), (ty, publisher));
        }
        let (_, publisher) = &running.publishers[name];
        publisher
            .set(Value::Raw(bytes))
            .map_err(|_| WriteError::NotConnected)
    }

    /// The running client, for subscribing widgets. `None` with no target.
    pub fn client(&self) -> Option<&Client> {
        self.running.as_ref().map(|r| &r.client)
    }
}
