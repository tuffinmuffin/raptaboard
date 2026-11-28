//! The NT tree: picking rows, dragging topics and struct fields onto the grid, a graph and lists, and its right-click menu.

// Tests written by Claude.

use super::*;

#[test]
fn tree_rows_select_with_click_ctrl_and_shift() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Test  (");
    let none = Modifiers::NONE;
    click_row(&mut harness, "count", 0, none);
    assert_eq!(picked(&harness), ["/Test/count"]);
    // Ctrl (Cmd) adds one; clicking it again takes it out.
    click_row(&mut harness, "number", 0, Modifiers::COMMAND);
    assert_eq!(picked(&harness), ["/Test/count", "/Test/number"]);
    click_row(&mut harness, "number", 0, Modifiers::COMMAND);
    assert_eq!(picked(&harness), ["/Test/count"]);
    // Shift takes the rows from the last one clicked (Ctrl clicks count
    // too), in name order: count, enabled, flag, level, mode,
    // modeOptions, number.
    click_row(&mut harness, "count", 0, none);
    click_row(&mut harness, "number", 0, Modifiers::SHIFT);
    assert_eq!(
        picked(&harness),
        [
            "/Test/count",
            "/Test/enabled",
            "/Test/flag",
            "/Test/level",
            "/Test/mode",
            "/Test/modeOptions",
            "/Test/number"
        ]
    );
    // And upwards, from the last click: number back to color.
    click_row(&mut harness, "number", 0, none);
    click_row(&mut harness, "color", 0, Modifiers::SHIFT);
    assert_eq!(
        picked(&harness),
        [
            "/Test/color",
            "/Test/colors",
            "/Test/count",
            "/Test/enabled",
            "/Test/flag",
            "/Test/level",
            "/Test/mode",
            "/Test/modeOptions",
            "/Test/number"
        ]
    );
    // A plain click picks just that row.
    click_row(&mut harness, "flag", 0, none);
    assert_eq!(picked(&harness), ["/Test/flag"]);
    // A click on empty grid clears it.
    click_at(
        &mut harness,
        Pos2::new(1700.0, 1250.0),
        egui::PointerButton::Primary,
        none,
    );
    assert!(picked(&harness).is_empty());
}

#[test]
fn nested_struct_fields_and_struct_array_items_select() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let none = Modifiers::NONE;
    // A field inside a nested struct: pose → translation → x.
    click_row(&mut harness, "x", 0, none);
    assert_eq!(picked(&harness), ["/Drive/pose translation.x"]);
    // Another branch of the same struct, added: pose → rotation → value
    // (the third "value": both modules come first).
    click_row(&mut harness, "value", 2, Modifiers::COMMAND);
    assert_eq!(
        picked(&harness),
        ["/Drive/pose translation.x", "/Drive/pose rotation.value"]
    );
    // A field of one item of a struct array: modules → [1] → speed.
    click_row(&mut harness, "speed", 1, none);
    assert_eq!(picked(&harness), ["/Drive/modules [1].speed"]);
    // A struct nested in an array item: modules → [0] → angle → value.
    click_row(&mut harness, "value", 0, none);
    assert_eq!(picked(&harness), ["/Drive/modules [0].angle.value"]);
    // Ctrl across topics: the same field of the other item.
    click_row(&mut harness, "value", 1, Modifiers::COMMAND);
    assert_eq!(
        picked(&harness),
        [
            "/Drive/modules [0].angle.value",
            "/Drive/modules [1].angle.value"
        ]
    );
}

#[test]
fn struct_fields_drag_onto_the_grid_as_field_widgets() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    // A nested field, onto empty grid right of the test widgets (they're
    // five across).
    drag_row(&mut harness, "y", 0, Pos2::new(1700.0, 1150.0));
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.topic(), Some("/Drive/pose"));
    let meta = made.struct_meta().expect("bound to a field");
    assert_eq!(meta.path, ["translation", "y"]);
    assert_eq!(meta.schema_name, "Pose2d");
    // A field of a struct array item, somewhere else.
    drag_row(&mut harness, "speed", 1, Pos2::new(1700.0, 900.0));
    assert_eq!(widgets(&harness).len(), before + 2);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.topic(), Some("/Drive/modules"));
    assert_eq!(made.struct_meta().unwrap().path, ["[1]", "speed"]);
    // Dragging a selected row takes the whole selection with it.
    click_row(&mut harness, "x", 0, Modifiers::NONE);
    click_row(&mut harness, "value", 2, Modifiers::COMMAND);
    drag_row(&mut harness, "x", 0, Pos2::new(1700.0, 650.0));
    assert_eq!(widgets(&harness).len(), before + 4);
}

#[test]
fn topics_dropped_on_a_graph_become_its_lines() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let index = widgets(&harness)
        .iter()
        .position(|w| w.widget_type == "Graph")
        .expect("the test layout has a graph");
    let on_graph = {
        let app = harness.state();
        let r = widgets(&harness)[index].rect;
        app.grid.area.min + egui::vec2((r.x + r.width / 2.0) as f32, (r.y + r.height / 2.0) as f32)
    };
    let count = widgets(&harness).len();
    let lines = |h: &Harness| graph::series(&widgets(h)[index]);
    let before = lines(&harness).len();
    // A struct field dropped on the graph is one more line, not a widget.
    drag_row(&mut harness, "y", 0, on_graph);
    assert_eq!(widgets(&harness).len(), count);
    let now = lines(&harness);
    assert_eq!(now.len(), before + 1);
    assert_eq!(
        now.last().unwrap().key.field.as_deref(),
        Some(&["translation".to_string(), "y".to_string()][..])
    );
    assert!(harness.state().status.starts_with("Added a line"));
    // The same again adds nothing, and says so.
    drag_row(&mut harness, "y", 0, on_graph);
    assert_eq!(lines(&harness).len(), before + 1);
    assert!(harness.state().status.starts_with("Nothing new"));
}

