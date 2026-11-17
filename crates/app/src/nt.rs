//! The live NetworkTables connection, using the [`nt4`] crate.
//!
//! egui draws on the main thread and never waits, while the NT client runs on
//! tokio. [`NtConnection`] owns a small tokio runtime for the client and gives
//! the UI plain, non-blocking calls: point it at a host with
//! [`NtConnection::set_target`] each frame and read [`NtConnection::status`].

use std::time::Duration;

use nt4::{Client, ClientConfig, SubscriptionHandle, SubscriptionOptions};

/// Client name sent to the server, so robot code sees `/nt/raptaboard`.
pub const CLIENT_NAME: &str = "raptaboard";

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

/// A running client and the address it was started for.
struct Running {
    host: String,
    port: u16,
    client: Client,
    // Keeps the "announce every topic" subscription open; dropping it unsubscribes.
    _all_topics: SubscriptionHandle,
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
            self.running = Some(Running {
                host: host.to_owned(),
                port,
                client,
                _all_topics: all_topics,
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

    /// The running client, for subscribing widgets. `None` with no target.
    pub fn client(&self) -> Option<&Client> {
        self.running.as_ref().map(|r| &r.client)
    }
}
