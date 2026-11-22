//! The Battery widget: a voltage as charge and level, following changes.

// Tests written by Claude.

use super::*;

#[test]
fn a_battery_shows_its_volts_charge_and_level() {
    let robot = Robot::start();
    let volts = robot._server.publish::<f64>("/Robot/battery");
    volts.set(12.6);
    let mut battery = widget(
        "Battery",
        "/Robot/battery",
        "double",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 128.0,
        },
    );
    battery.title = "Battery".into();
    let mut tab = Tab::new("Power");
    tab.grid_layout.containers.push(battery);
    let mut harness = harness_for(&robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    // 11 V empty to 13 V full by default; yellow below 12, red below 11.5.
    labelled_soon(&mut harness, "Battery: 12.60 V, 80%, ok");
    volts.set(11.8);
    labelled_soon(&mut harness, "Battery: 11.80 V, 40%, warning");
    volts.set(11.2);
    labelled_soon(&mut harness, "Battery: 11.20 V, 10%, low");
    // Below empty is empty, not less.
    volts.set(10.4);
    labelled_soon(&mut harness, "Battery: 10.40 V, 0%, low");
}
