//! The milestone 1 CPU test: 30 widgets updating at 50 Hz.
//!
//! The spec's target is under 5% CPU on a Driver Station laptop. This test
//! starts an NT server that changes 30 topics 50 times a second (plus 250
//! still ones, so the NT tree is robot-sized), opens the
//! release app on a layout of 30 widgets bound to them (each with a 0.02 s
//! period), and measures the app's CPU time over a window.
//!
//! It opens a real window and takes about 40 s, so it is ignored by default.
//! Run it by hand with the window visible (a hidden window may not redraw):
//!
//! ```text
//! cargo test -p raptaboard --release --test cpu_load -- --ignored --nocapture
//! ```
//!
//! `RAPTABOARD_CPU_SECS` sets the measurement time (default 30 s).

// Tests written by Claude.

use std::net::{IpAddr, Ipv4Addr};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use nt4::{Server, ServerConfig};
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Topics and widgets: 20 numbers in Text Displays, 10 booleans in Boolean Boxes.
const NUMBERS: usize = 20;
const BOOLEANS: usize = 10;
/// Other topics that never change, so the NT tree is about as big as on the
/// robot-2027 sim (about 280 topics).
const BACKGROUND: usize = 250;
/// How often the server changes every value, and each widget's period.
const RATE_HZ: f64 = 50.0;
/// Time for the app to start, connect and settle before measuring.
const WARM_UP: Duration = Duration::from_secs(5);
/// The spec's target: percent of the whole machine (all cores), as Task
/// Manager shows it.
const TARGET_PCT_OF_MACHINE: f64 = 5.0;

/// Kills the app when the test ends, even if it fails.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn number_topic(i: usize) -> String {
    format!("/CpuTest/number{i:02}")
}

fn boolean_topic(i: usize) -> String {
    format!("/CpuTest/flag{i:02}")
}

/// An Elastic layout with one tab: a 6 x 5 grid of single-cell widgets.
fn layout_json() -> String {
    let period = 1.0 / RATE_HZ;
    let widget = |n: usize, title: String, kind: &str, topic: String, data_type: &str| {
        json!({
            "title": title,
            "x": (n % 6 * 128) as f64,
            "y": (n / 6 * 128) as f64,
            "width": 128.0,
            "height": 128.0,
            "type": kind,
            "properties": {"topic": topic, "period": period, "data_type": data_type},
        })
    };
    let containers: Vec<_> = (0..NUMBERS)
        .map(|i| {
            widget(
                i,
                format!("number {i}"),
                "Text Display",
                number_topic(i),
                "double",
            )
        })
        .chain((0..BOOLEANS).map(|i| {
            widget(
                NUMBERS + i,
                format!("flag {i}"),
                "Boolean Box",
                boolean_topic(i),
                "boolean",
            )
        }))
        .collect();
    json!({
        "version": 1.0,
        "grid_size": 128,
        "tabs": [{"name": "CPU test", "grid_layout": {"layouts": [], "containers": containers}}],
    })
    .to_string()
}

/// The app's total CPU time so far, in milliseconds (all threads, all cores).
fn cpu_ms(sys: &mut System, pid: Pid) -> u64 {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );
    sys.process(pid)
        .expect("the app exited early; run with --nocapture to see why")
        .accumulated_cpu_time()
}

#[test]
#[ignore = "opens a window and takes about 40 s; see the file comment"]
fn thirty_widgets_at_50_hz() {
    let measure = Duration::from_secs(
        std::env::var("RAPTABOARD_CPU_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
    );

    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let server = server_rt
        .block_on(Server::start(ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))
        .unwrap();
    let numbers: Vec<_> = (0..NUMBERS)
        .map(|i| server.publish::<f64>(&number_topic(i)))
        .collect();
    let flags: Vec<_> = (0..BOOLEANS)
        .map(|i| server.publish::<bool>(&boolean_topic(i)))
        .collect();
    let background: Vec<_> = (0..BACKGROUND)
        .map(|i| {
            let p = server.publish::<f64>(&format!("/Background/group{}/value{i:03}", i / 25));
            p.set(i as f64);
            p
        })
        .collect();

    let layout = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cpu_test_layout.json");
    std::fs::write(&layout, layout_json()).unwrap();
    let app = Command::new(env!("CARGO_BIN_EXE_raptaboard"))
        .args(["--address", "localhost", "--port"])
        .arg(server.local_addr().port().to_string())
        .arg(&layout)
        .spawn()
        .expect("start the app");
    let pid = Pid::from_u32(app.id());
    let _app = KillOnDrop(app);
    let _keep = background;

    let mut sys = System::new().expect("read process info");
    let tick = Duration::from_secs_f64(1.0 / RATE_HZ);
    let start = Instant::now();
    let mut next = start;
    let mut measuring: Option<(Instant, u64)> = None;
    let mut ticks: u64 = 0;
    loop {
        // Change every value: numbers as sine waves, flags at different rates.
        let t = start.elapsed().as_secs_f64();
        for (i, p) in numbers.iter().enumerate() {
            p.set((t * (1.0 + i as f64 * 0.1)).sin() * 100.0);
        }
        for (i, p) in flags.iter().enumerate() {
            p.set((ticks / (i as u64 + 1)).is_multiple_of(2));
        }
        ticks += 1;

        let now = Instant::now();
        if measuring.is_none() && now - start >= WARM_UP {
            measuring = Some((now, cpu_ms(&mut sys, pid)));
        }
        if let Some((from, cpu_from)) = measuring
            && now - from >= measure
        {
            let cpu = cpu_ms(&mut sys, pid) - cpu_from;
            let wall_ms = (now - from).as_secs_f64() * 1000.0;
            let of_one_core = cpu as f64 / wall_ms * 100.0;
            let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
            let of_machine = of_one_core / cores as f64;
            println!(
                "{} widgets at {RATE_HZ} Hz for {:.0} s: {of_one_core:.1}% of one core, \
                 {of_machine:.2}% of this machine ({cores} cores); target under \
                 {TARGET_PCT_OF_MACHINE}% of the machine",
                NUMBERS + BOOLEANS,
                wall_ms / 1000.0,
            );
            assert!(
                of_machine < TARGET_PCT_OF_MACHINE,
                "over the CPU target: {of_machine:.2}%"
            );
            return;
        }
        next += tick;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
}
