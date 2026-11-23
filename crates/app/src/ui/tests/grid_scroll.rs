//! Scrolling the grid: to widgets past the edge of the window, and by
//! dragging a widget to the edge.

// Tests written by Claude.

use super::*;

/// One widget near the top left, and with `far`, one far past the
/// window's edge.
fn harness_with(robot: &Robot, far: bool) -> Harness {
    let mut harness = harness_for(robot);
    let at = |x: f64, y: f64| LayoutRect {
        x,
        y,
        width: 256.0,
        height: 128.0,
    };
    let mut tab = Tab::new("Wide");
    tab.grid_layout.containers = vec![widget(
        "Text Display",
        "/Test/number",
        "double",
        at(0.0, 0.0),
    )];
    if far {
        tab.grid_layout.containers.push(widget(
            "Text Display",
            "/Test/count",
            "int",
            at(3200.0, 2400.0),
        ));
    }
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.locked = false;
    settle(&mut harness);
    harness
}

/// Widget `index`'s rect on screen now.
fn on_screen(harness: &Harness, index: usize) -> Rect {
    let app = harness.state();
    let r = app.layout.tabs[0].grid_layout.containers[index].rect;
    Rect::from_min_size(
        app.grid.area.min + egui::vec2(r.x as f32, r.y as f32),
        egui::vec2(r.width as f32, r.height as f32),
    )
}

/// Turn the mouse wheel over `at` by `delta` points (negative: toward the
/// bottom right).
fn wheel(harness: &mut Harness, at: Pos2, delta: egui::Vec2) {
    harness.hover_at(at);
    harness.step();
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    });
    settle(harness);
}

#[test]
fn a_widget_past_the_edge_is_scrolled_to_with_the_wheel() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot, true);
    let visible = harness.state().grid.visible;
    assert!(
        !visible.intersects(on_screen(&harness, 1)),
        "starts off screen"
    );
    // The wheel over the empty grid scrolls it, down and right.
    let empty = visible.center() + egui::vec2(100.0, 100.0);
    for _ in 0..10 {
        wheel(&mut harness, empty, egui::vec2(-500.0, -500.0));
    }
    let visible = harness.state().grid.visible;
    assert!(
        visible.contains_rect(on_screen(&harness, 1)),
        "the far widget shows: {visible:?}, {:?}",
        on_screen(&harness, 1)
    );
    // Not past it: the grid ends where the far widget does.
    assert!(on_screen(&harness, 1).max.x <= visible.max.x + 1.0);
    assert!(visible.max.x - on_screen(&harness, 1).max.x < 2.0);
}

#[test]
fn dragging_a_widget_to_the_edge_scrolls_and_moves_it_past_the_window() {
    let robot = Robot::start();
    // Nothing past the edge: the grid grows as the widget is dragged.
    let mut harness = harness_with(&robot, false);
    let visible = harness.state().grid.visible;
    // Pick widget 0 up by its title bar and hold it at the right edge.
    let title = on_screen(&harness, 0).center_top() + egui::vec2(0.0, 8.0);
    let edge = Pos2::new(visible.max.x - 4.0, title.y);
    harness.hover_at(title);
    harness.step();
    harness.drag_at(title);
    harness.step();
    for step in 1..=10 {
        harness.hover_at(title + (edge - title) * (step as f32 / 10.0));
        harness.step();
    }
    for _ in 0..60 {
        harness.hover_at(edge);
        harness.step();
    }
    let scrolled = harness
        .state()
        .grid
        .scroll
        .get(&0)
        .copied()
        .unwrap_or_default();
    assert!(scrolled.x > 200.0, "scrolled right: {scrolled:?}");
    harness.drop_at(edge);
    settle(&mut harness);
    // Dropped where it was let go: wholly past the window's first width,
    // not stopped at it.
    let x = harness.state().layout.tabs[0].grid_layout.containers[0]
        .rect
        .x;
    assert!(
        x > f64::from(visible.width()),
        "moved right to {x} (the window shows {})",
        visible.width()
    );
}

#[test]
fn with_everything_on_screen_the_grid_does_not_scroll() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot, false);
    let empty = harness.state().grid.visible.center() + egui::vec2(100.0, 100.0);
    for _ in 0..5 {
        wheel(&mut harness, empty, egui::vec2(-500.0, -500.0));
    }
    let scrolled = harness
        .state()
        .grid
        .scroll
        .get(&0)
        .copied()
        .unwrap_or_default();
    assert_eq!(scrolled, egui::Vec2::ZERO);
}

#[test]
fn a_locked_grid_does_not_scroll() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot, true);
    harness.state_mut().locked = true;
    settle(&mut harness);
    let empty = harness.state().grid.visible.center() + egui::vec2(100.0, 100.0);
    for _ in 0..5 {
        wheel(&mut harness, empty, egui::vec2(-500.0, -500.0));
    }
    let scrolled = |h: &Harness| h.state().grid.scroll.get(&0).copied().unwrap_or_default();
    assert_eq!(scrolled(&harness), egui::Vec2::ZERO, "locked");
    // Unlocked, the same wheel scrolls it.
    harness.state_mut().locked = false;
    settle(&mut harness);
    wheel(&mut harness, empty, egui::vec2(-500.0, -500.0));
    assert!(
        scrolled(&harness).y > 0.0,
        "unlocked: {:?}",
        scrolled(&harness)
    );
}

#[test]
fn the_wheel_over_a_paused_graph_moves_the_graph_not_the_grid() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot, true);
    // Widget 0 becomes a graph, and is paused.
    harness.state_mut().layout.tabs[0].grid_layout.containers[0].widget_type = "Graph".into();
    settle(&mut harness);
    harness.get_by_label("⏸").click();
    settle(&mut harness);
    assert!(harness.query_by_label("▶").is_some(), "paused");
    let scrolled = |h: &Harness| h.state().grid.scroll.get(&0).copied().unwrap_or_default();
    let graph = on_screen(&harness, 0).center() + egui::vec2(0.0, 10.0);
    wheel(&mut harness, graph, egui::vec2(0.0, -500.0));
    assert_eq!(scrolled(&harness), egui::Vec2::ZERO, "the grid stayed");
    // Off the graph, the same wheel scrolls the grid.
    let empty = harness.state().grid.visible.center() + egui::vec2(100.0, 100.0);
    wheel(&mut harness, empty, egui::vec2(0.0, -500.0));
    assert!(scrolled(&harness).y > 0.0, "{:?}", scrolled(&harness));
}
