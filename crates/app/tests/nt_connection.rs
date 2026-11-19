//! NtConnection against a real NT4 server on localhost.

// Tests written by Claude.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use nt4::{Server, ServerConfig, Value};
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
    nt.watch_widget_values(vec![(0.06, vec!["/Drive/Health/allModulesHealthy".into()])]);
    wait_value(&nt, true);

    // Closing the tree folder must not stop the widget's updates.
    nt.watch_values(Vec::new());
    healthy.set(false);
    wait_value(&nt, false);
}

#[test]
fn traced_topics_get_every_value_not_just_the_newest() {
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let port = server.local_addr().port();
    let speed = server.publish::<f64>("/Drive/speed");
    let other = server.publish::<f64>("/Drive/other");
    speed.set(-1.0);
    other.set(0.0);

    let mut nt = NtConnection::new().unwrap();
    nt.set_target(Some("127.0.0.1"), port);
    wait_for(
        &nt,
        |s| matches!(s, ConnectionStatus::Connected { topics, .. } if *topics >= 2),
    );
    nt.trace(vec!["/Drive/speed".into()], 0.01);
    // Wait for the first value, so the subscription is in place.
    let deadline = Instant::now() + Duration::from_secs(10);
    while nt.take_traced().is_empty() {
        assert!(Instant::now() < deadline, "no traced value arrived");
        std::thread::sleep(Duration::from_millis(20));
    }

    // 50 values 2 ms apart, much faster than the 10 ms period: all of them
    // should come through, not only the newest per period.
    for i in 0..50 {
        speed.set(f64::from(i));
        other.set(f64::from(i));
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while seen.last() != Some(&49.0) {
        assert!(Instant::now() < deadline, "got {seen:?}");
        for (name, value) in nt.take_traced() {
            // Only the traced topic.
            assert_eq!(&*name, "/Drive/speed");
            if let nt4::Value::Double(d) = value.value {
                seen.push(d);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let expected: Vec<f64> = (0..50).map(f64::from).collect();
    assert_eq!(seen, expected);
}

#[test]
fn writes_reach_the_server_in_the_topics_own_type() {
    use raptaboard::nt::WriteError;

    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let port = server.local_addr().port();
    // What robot code publishes for the dashboard to change.
    let enabled = server.publish::<bool>("/Tune/enabled");
    enabled.set(false);
    let count = server.publish::<i64>("/Tune/count");
    count.set(1);
    let name = server.publish::<String>("/Tune/name");
    name.set("a".into());
    server.add_schema("struct:Rotation2d", "structschema", b"double value");
    let heading = server.publish_generic(
        "/Tune/heading",
        "struct:Rotation2d".parse().unwrap(),
        nt4::Properties::new(),
    );
    heading
        .set(nt4::Value::Raw(1.0f64.to_le_bytes().to_vec()))
        .unwrap();
    // What the server sees of them.
    let seen_enabled = server.subscribe::<bool>("/Tune/enabled", Default::default());
    let seen_count = server.subscribe::<i64>("/Tune/count", Default::default());
    let seen_name = server.subscribe::<String>("/Tune/name", Default::default());

    let mut nt = NtConnection::new().unwrap();
    assert_eq!(
        nt.write("/Tune/enabled", Value::Boolean(true)),
        Err(WriteError::NotConnected)
    );
    nt.set_target(Some("127.0.0.1"), port);
    wait_for(
        &nt,
        |s| matches!(s, ConnectionStatus::Connected { topics, .. } if *topics >= 4),
    );

    nt.write("/Tune/enabled", Value::Boolean(true)).unwrap();
    // A number for an int topic is rounded; text is read as the type.
    nt.write("/Tune/count", Value::Double(6.6)).unwrap();
    nt.write("/Tune/name", Value::String("b".into())).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(seen_enabled.get() == Some(true)
        && seen_count.get() == Some(7)
        && seen_name.get().as_deref() == Some("b"))
    {
        assert!(Instant::now() < deadline, "writes never reached the server");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Writing again uses the same publisher.
    nt.write("/Tune/count", Value::String(" 9 ".into()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while seen_count.get() != Some(9) {
        assert!(Instant::now() < deadline, "second write never arrived");
        std::thread::sleep(Duration::from_millis(20));
    }

    // Refused, with reasons a student can act on.
    let err = nt.write("/Tune/missing", Value::Boolean(true)).unwrap_err();
    assert_eq!(err, WriteError::NotPublished("/Tune/missing".into()));
    assert_eq!(
        err.to_string(),
        "Can't send to /Tune/missing: the robot hasn't published it"
    );
    let err = nt.write("/Tune/heading", Value::Double(1.0)).unwrap_err();
    assert_eq!(err, WriteError::Struct("/Tune/heading".into()));
    let err = nt
        .write("/Tune/enabled", Value::String("maybe".into()))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Can't send \"maybe\" to /Tune/enabled: it's a boolean"
    );
    // The robot's own value is untouched by refused writes.
    assert_eq!(seen_enabled.get(), Some(true));
}
