// Tests written by Claude: check that an Elastic layout survives a load and save.

use raptaboard_layout::{ElasticLayout, LayoutError};
use serde_json::Value;

const SAMPLE: &str = include_str!("data/elastic_sample.json");

#[test]
fn sample_round_trips_without_losing_fields() {
    let layout = ElasticLayout::from_json(SAMPLE).unwrap();
    let written: Value = serde_json::from_str(&layout.to_json()).unwrap();
    let original: Value = serde_json::from_str(SAMPLE).unwrap();
    assert_eq!(written, original);
}

#[test]
fn widgets_include_list_layout_children() {
    let layout = ElasticLayout::from_json(SAMPLE).unwrap();
    let topics: Vec<_> = layout.widgets().filter_map(|w| w.topic()).collect();
    assert_eq!(
        topics,
        [
            "/Drive/Field",
            "/Tunables/Auto",
            "/FrameTiming/loop_ms",
            "/Drive/Health/allModulesHealthy"
        ]
    );
}

#[test]
fn unknown_fields_are_kept() {
    let layout = ElasticLayout::from_json(SAMPLE).unwrap();
    let graph = &layout.tabs[0].grid_layout.containers[2];
    assert_eq!(graph.extra["future_field"], "kept");
    assert_eq!(graph.period(), Some(0.033));
}

#[test]
fn version_two_is_rejected() {
    let err = ElasticLayout::from_json(r#"{"version":2.0,"tabs":[]}"#).unwrap_err();
    assert!(matches!(err, LayoutError::Version(v) if v == 2.0));
}
