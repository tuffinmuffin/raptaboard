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
    assert_eq!(fixes.len(), 5, "{fixes:#?}");
    assert_eq!(
        fixes[0],
        r#"tab "T", widget "flat": 128x0 at (0, 0) became 128x128 at (0, 0)"#
    );
    // Moving "left of the grid" onto the grid put it on top of "flat", so it
    // then moves to the free cell beside it.
    assert_eq!(
        fixes[4],
        r#"tab "T", widget "left of the grid": covers widget "flat"; moved from (0, 0) to (128, 0)"#
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
            rect(128.0, 0.0, 128.0, 128.0),
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

#[test]
fn overlapping_items_move_to_the_nearest_free_spot() {
    let containers = [
        widget("a", 0.0, 0.0, 256.0, 128.0),
        // Half on top of "a": moves right next to it, the nearest free spot.
        widget("b", 128.0, 0.0, 256.0, 128.0),
        widget("c", 0.0, 128.0, 128.0, 128.0),
    ]
    .join(",");
    // A list over "c": lists are checked after widgets, so the list moves.
    let list = r#"{"title": "list", "x": 0.0, "y": 128.0, "width": 128.0, "height": 256.0,
        "type": "List Layout", "properties": {}, "children": []}"#;
    let mut l = layout("128", &containers, list);
    let fixes = l.sanitize();
    assert_eq!(
        fixes,
        [
            r#"tab "T", widget "b": covers widget "a"; moved from (128, 0) to (256, 0)"#,
            r#"tab "T", list "list": covers widget "c"; moved from (0, 128) to (128, 128)"#,
        ]
    );
    let grid = &l.tabs[0].grid_layout;
    assert_eq!(grid.containers[1].rect, rect(256.0, 0.0, 256.0, 128.0));
    assert_eq!(grid.layouts[0].rect, rect(128.0, 128.0, 128.0, 256.0));
    // Fixed once, nothing left to fix.
    let mut again = l.clone();
    assert!(again.sanitize().is_empty());
    assert_eq!(again, l);
}

#[test]
fn identical_widgets_are_spread_out() {
    // Three widgets stacked on one spot, as a script might write them.
    let containers = [0, 1, 2]
        .map(|i| widget(&format!("w{i}"), 128.0, 128.0, 128.0, 128.0))
        .join(",");
    let mut l = layout("128", &containers, "");
    assert_eq!(l.sanitize().len(), 2);
    let spots: Vec<Rect> = l.tabs[0]
        .grid_layout
        .containers
        .iter()
        .map(|w| w.rect)
        .collect();
    // The first stays; the others go to the nearest free cells: above, then left.
    assert_eq!(
        spots,
        [
            rect(128.0, 128.0, 128.0, 128.0),
            rect(128.0, 0.0, 128.0, 128.0),
            rect(0.0, 128.0, 128.0, 128.0),
        ]
    );
}

#[test]
fn an_overlap_with_no_room_is_left_and_reported() {
    // Grid size 1024 with 100 cells is a 102400-pixel grid; fill it with one
    // giant widget so nothing else fits anywhere.
    let containers = [
        widget("everything", 0.0, 0.0, 102400.0, 102400.0),
        widget("small", 0.0, 0.0, 1024.0, 1024.0),
    ]
    .join(",");
    let mut l = layout("1024", &containers, "");
    assert_eq!(
        l.sanitize(),
        [
            r#"tab "T", widget "small": covers widget "everything", and there's no free space; left there"#
        ]
    );
    assert_eq!(
        l.tabs[0].grid_layout.containers[1].rect,
        rect(0.0, 0.0, 1024.0, 1024.0)
    );
}

