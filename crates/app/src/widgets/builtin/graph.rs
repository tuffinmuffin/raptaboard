//! Graph: numbers over time, drawn with `egui_plot`. One graph can show
//! several series, so values to compare sit on one plot; Elastic's graphs
//! show one.
//!
//! The first series is the widget's own `topic` (or struct field), saved as
//! Elastic saves a graph, so Elastic layouts open unchanged. More series go
//! in raptaboard's `series` setting: drop a topic from the tree onto a graph
//! to add one. A legend shows when there's more than one.
//!
//! Elastic's settings: `time_displayed` (seconds shown, 5; older files say
//! `visibleTime`), `min_value` and `max_value` (a fixed range; both unset
//! fits the values), `color` (the first series, cyan) and `line_width` (2).
//! raptaboard's: `series` and `show_legend`.

use eframe::egui::{self, Color32, Rect, Sense};
use egui_plot::{Corner, Legend, Line, Plot, PlotPoints};
use nt4::Value;
use raptaboard_layout::{StructMeta, WidgetContainer};
use serde_json::{Map, Value as Json};

use crate::traces::SeriesKey;
use crate::widgets::shared::card;
use crate::widgets::shared::format::number_of;
use crate::widgets::shared::props::{self, set_property};
use crate::widgets::{Live, ValueInfo, WidgetType, source_title};

/// Seconds of history shown (Elastic).
pub const TIME_DISPLAYED: &str = "time_displayed";
/// Elastic's older name for [`TIME_DISPLAYED`]; read, never written.
const VISIBLE_TIME: &str = "visibleTime";
/// The bottom of a fixed range (Elastic). Unset: fit the values.
pub const MIN_VALUE: &str = "min_value";
/// The top of a fixed range (Elastic). Unset: fit the values.
pub const MAX_VALUE: &str = "max_value";
/// The first series' color, as ARGB (Elastic).
pub const COLOR: &str = "color";
/// Line width in points (Elastic).
pub const LINE_WIDTH: &str = "line_width";
/// More series: a list of `{"topic", "data_type", "struct_meta"?, "color"?,
/// "name"?}` (raptaboard).
pub const SERIES: &str = "series";
/// Show the legend; unset shows it when there's more than one series
/// (raptaboard).
pub const SHOW_LEGEND: &str = "show_legend";

const KEYS: [&str; 6] = [
    TIME_DISPLAYED,
    MIN_VALUE,
    MAX_VALUE,
    LINE_WIDTH,
    SHOW_LEGEND,
    COLOR,
];

/// Elastic's default: 5 seconds shown.
pub const DEFAULT_TIME: f64 = 5.0;
/// Elastic's default line color (Material cyan 500).
pub const DEFAULT_COLOR: Color32 = Color32::from_rgb(0x00, 0xBC, 0xD4);
const DEFAULT_WIDTH: f64 = 2.0;
/// Colors for the series after the first, in order, until they repeat.
const PALETTE: [Color32; 8] = [
    Color32::from_rgb(0xFF, 0x98, 0x00),
    Color32::from_rgb(0x8B, 0xC3, 0x4A),
    Color32::from_rgb(0xE9, 0x1E, 0x63),
    Color32::from_rgb(0xFF, 0xEB, 0x3B),
    Color32::from_rgb(0x9C, 0x27, 0xB0),
    Color32::from_rgb(0x03, 0xA9, 0xF4),
    Color32::from_rgb(0xFF, 0x57, 0x22),
    Color32::from_rgb(0x9E, 0x9E, 0x9E),
];

/// The Graph widget (Elastic's `"Graph"`).
pub struct Graph;

/// One line on a graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// What it shows.
    pub key: SeriesKey,
    /// Its name in the legend.
    pub name: String,
    /// Its color.
    pub color: Color32,
}

/// Seconds of history `props` asks for, between 0.1 s and an hour.
pub fn time_displayed(props: &Map<String, Json>) -> f64 {
    let older = props::number(props, VISIBLE_TIME, DEFAULT_TIME);
    props::number(props, TIME_DISPLAYED, older).clamp(0.1, 3600.0)
}

/// The fixed range, if both ends are set and the bottom is below the top.
pub fn fixed_range(props: &Map<String, Json>) -> Option<(f64, f64)> {
    let low = props.get(MIN_VALUE)?.as_f64()?;
    let high = props.get(MAX_VALUE)?.as_f64()?;
    (low < high).then_some((low, high))
}

