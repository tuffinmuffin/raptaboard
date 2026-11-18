//! `ElasticLayout::sanitize`: bad values are fixed, good layouts untouched.

// Tests written by Claude.

use raptaboard_layout::{ElasticLayout, Rect};

fn layout(grid_size: &str, containers: &str, layouts: &str) -> ElasticLayout {
    ElasticLayout::from_json(&format!(
        r#"{{"version": 1.0, "grid_size": {grid_size}, "tabs": [{{"name": "T",
            "grid_layout": {{"containers": [{containers}], "layouts": [{layouts}]}}}}]}}"#
    ))
    .unwrap()
}

fn widget(title: &str, x: f64, y: f64, width: f64, height: f64) -> String {
    format!(
        r#"{{"title": "{title}", "x": {x:?}, "y": {y:?}, "width": {width:?}, "height": {height:?},
            "type": "Text Display", "properties": {{"topic": "/a", "period": 0.06}}}}"#
    )
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn real_layouts_need_no_fixes() {
    let root = env!("CARGO_MANIFEST_DIR");
    for path in [
        format!("{root}/tests/data/elastic_sample.json"),
        format!("{root}/../../layouts/robot2027_sim.json"),
    ] {
        let original = ElasticLayout::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut cleaned = original.clone();
        assert_eq!(cleaned.sanitize(), Vec::<String>::new(), "{path}");
        assert_eq!(cleaned, original, "{path}");
        assert_eq!(cleaned.to_json(), original.to_json(), "{path}");
    }
}

#[test]
fn grid_size_must_be_sensible() {
    for (bad, fixed) in [
        ("0", 128),
        ("8", 128),
        ("5000", 128),
        ("16", 16),
        ("1024", 1024),
    ] {
        let mut l = layout(bad, "", "");
        let fixes = l.sanitize();
        assert_eq!(l.grid_size(), fixed, "grid_size {bad}");
        assert_eq!(
            fixes.len(),
            usize::from(bad != fixed.to_string()),
            "{fixes:?}"
        );
    }
    // A missing grid size means Elastic's default and is left missing.
    let mut l =
        ElasticLayout::from_json(r#"{"version": 1.0, "tabs": [{"name": "T", "grid_layout": {}}]}"#)
            .unwrap();
    assert!(l.sanitize().is_empty());
    assert_eq!(l.grid_size, None);
}

#[test]
fn a_layout_with_no_tabs_gets_the_defaults() {
    let mut l =
        ElasticLayout::from_json(r#"{"version": 1.0, "grid_size": 128, "tabs": []}"#).unwrap();
    assert_eq!(l.sanitize(), ["no tabs; added Teleoperated and Autonomous"]);
    let names: Vec<_> = l.tabs.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Teleoperated", "Autonomous"]);
}

#[test]
fn sizes_and_positions_are_fixed() {
    let containers = [
        widget("flat", 0.0, 0.0, 128.0, 0.0),
        widget("inside out", 256.0, 0.0, -128.0, -256.0),
        widget("left of the grid", -40.0, -1.0, 128.0, 128.0),
        widget("enormous", 0.0, 512.0, 1e12, 256.0),
    ]
    .join(",");
    let mut l = layout("128", &containers, "");
    let fixes = l.sanitize();
    assert_eq!(fixes.len(), 4, "{fixes:#?}");
    assert_eq!(
        fixes[0],
        r#"tab "T", widget "flat": 128x0 at (0, 0) became 128x128 at (0, 0)"#
    );
    let rects: Vec<Rect> = l.tabs[0]
        .grid_layout
        .containers
        .iter()
        .map(|w| w.rect)
        .collect();
    assert_eq!(
        rects,
        [
            rect(0.0, 0.0, 128.0, 128.0),
            rect(256.0, 0.0, 128.0, 128.0),
            rect(0.0, 0.0, 128.0, 128.0),
            // Cut to 100 cells wide.
            rect(0.0, 512.0, 12800.0, 256.0),
        ]
    );
}

#[test]
fn widgets_off_the_grid_move_to_a_free_spot() {
    let containers = [
        widget("home", 0.0, 0.0, 256.0, 128.0),
        widget("far away", 1e9, 0.0, 128.0, 128.0),
    ]
    .join(",");
    let list = r#"{"title": "list", "x": 384.0, "y": 0.0, "width": 128.0, "height": 128.0,
        "type": "List Layout", "properties": {}, "children": []}"#;
    let mut l = layout("128", &containers, list);
    let fixes = l.sanitize();
    assert_eq!(
        fixes,
        [r#"tab "T", widget "far away": off the grid at (1000000000, 0); moved to (256, 0)"#]
    );
    // The first free cell, reading row by row: right of "home", left of the list.
    assert_eq!(
        l.tabs[0].grid_layout.containers[1].rect,
        rect(256.0, 0.0, 128.0, 128.0)
    );
}

#[test]
fn list_children_only_get_size_fixes() {
    let list = format!(
        r#"{{"title": "list", "x": 0.0, "y": 0.0, "width": 256.0, "height": 512.0,
            "type": "List Layout", "properties": {{}},
            "children": [{}, {}]}}"#,
        // Elastic saves child positions inside the list; they aren't used.
        widget("ok", -5.0, -5.0, 256.0, 128.0),
        widget("no height", 0.0, 0.0, 256.0, 0.0),
    );
    let mut l = layout("128", "", &list);
    let fixes = l.sanitize();
    assert_eq!(fixes.len(), 1, "{fixes:#?}");
    assert!(fixes[0].starts_with(r#"tab "T", list "list", widget "no height""#));
    let children = &l.tabs[0].grid_layout.layouts[0].children;
    assert_eq!(children[0].rect, rect(-5.0, -5.0, 256.0, 128.0));
    assert_eq!(children[1].rect.height, 128.0);
}
