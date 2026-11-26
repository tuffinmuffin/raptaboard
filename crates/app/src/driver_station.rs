//! The robot's address from the Driver Station on this computer, for the
//! "Driver Station" server choice, as Elastic gets it:
//!
//! - The 2027 Driver Station (SystemCore) serves a websocket,
//!   `ws://127.0.0.1:6768/ipws`, sending JSON such as
//!   `{"robotIp": "10.32.0.2"}` (`"0.0.0.0"` while it doesn't know).
//! - The NI Driver Station (a roboRIO, through 2026) serves TCP port 1742,
//!   sending JSON such as `{"robotIP": 169869314}`: the address as one
//!   number, big-endian (0 while it doesn't know). WPILib's own
//!   NetworkTables client reads the same.
//!
//! [`DsLookup`] listens to both, retrying each second, and keeps the newest
//! address either gave. It only ever connects to this computer.

use std::io::{ErrorKind, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The 2027 Driver Station's websocket.
pub const WEBSOCKET_URL: &str = "ws://127.0.0.1:6768/ipws";
/// The NI Driver Station's TCP port, on this computer.
pub const NI_PORT: u16 = 1742;
/// How long between tries while a Driver Station isn't there.
const RETRY: Duration = Duration::from_secs(1);
/// How often a thread waiting on a read looks whether it should stop.
const TICK: Duration = Duration::from_millis(100);

/// The robot's address in a 2027 Driver Station message, or `None` if it
/// has none (or `0.0.0.0`, not known yet).
///
/// ```
/// use raptaboard::driver_station::robot_ip_2027;
/// assert_eq!(robot_ip_2027(r#"{"robotIp": "10.32.0.2", "dockedHeight": 0}"#), Some("10.32.0.2".into()));
/// assert_eq!(robot_ip_2027(r#"{"robotIp": "0.0.0.0"}"#), None);
/// ```
pub fn robot_ip_2027(text: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let ip = json.get("robotIp")?.as_str()?.trim();
    (!ip.is_empty() && ip != "0.0.0.0").then(|| ip.to_string())
}

/// The robot's address in an NI Driver Station message, or `None` if it
/// has none (or 0, not known yet).
///
/// ```
/// use raptaboard::driver_station::robot_ip_ni;
/// assert_eq!(robot_ip_ni(r#"{"robotIP": 169869314}"#), Some("10.32.0.2".into()));
/// assert_eq!(robot_ip_ni(r#"{"robotIP": 0}"#), None);
/// ```
pub fn robot_ip_ni(text: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let n = json.get("robotIP")?.as_u64()?;
    let n = u32::try_from(n).ok().filter(|n| *n != 0)?;
    Some(Ipv4Addr::from(n).to_string())
}

/// The whole JSON objects at the front of `buffer`, taken out of it: the NI
/// Driver Station sends one after another on its stream, maybe split or
/// joined across reads. What's left is a part still to come.
fn take_messages(buffer: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(end) = buffer.find('}') {
        let message: String = buffer.drain(..=end).collect();
        if let Some(start) = message.rfind('{') {
            out.push(message[start..].to_string());
        }
    }
    out
}

/// Listens to the Driver Station on this computer for the robot's address;
/// stops when dropped.
pub struct DsLookup {
    latest: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl DsLookup {
    /// Listen to the Driver Station on this computer, the 2027 one and the
    /// NI one.
    pub fn start() -> Self {
        Self::start_at(
            WEBSOCKET_URL,
            SocketAddr::from((Ipv4Addr::LOCALHOST, NI_PORT)),
        )
    }

    /// Listen to a 2027 Driver Station's websocket at `websocket` and an NI
    /// one at `ni`: [`DsLookup::start`] with other places, for tests.
    pub fn start_at(websocket: &str, ni: SocketAddr) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let url = websocket.to_string();
        let (l, s) = (latest.clone(), stop.clone());
        let ws = std::thread::Builder::new()
            .name("ds-2027".into())
            .spawn(move || listen_2027(&url, &l, &s))
            .expect("start the Driver Station thread");
        let (l, s) = (latest.clone(), stop.clone());
        let tcp = std::thread::Builder::new()
            .name("ds-ni".into())
            .spawn(move || listen_ni(ni, &l, &s))
            .expect("start the Driver Station thread");
        DsLookup {
            latest,
            stop,
            threads: vec![ws, tcp],
        }
    }

    /// The robot's address, once a Driver Station has given one: kept if
    /// the Driver Station closes, until another comes.
    pub fn robot_ip(&self) -> Option<String> {
        self.latest.lock().ok()?.clone()
    }
}

impl Drop for DsLookup {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Sleep `d`, a tick at a time, until `stop`; whether to go on.
fn wait(d: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(TICK);
    }
    !stop.load(Ordering::Relaxed)
}

/// Keep `ip` as the newest address, if there is one.
fn keep(latest: &Mutex<Option<String>>, ip: Option<String>) {
    if let (Some(ip), Ok(mut l)) = (ip, latest.lock()) {
        *l = Some(ip);
    }
}

/// A read that ran out of time rather than failing.
fn timed_out(e: &std::io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// The 2027 Driver Station's websocket at `url`, until `stop`.
fn listen_2027(url: &str, latest: &Mutex<Option<String>>, stop: &AtomicBool) {
    let addr = url
        .strip_prefix("ws://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|host| host.parse::<SocketAddr>().ok());
    let Some(addr) = addr else { return };
    while !stop.load(Ordering::Relaxed) {
        // The handshake gets the whole retry time to answer; then reads
        // wait a tick at a time, to see `stop`.
        if let Ok(stream) = TcpStream::connect_timeout(&addr, RETRY)
            && stream.set_read_timeout(Some(RETRY)).is_ok()
            && let Ok((mut socket, _)) = tungstenite::client(url, stream)
            && socket.get_ref().set_read_timeout(Some(TICK)).is_ok()
        {
            while !stop.load(Ordering::Relaxed) {
                match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => keep(latest, robot_ip_2027(&text)),
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if timed_out(&e) => {}
                    Err(_) => break,
                }
            }
        }
        if !wait(RETRY, stop) {
            return;
        }
    }
}

/// The NI Driver Station's TCP port at `addr`, until `stop`.
fn listen_ni(addr: SocketAddr, latest: &Mutex<Option<String>>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, RETRY)
            && stream.set_read_timeout(Some(TICK)).is_ok()
        {
            let mut text = String::new();
            let mut chunk = [0u8; 1024];
            while !stop.load(Ordering::Relaxed) {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                        for message in take_messages(&mut text) {
                            keep(latest, robot_ip_ni(&message));
                        }
                    }
                    Err(e) if timed_out(&e) => {}
                    Err(_) => break,
                }
            }
        }
        if !wait(RETRY, stop) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn a_2027_message_gives_its_address_unless_it_has_none() {
        assert_eq!(
            robot_ip_2027(r#"{"robotIp":"172.30.0.1"}"#),
            Some("172.30.0.1".into())
        );
        for none in [
            r#"{"robotIp":"0.0.0.0"}"#,
            r#"{"robotIp":""}"#,
            r#"{"dockedHeight":300}"#,
            r#"{"robotIp":167772160}"#,
            "not json",
        ] {
            assert_eq!(robot_ip_2027(none), None, "{none}");
        }
    }

    #[test]
    fn an_ni_message_gives_its_address_big_endian() {
        // 10.32.0.2 is 0x0A200002.
        assert_eq!(
            robot_ip_ni(r#"{"robotIP":169869314}"#),
            Some("10.32.0.2".into())
        );
        assert_eq!(
            robot_ip_ni(r#"{"robotIP":2130706433}"#),
            Some("127.0.0.1".into())
        );
        for none in [
            r#"{"robotIP":0}"#,
            r#"{"robotIP":-1}"#,
            r#"{"robotIP":4294967296}"#,
            r#"{"robotIP":"10.32.0.2"}"#,
            r#"{"robotIp":169869314}"#,
            "{",
        ] {
            assert_eq!(robot_ip_ni(none), None, "{none}");
        }
    }

    #[test]
    fn messages_joined_or_split_across_reads_come_out_whole() {
        let mut buffer = String::from("{\"robotIP\":0}\n{\"robotIP\":1");
        assert_eq!(take_messages(&mut buffer), ["{\"robotIP\":0}"]);
        assert_eq!(buffer, "\n{\"robotIP\":1");
        buffer.push_str("69869314}{\"robotIP\":2}");
        assert_eq!(
            take_messages(&mut buffer),
            ["{\"robotIP\":169869314}", "{\"robotIP\":2}"]
        );
        assert!(buffer.is_empty());
        // Junk before a message is dropped with it.
        let mut buffer = String::from("junk}{\"robotIP\":3}");
        assert_eq!(take_messages(&mut buffer), ["{\"robotIP\":3}"]);
    }

    /// A port nothing listens on.
    fn closed_port() -> SocketAddr {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
    }

    /// Wait up to 5 s for `lookup` to give `want`.
    fn gives(lookup: &DsLookup, want: &str) {
        let until = Instant::now() + Duration::from_secs(5);
        while lookup.robot_ip().as_deref() != Some(want) {
            assert!(
                Instant::now() < until,
                "never gave {want}: {:?}",
                lookup.robot_ip()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_2027_websocket_gives_the_address_and_is_listened_to_again_after_it_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}/ipws", listener.local_addr().unwrap());
        let ds = std::thread::spawn(move || {
            // Not known yet, then known, then gone; the next connection
            // gives another.
            for messages in [
                vec![
                    r#"{"robotIp":"0.0.0.0","dockedHeight":0}"#,
                    r#"{"robotIp":"10.32.0.2"}"#,
                ],
                vec![r#"{"robotIp":"172.30.0.1"}"#],
            ] {
                let (stream, _) = listener.accept().unwrap();
                let mut socket = tungstenite::accept(stream).unwrap();
                for m in messages {
                    socket.send(tungstenite::Message::text(m)).unwrap();
                }
                std::thread::sleep(Duration::from_millis(300));
                let _ = socket.close(None);
                let _ = socket.flush();
            }
        });
        let lookup = DsLookup::start_at(&url, closed_port());
        gives(&lookup, "10.32.0.2");
        gives(&lookup, "172.30.0.1");
        ds.join().unwrap();
        // Closed: the last address is kept.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(lookup.robot_ip().as_deref(), Some("172.30.0.1"));
    }

    #[test]
    fn the_ni_port_gives_the_address_from_messages_split_across_writes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let ni = listener.local_addr().unwrap();
        let ds = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(b"{\"robotIP\":0}\n{\"robotIP\":1698")
                .unwrap();
            std::thread::sleep(Duration::from_millis(150));
            stream.write_all(b"69314}\n").unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let lookup = DsLookup::start_at("ws://127.0.0.1:1/ipws", ni);
        gives(&lookup, "10.32.0.2");
        ds.join().unwrap();
    }

    #[test]
    fn with_no_driver_station_there_is_no_address_and_dropping_stops_at_once() {
        let lookup = DsLookup::start_at(&format!("ws://{}/ipws", closed_port()), closed_port());
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(lookup.robot_ip(), None);
        let start = Instant::now();
        drop(lookup);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }
}