/// The series a graph widget shows: its own topic first, then any in its
/// `series` setting.
///
/// ```
/// use raptaboard::widgets::builtin::graph::series;
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 256.0, height: 256.0 };
/// let mut w = for_topic("/Drive/speed", "double", None, at).unwrap();
/// w.properties.insert("series".into(), serde_json::json!([{"topic": "/Drive/target"}]));
/// let names: Vec<_> = series(&w).into_iter().map(|s| s.name).collect();
/// assert_eq!(names, ["speed", "target"]);
/// ```
pub fn series(widget: &WidgetContainer) -> Vec<Series> {
    let mut all = Vec::new();
    if let Some(topic) = widget.topic() {
        let meta = widget.struct_meta();
        all.push(Series {
            key: SeriesKey {
                topic: topic.to_owned(),
                field: meta.as_ref().map(|m| m.path.clone()),
            },
            name: source_title(topic, meta.as_ref()),
            color: props::color_property(&widget.properties, COLOR).unwrap_or(DEFAULT_COLOR),
        });
    }
    let extra = widget
        .properties
        .get(SERIES)
        .and_then(Json::as_array)
        .into_iter()
        .flatten();
    for (i, entry) in extra.enumerate() {
        let Some(topic) = entry.get("topic").and_then(Json::as_str) else {
            continue;
        };
        let meta = entry.get("struct_meta").and_then(struct_meta_of);
        let name = entry
            .get("name")
            .and_then(Json::as_str)
            .map_or_else(|| source_title(topic, meta.as_ref()), str::to_owned);
        let color = entry
            .as_object()
            .and_then(|o| props::color_property(o, "color"))
            .unwrap_or(PALETTE[i % PALETTE.len()]);
        all.push(Series {
            key: SeriesKey {
                topic: topic.to_owned(),
                field: meta.map(|m| m.path),
            },
            name,
            color,
        });
    }
    all
}

/// A `struct_meta` object (Elastic's form), as a [`StructMeta`].
fn struct_meta_of(json: &Json) -> Option<StructMeta> {
    let path = json
        .get("path")?
        .as_array()?
        .iter()
        .map(|p| p.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    let schema_name = json.get("schema_name")?.as_str()?.to_owned();
    Some(StructMeta { path, schema_name })
}

/// Add a series to a graph: topic `topic` of NT type `nt_type`, or one of
/// its struct `field`s. Returns `false` (adding nothing) if the graph
/// already shows it.
///
/// ```
/// use raptaboard::widgets::builtin::graph::{add_series, series};
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 256.0, height: 256.0 };
/// let mut w = for_topic("/Drive/speed", "double", None, at).unwrap();
/// assert!(add_series(&mut w, "/Drive/target", "double", None));
/// assert!(!add_series(&mut w, "/Drive/speed", "double", None));
/// assert_eq!(series(&w).len(), 2);
/// ```
pub fn add_series(
    widget: &mut WidgetContainer,
    topic: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
) -> bool {
    let key = SeriesKey {
        topic: topic.to_owned(),
        field: field.map(|f| f.path.clone()),
    };
    if series(widget).iter().any(|s| s.key == key) {
        return false;
    }
    let mut entry = Map::new();
    entry.insert("topic".into(), topic.into());
    entry.insert("data_type".into(), nt_type.into());
    if let Some(field) = field {
        entry.insert("struct_meta".into(), field.to_json(nt_type));
    }
    let list = widget
        .properties
        .entry(SERIES)
        .or_insert_with(|| Json::Array(Vec::new()));
    match list.as_array_mut() {
        Some(list) => list.push(Json::Object(entry)),
        None => *list = Json::Array(vec![Json::Object(entry)]),
    }
    true
}

/// Remove the series at `index` in the `series` setting (the graph's own
/// topic can't be removed). The setting goes when it's empty.
pub fn remove_series(props: &mut Map<String, Json>, index: usize) {
    if let Some(list) = props.get_mut(SERIES).and_then(Json::as_array_mut)
        && index < list.len()
    {
        list.remove(index);
        if list.is_empty() {
            props.remove(SERIES);
        }
    }
}

