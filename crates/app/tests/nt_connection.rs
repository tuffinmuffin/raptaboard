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

    // The topics-only subscription brings names and types but no values.
    let angle_value = |nt: &NtConnection| {
        nt.topics()
            .into_iter()
            .find(|t| &*t.name == "/Telemetry/Arm/angle_rad")
            .and_then(|t| t.value)
            .map(|v| v.value)
    };
    assert_eq!(angle_value(&nt), None);

    // Asking for one topic's value brings it.
    nt.watch_values(vec!["/Telemetry/Arm/angle_rad".into()]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while angle_value(&nt).is_none() {
        assert!(Instant::now() < deadline, "value never arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(angle_value(&nt), Some(nt4::Value::Double(0.5)));

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

#[test]
fn struct_schemas_arrive_and_decode() {
    use raptaboard::struct_view::{Shown, decode};

    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let port = server.local_addr().port();
    // What robot code publishes for a Rotation2d topic: its schema, then bytes.
    server.add_schema("struct:Rotation2d", "structschema", b"double value");
    let heading = server.publish_generic(
        "/Drive/heading",
        "struct:Rotation2d".parse().unwrap(),
        nt4::Properties::new(),
    );
    heading
        .set(nt4::Value::Raw(1.25f64.to_le_bytes().to_vec()))
        .unwrap();

    let mut nt = NtConnection::new().unwrap();
    nt.set_target(Some("127.0.0.1"), port);
    nt.watch_values(vec!["/Drive/heading".into()]);

    let deadline = Instant::now() + Duration::from_secs(10);
    let shown = loop {
        let topics = nt.topics();
        nt.update_structs(&topics);
        let bytes = topics
            .iter()
            .find(|t| &*t.name == "/Drive/heading")
            .and_then(|t| match t.value.as_ref().map(|v| &v.value) {
                Some(nt4::Value::Raw(b)) => Some(b.clone()),
                _ => None,
            });
        if let (Some(structs), Some(bytes)) = (nt.structs(), bytes)
            && let Some(shown) = decode(structs, "struct:Rotation2d", &bytes)
        {
            break shown;
        }
        assert!(Instant::now() < deadline, "struct never decoded");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        shown,
        Shown::Node {
            summary: "Rotation2d".into(),
            children: vec![("value".into(), Shown::Leaf("1.25".into()))],
        }
    );
}

#[test]
fn widget_values_keep_coming_when_the_tree_closes() {
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let healthy = server.publish::<bool>("/Drive/Health/allModulesHealthy");
    healthy.set(true);

    let mut nt = NtConnection::new().unwrap();
    nt.set_target(Some("127.0.0.1"), server.local_addr().port());
    wait_for(&nt, |s| matches!(s, ConnectionStatus::Connected { .. }));
    let value = |nt: &NtConnection| {
        nt.topics()
            .into_iter()
            .find(|t| &*t.name == "/Drive/Health/allModulesHealthy")
            .and_then(|t| t.value)
            .map(|v| v.value)
    };
    let wait_value = |nt: &NtConnection, want: bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while value(nt) != Some(nt4::Value::Boolean(want)) {
            assert!(
                Instant::now() < deadline,
                "never saw {want}, last {:?}",
                value(nt)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };

    // The tree and a widget both want the topic.
    nt.watch_values(vec!["/Drive/Health/allModulesHealthy".into()]);
    nt.watch_widget_values(vec!["/Drive/Health/allModulesHealthy".into()], 0.06);
    wait_value(&nt, true);

    // Closing the tree folder must not stop the widget's updates.
    nt.watch_values(Vec::new());
    healthy.set(false);
    wait_value(&nt, false);
}
