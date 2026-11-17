//! NtConnection against a real NT4 server on localhost.

// Tests written by Claude.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use nt4::{Server, ServerConfig};
use raptaboard::nt::{ConnectionStatus, NtConnection};

/// Poll `status` until `done` says yes, or fail after 10 s.
fn wait_for(nt: &NtConnection, done: impl Fn(&ConnectionStatus) -> bool) -> ConnectionStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = nt.status();
        if done(&status) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "timed out, last status {status:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn connects_and_counts_announced_topics() {
    // The server gets its own runtime, like robot code would be its own process.
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let port = server.local_addr().port();
    let angle = server.publish::<f64>("/Telemetry/Arm/angle_rad");
    angle.set(0.5);
    let tag = server.publish::<String>("/Telemetry/Arm/.type");
    tag.set("Arm".into());

    let mut nt = NtConnection::new().unwrap();
    assert_eq!(nt.status(), ConnectionStatus::NoTarget);

    nt.set_target(Some("127.0.0.1"), port);
    let status = wait_for(
        &nt,
        |s| matches!(s, ConnectionStatus::Connected { topics, .. } if *topics >= 2),
    );
    let ConnectionStatus::Connected { host, port: p, .. } = status else {
        unreachable!()
    };
    assert_eq!((host.as_str(), p), ("127.0.0.1", port));

    // Calling again with the same address keeps the same connection.
    nt.set_target(Some("127.0.0.1"), port);
    assert!(matches!(nt.status(), ConnectionStatus::Connected { .. }));

    // No address disconnects.
    nt.set_target(None, port);
    assert_eq!(nt.status(), ConnectionStatus::NoTarget);
}

#[test]
fn unreachable_server_stays_connecting() {
    let mut nt = NtConnection::new().unwrap();
    // Port 1 on localhost has nothing listening.
    nt.set_target(Some("127.0.0.1"), 1);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        nt.status(),
        ConnectionStatus::Connecting {
            host: "127.0.0.1".into(),
            port: 1
        }
    );
    // A blank custom address counts as no address.
    nt.set_target(Some("  "), 5810);
    assert_eq!(nt.status(), ConnectionStatus::NoTarget);
}