/// Whether the legend shows: as set, or when there's more than one series.
fn show_legend(props: &Map<String, Json>, series: usize) -> bool {
    props::flag(props, SHOW_LEGEND, series > 1)
}

impl WidgetType for Graph {
    fn names(&self) -> &'static [&'static str] {
        &["Graph"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int" | "boolean")
    }

    fn show(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title)
            .shrink(card::BODY_MARGIN);
        let p = &widget.properties;
        let lines = series(widget);
        let seconds = time_displayed(p);
        let range = fixed_range(p);
        let width = props::number(p, LINE_WIDTH, DEFAULT_WIDTH).clamp(0.5, 20.0) as f32;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        // Hover only: the plot never takes the drags that move the widget.
        let mut plot = Plot::new("graph")
            .sense(Sense::hover())
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .allow_axis_zoom_drag(false)
            .show_background(false)
            .width(body.width())
            .height(body.height());
        if show_legend(p, lines.len()) {
            plot = plot.legend(Legend::default().position(Corner::LeftTop));
        }
        plot.show(&mut child, |plot_ui| {
            plot_ui.set_plot_bounds_x(-seconds..=0.0);
            match range {
                Some((low, high)) => plot_ui.set_plot_bounds_y(low..=high),
                None => plot_ui.set_auto_bounds([false, true]),
            }
            for line in lines {
                let points: Vec<[f64; 2]> = live
                    .traces
                    .points(&line.key)
                    .map(|(t, v)| [(t - live.now_us) as f64 / 1e6, v])
                    .collect();
                plot_ui.line(
                    Line::new(line.name, PlotPoints::new(points))
                        .color(line.color)
                        .width(width),
                );
            }
        });
    }

    /// Without a full Ui (in a List Layout, or while dragged): the newest
    /// value as text.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match value.and_then(number_of) {
            Some(v) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{v:.2}"),
                    egui::FontId::monospace(16.0),
                    visuals.text_color(),
                );
            }
            None => card::draw_message(painter, visuals, rect, "no value"),
        }
    }

    fn extra_topics(&self, props: &Map<String, Json>) -> Vec<String> {
        props
            .get(SERIES)
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| s.get("topic").and_then(Json::as_str).map(str::to_owned))
            .collect()
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::number_row(
            ui,
            p,
            "Time shown (s)",
            TIME_DISPLAYED,
            DEFAULT_TIME,
            0.1..=3600.0,
        );

        ui.label("Range");
        ui.horizontal(|ui| {
            let mut fixed = fixed_range(p).is_some();
            if ui
                .checkbox(&mut fixed, "Fixed")
                .on_hover_text("Off: fit the values shown")
                .changed()
            {
                let (low, high) = if fixed {
                    (Some(-1.0), Some(1.0))
                } else {
                    (None, None)
                };
                set_property(p, MIN_VALUE, low.map(Json::from));
                set_property(p, MAX_VALUE, high.map(Json::from));
            }
            if let Some((mut low, mut high)) = fixed_range(p) {
                let a = ui.add(egui::DragValue::new(&mut low).speed(0.1).prefix("from "));
                let b = ui.add(egui::DragValue::new(&mut high).speed(0.1).prefix("to "));
                if (a.changed() || b.changed()) && low < high {
                    set_property(p, MIN_VALUE, Some(low.into()));
                    set_property(p, MAX_VALUE, Some(high.into()));
                }
            }
        });
        ui.end_row();

        props::number_row(ui, p, "Line width", LINE_WIDTH, DEFAULT_WIDTH, 0.5..=20.0);
        props::color_row(ui, p, "Color", COLOR, DEFAULT_COLOR);

        // The other series, with their colors and a way to remove them.
        let count = p.get(SERIES).and_then(Json::as_array).map_or(0, Vec::len);
        props::flag_row(ui, p, "Legend", "Show the legend", SHOW_LEGEND, count > 0);
        ui.label("More series");
        ui.vertical(|ui| {
            if count == 0 {
                ui.weak("Drop topics from the tree onto the graph to add them.");
            }
            let mut remove = None;
            for i in 0..count {
                let Some(entry) = p
                    .get_mut(SERIES)
                    .and_then(Json::as_array_mut)
                    .and_then(|l| l.get_mut(i))
                    .and_then(Json::as_object_mut)
                else {
                    continue;
                };
                ui.horizontal(|ui| {
                    let mut color =
                        props::color_property(entry, "color").unwrap_or(PALETTE[i % PALETTE.len()]);
                    if egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    )
                    .changed()
                    {
                        entry.insert("color".into(), props::color_json(color));
                    }
                    let topic = entry.get("topic").and_then(Json::as_str).unwrap_or("?");
                    let meta = entry.get("struct_meta").and_then(struct_meta_of);
                    ui.label(source_title(topic, meta.as_ref()))
                        .on_hover_text(topic);
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                remove_series(p, i);
            }
        });
        ui.end_row();
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        // `series` isn't here: Reset settings keeps the lines shown.
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use raptaboard_layout::Rect as LayoutRect;

    fn graph() -> WidgetContainer {
        let at = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 256.0,
        };
        let mut w = crate::widgets::for_topic("/Drive/speed", "double", None, at).unwrap();
        w.widget_type = "Graph".into();
        w
    }

    #[test]
    fn elastic_graph_settings_read_back() {
        let mut w = graph();
        assert_eq!(time_displayed(&w.properties), DEFAULT_TIME);
        assert_eq!(fixed_range(&w.properties), None);
        // Elastic's older name for the time window.
        w.properties.insert(VISIBLE_TIME.into(), 10.0.into());
        assert_eq!(time_displayed(&w.properties), 10.0);
        w.properties.insert(TIME_DISPLAYED.into(), 3.0.into());
        assert_eq!(time_displayed(&w.properties), 3.0);
        // A range needs both ends, the right way round.
        w.properties.insert(MIN_VALUE.into(), 0.0.into());
        assert_eq!(fixed_range(&w.properties), None);
        w.properties.insert(MAX_VALUE.into(), 12.0.into());
        assert_eq!(fixed_range(&w.properties), Some((0.0, 12.0)));
        w.properties.insert(MAX_VALUE.into(), (-1.0).into());
        assert_eq!(fixed_range(&w.properties), None);
    }

    #[test]
    fn series_are_added_once_and_removed() {
        let mut w = graph();
        let x = StructMeta {
            path: vec!["translation".into(), "x".into()],
            schema_name: "Pose2d".into(),
        };
        assert!(add_series(&mut w, "/Drive/pose", "double", Some(&x)));
        assert!(add_series(&mut w, "/Drive/target", "double", None));
        assert!(!add_series(&mut w, "/Drive/pose", "double", Some(&x)));
        let all = series(&w);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["speed", "pose.translation.x", "target"]);
        // Each has its own color: cyan, then the palette.
        assert_eq!(all[0].color, DEFAULT_COLOR);
        assert_eq!(all[1].color, PALETTE[0]);
        assert_eq!(all[2].color, PALETTE[1]);
        assert_eq!(all[1].key.field, Some(x.path.clone()));
        assert_eq!(
            Graph.extra_topics(&w.properties),
            ["/Drive/pose", "/Drive/target"]
        );
        // The legend shows itself once there's more than one.
        assert!(show_legend(&w.properties, all.len()));
        remove_series(&mut w.properties, 0);
        remove_series(&mut w.properties, 0);
        assert!(!w.properties.contains_key(SERIES));
        assert!(!show_legend(&w.properties, series(&w).len()));
    }

    #[test]
    fn a_graph_draws_its_lines_and_legend() {
        let mut w = graph();
        add_series(&mut w, "/Drive/target", "double", None);
        let mut traces = crate::traces::Traces::default();
        for line in series(&w) {
            traces.want(&line.key, 5.0);
            for i in 0..20 {
                traces.record(&line.key, i * 100_000, (i as f64).sin());
            }
        }
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
            let live = Live {
                value: None,
                traces: &traces,
                now_us: 2_000_000,
            };
            Graph.show(ui, rect, &w, &live);
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        let mut paths = 0;
        fn walk(shape: &egui::Shape, texts: &mut Vec<String>, paths: &mut usize) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
                egui::Shape::Path(p) if p.points.len() > 2 => *paths += 1,
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts, paths)),
                _ => {}
            }
        }
        for s in &output.shapes {
            walk(&s.shape, &mut texts, &mut paths);
        }
        // The title, both names in the legend, and a line for each.
        for name in ["speed", "target"] {
            assert!(texts.iter().any(|t| t == name), "{name} in {texts:?}");
        }
        assert!(paths >= 2, "{paths} lines drawn");
    }
}