#[test]
fn allowed_partial_overlaps_are_kept_and_others_moved() {
    // "label" may overlap and sits on half of "camera": kept as it is.
    let label = r#"{"title": "label", "x": 128.0, "y": 0.0, "width": 128.0, "height": 128.0,
        "type": "Text Display", "properties": {"may_overlap": true}}"#;
    let camera = widget("camera", 0.0, 0.0, 256.0, 128.0);
    let mut l = layout("128", &format!("{camera}, {label}"), "");
    assert!(l.sanitize().is_empty());
    assert_eq!(
        l.tabs[0].grid_layout.containers[1].rect,
        rect(128.0, 0.0, 128.0, 128.0)
    );

    // One that may overlap but would hide "small" completely: moved.
    let small = widget("small", 0.0, 0.0, 128.0, 128.0);
    let cover = r#"{"title": "cover", "x": 0.0, "y": 0.0, "width": 256.0, "height": 128.0,
        "type": "Text Display", "properties": {"may_overlap": true}}"#;
    let mut l = layout("128", &format!("{small}, {cover}"), "");
    let fixes = l.sanitize();
    assert_eq!(fixes.len(), 1, "{fixes:?}");
    assert!(fixes[0].contains("covers widget \"small\""), "{fixes:?}");
    let moved = l.tabs[0].grid_layout.containers[1].rect;
    assert!(!moved.overlaps(&rect(0.0, 0.0, 128.0, 128.0)));
}

fn group(children: &[String]) -> String {
    format!(
        r#"{{"title": "g", "x": 0.0, "y": 0.0, "width": 512.0, "height": 256.0,
            "type": "Group", "properties": {{"rows": 2}}, "children": [{}]}}"#,
        children.join(", ")
    )
}

#[test]
fn a_groups_widgets_are_kept_on_its_cells() {
    // A good group is left alone.
    let good = group(&[
        widget("a", 0.0, 0.0, 256.0, 128.0),
        widget("b", 256.0, 0.0, 128.0, 256.0),
    ]);
    let original = layout("128", "", &good);
    let mut l = original.clone();
    assert_eq!(l.sanitize(), Vec::<String>::new());
    assert_eq!(l, original);
    // Unlike a list's, a group's widgets have places, fixed like the tab's,
    // and on whole cells (the group's 128 pixels, not the tab's 64).
    let bad = group(&[
        widget("negative", -128.0, -5.0, 128.0, 128.0),
        widget("no size", 256.0, 0.0, 0.0, 128.0),
        widget("between", 400.0, 120.0, 100.0, 140.0),
        widget("far", 128.0 * 30.0, 0.0, 128.0, 128.0),
    ]);
    let mut l = layout("64", "", &bad);
    let fixes = l.sanitize();
    assert!(
        fixes
            .iter()
            .all(|f| f.starts_with(r#"tab "T", group "g", widget ""#)),
        "{fixes:#?}"
    );
    let at = |i: usize| l.tabs[0].grid_layout.layouts[0].children[i].rect;
    assert_eq!(at(0), rect(0.0, 0.0, 128.0, 128.0));
    assert_eq!(at(1), rect(256.0, 0.0, 128.0, 128.0));
    assert_eq!(at(2), rect(384.0, 128.0, 128.0, 128.0));
    // Past the most cells a group has: back at its edge.
    assert_eq!(at(3), rect(128.0 * 23.0, 0.0, 128.0, 128.0));
}

#[test]
fn widgets_overlapping_in_a_group_move_even_if_they_may_overlap() {
    // "b" may overlap (it was Alt-dropped on the tab once), but nothing
    // overlaps inside a group: it moves to the nearest free cell.
    let mut b = widget("b", 128.0, 0.0, 128.0, 128.0);
    b = b.replace(
        r#""period": 0.06"#,
        r#""period": 0.06, "may_overlap": true"#,
    );
    let g = group(&[widget("a", 0.0, 0.0, 256.0, 128.0), b]);
    let mut l = layout("128", "", &g);
    let fixes = l.sanitize();
    assert_eq!(fixes.len(), 1, "{fixes:#?}");
    assert!(
        fixes[0].starts_with(r#"tab "T", group "g", widget "b": covers widget "a"; moved"#),
        "{fixes:#?}"
    );
    let children = &l.tabs[0].grid_layout.layouts[0].children;
    assert_eq!(children[0].rect, rect(0.0, 0.0, 256.0, 128.0));
    assert!(!children[1].rect.overlaps(&children[0].rect));
    assert_eq!(children[1].rect, rect(256.0, 0.0, 128.0, 128.0));
}