#[test]
fn the_right_click_menu_adds_the_selection_and_locking_stops_it() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let count = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.len();
    let before = count(&harness);
    // Select two fields in different structs, then right-click one.
    click_row(&mut harness, "x", 0, Modifiers::NONE);
    click_row(&mut harness, "speed", 0, Modifiers::COMMAND);
    right_click_row(&mut harness, "x", 0);
    harness.get_by_label("Add 2 to grid").click();
    settle(&mut harness);
    assert_eq!(count(&harness), before + 2);
    // Right-clicking a row that isn't selected picks just it.
    right_click_row(&mut harness, "y", 0);
    assert_eq!(picked(&harness), ["/Drive/pose translation.y"]);
    // Into a new list, through the submenu.
    harness.get_by_label("Add to list").hover();
    settle(&mut harness);
    harness.get_by_label("New list").click();
    settle(&mut harness);
    let lists = harness.state().layout.tabs[0].grid_layout.layouts.clone();
    assert_eq!(lists.len(), 2);
    assert_eq!(
        lists[1].children[0].struct_meta().unwrap().path,
        ["translation", "y"]
    );
    // Locked: rows still select, but there's no menu and no dragging.
    harness.state_mut().locked = true;
    settle(&mut harness);
    right_click_row(&mut harness, "x", 0);
    assert_eq!(picked(&harness), ["/Drive/pose translation.x"]);
    assert!(harness.query_by_label("Add to grid").is_none());
    harness.key_press(Key::Escape);
    settle(&mut harness);
    let before = count(&harness);
    drag_row(&mut harness, "y", 0, Pos2::new(1500.0, 1150.0));
    assert_eq!(count(&harness), before);
}

#[test]
fn add_as_makes_any_type_that_shows_the_topic_and_sets_it_up() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Test  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    // A string topic as an Enum Dropdown: set up as the settings dialog
    // sets up a type picked there, its names found in /Test/modeOptions.
    add_as(&mut harness, "mode", 0, "Enum Dropdown");
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Enum Dropdown");
    assert_eq!(made.topic(), Some("/Test/mode"));
    assert_eq!(made.properties["enum_source"], "topic");
    assert_eq!(made.properties["options_topic"], "/Test/modeOptions");
    // A number as a Graph, with a graph's own update period.
    add_as(&mut harness, "number", 0, "Graph");
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Graph");
    assert_eq!(
        made.period(),
        crate::widgets::find("Graph").map(|g| g.default_period())
    );
}

#[test]
fn a_topic_dropped_on_a_widget_pushes_it_aside() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Test  (");
    let grid = |h: &Harness| h.state().layout.tabs[0].grid_layout.clone();
    let first = grid(&harness).containers[0].clone();
    assert_eq!((first.rect.x, first.rect.y), (0.0, 0.0));
    let before = grid(&harness).containers.len();
    // Onto the first widget's top-left cell.
    let to = harness.state().grid.area.min + egui::vec2(64.0, 64.0);
    drag_row(&mut harness, "flag", 0, to);
    let after = grid(&harness);
    assert_eq!(after.containers.len(), before + 1);
    let made = after.containers.last().unwrap();
    assert_eq!(made.topic(), Some("/Test/flag"));
    assert_eq!((made.rect.x, made.rect.y), (0.0, 0.0));
    // The widget that was there moved, and nothing covers anything.
    // (Still first in the list: new widgets go on the end.)
    let moved = &after.containers[0];
    assert_ne!(moved.rect, first.rect);
    let rects: Vec<_> = crate::grid::items(&after)
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            assert!(!a.overlaps(b), "{a:?} on {b:?}");
        }
    }
}

#[test]
fn a_topic_dropped_on_a_group_goes_into_the_cell_under_the_pointer() {
    use crate::widgets::group;
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    // Just a group on the tab, four cells by two.
    let at = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 512.0,
        height: 256.0,
    };
    let app = harness.state_mut();
    app.layout.tabs[0].grid_layout = raptaboard_layout::GridLayout::default();
    app.layout.tabs[0]
        .grid_layout
        .layouts
        .push(group::new_group(at));
    settle(&mut harness);
    open(&mut harness, "Test  (");
    let area = harness.state().grid.area;
    let card = Rect::from_min_size(area.min, egui::vec2(512.0, 256.0)).shrink(4.0);
    let grid = harness.state().layout.tabs[0].grid_layout.clone();
    let m = group::Mapping::of(card, &grid.layouts[0]);
    let third = LayoutRect {
        x: 2.0 * group::INNER_CELL,
        y: group::INNER_CELL,
        width: group::INNER_CELL,
        height: group::INNER_CELL,
    };
    drag_row(&mut harness, "flag", 0, m.to_screen(&third).center());
    let grid = harness.state().layout.tabs[0].grid_layout.clone();
    assert!(grid.containers.is_empty(), "not onto the grid");
    let inside = &grid.layouts[0].children;
    assert_eq!(inside.len(), 1);
    assert_eq!(inside[0].topic(), Some("/Test/flag"));
    assert_eq!(inside[0].rect, third);
}
