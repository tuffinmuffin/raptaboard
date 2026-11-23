//! Graph: numbers over time, drawn with `egui_plot`. One graph can show
//! several lines, so values to compare sit on one plot; Elastic's graphs
//! show one.
//!
//! All lines are equal: the first is the **main** line, saved in the
//! widget's own `topic` (or struct field) and `color` as Elastic saves a
//! graph, so Elastic layouts open unchanged; the rest go in raptaboard's
//! `series` setting. Drop a topic from the tree onto a graph to add a line;
//! reorder lines in the settings to change which is main. A legend shows
//! when there's more than one.
//!
//! A graph can be paused (the ⏸ button): the picture freezes while values
//! keep being recorded. While paused, clicking the plot places measuring
//! cursors, A then B (with only A, clicking A takes it away; with both,
//! any click clears them), and a readout gives the
//! time between them and each line's values there, like a scope's cursors.
//! Paused, the plot can also look back through everything recorded (the
//! `history_kept` setting, a minute unless set): the mouse wheel or a drag
//! along it moves through time, and Ctrl (⌘ on a Mac) with the wheel zooms.
//! Lines are thinned to about two points per pixel column before drawing,
//! so minutes of values draw quickly; the cursors and hover read every
//! value.
//!
//! Elastic's settings: `time_displayed` (seconds shown, 5; older files say
//! `visibleTime`), `min_value` and `max_value` (a fixed range; both unset
//! fits the values), `color` (the main line, cyan) and `line_width` (2).
//! raptaboard's: `series`, `show_legend`, `show_pause`, `show_hover` and
//! `history_kept`.

use std::sync::Arc;

use eframe::egui::{self, Color32, Rect, Sense};
use egui_plot::{Corner, Legend, Line, Plot, PlotPoints, Points, VLine};
use nt4::Value;
use raptaboard_layout::{StructMeta, WidgetContainer};
use serde_json::{Map, Value as Json};

use crate::traces::SeriesKey;
use crate::tree::DraggedTopic;
use crate::widgets::shared::card;
use crate::widgets::shared::format::number_of;
use crate::widgets::shared::props::{self, DATA_TYPE, set_property};
use crate::widgets::{Live, Server, ValueInfo, WidgetType, source_title};

/// Seconds of history shown (Elastic).
pub const TIME_DISPLAYED: &str = "time_displayed";
/// Elastic's older name for [`TIME_DISPLAYED`]; read, never written.
const VISIBLE_TIME: &str = "visibleTime";
/// The bottom of a fixed range (Elastic). Unset: fit the values.
pub const MIN_VALUE: &str = "min_value";
/// The top of a fixed range (Elastic). Unset: fit the values.
pub const MAX_VALUE: &str = "max_value";
/// The main line's color, as ARGB (Elastic).
pub const COLOR: &str = "color";
/// Line width in points (Elastic).
pub const LINE_WIDTH: &str = "line_width";
/// The lines after the main one: a list of `{"topic", "data_type",
/// "struct_meta"?, "color"?, "name"?}` (raptaboard).
pub const SERIES: &str = "series";
/// Show the legend; unset shows it when there's more than one line
/// (raptaboard).
pub const SHOW_LEGEND: &str = "show_legend";
/// Show the pause button; on unless turned off (raptaboard).
pub const SHOW_PAUSE: &str = "show_pause";
/// Show values where the pointer is; on unless turned off (raptaboard).
pub const SHOW_HOVER: &str = "show_hover";
/// How many seconds of values are kept, to look back through while paused
/// (raptaboard). Never less than `time_displayed`.
pub const HISTORY_KEPT: &str = "history_kept";

const KEYS: [&str; 8] = [
    HISTORY_KEPT,
    TIME_DISPLAYED,
    MIN_VALUE,
    MAX_VALUE,
    LINE_WIDTH,
    SHOW_LEGEND,
    SHOW_PAUSE,
    SHOW_HOVER,
];

/// How often a graph wants new values when its `period` isn't set, in
/// seconds: about 60 a second, the most the window redraws. (Elastic's
/// widgets default to 0.06, 0.033 for its graphs.) Values are recorded as
/// they arrive; NetworkTables batches them, so graphs aren't for signals
/// much faster than this anyway.
pub const DEFAULT_PERIOD_S: f64 = 0.016;
/// Elastic's default: 5 seconds shown.
pub const DEFAULT_TIME: f64 = 5.0;
/// Seconds of values kept when `history_kept` isn't set: a minute.
pub const DEFAULT_HISTORY: f64 = 60.0;
/// The most seconds of values a graph keeps: 10 minutes.
pub const MAX_HISTORY: f64 = 600.0;
/// The fewest seconds a paused graph zooms in to.
const MIN_SPAN: f64 = 0.05;
/// Elastic's default line color (Material cyan 500).
pub const DEFAULT_COLOR: Color32 = Color32::from_rgb(0x00, 0xBC, 0xD4);
const DEFAULT_WIDTH: f64 = 2.0;
/// Colors for the lines after the main one, in order, until they repeat.
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
/// The measuring cursors' color.
const CURSOR_COLOR: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// The hover guide line's color: faint grey.
const HOVER_COLOR: Color32 = Color32::from_rgba_premultiplied(0x60, 0x60, 0x60, 0x80);
/// Radius of the dots marking each line's value under the pointer, points.
const HOVER_DOT: f32 = 3.5;
/// How close a click must be to a cursor line to take it away, in points.
const CURSOR_GRAB: f32 = 6.0;
/// The pause button's size, in points.
const BUTTON_SIZE: egui::Vec2 = egui::vec2(22.0, 18.0);

/// The Graph widget (Elastic's `"Graph"`).
pub struct Graph;

/// One line on a graph, as saved: what it shows and how it's drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The topic.
    pub topic: String,
    /// The value's NT type, if known.
    pub data_type: Option<String>,
    /// For one field of a struct topic: Elastic's `struct_meta` object.
    pub struct_meta: Option<Json>,
    /// Its color.
    pub color: Color32,
    /// A name for the legend, if set; else from the topic.
    pub name: Option<String>,
}

impl Entry {
    /// The struct field this line shows, if any.
    pub fn field(&self) -> Option<StructMeta> {
        self.struct_meta.as_ref().and_then(struct_meta_of)
    }

    /// Its name in the legend.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| source_title(&self.topic, self.field().as_ref()))
    }

    /// What it shows, for recording its values.
    pub fn key(&self) -> SeriesKey {
        SeriesKey {
            topic: self.topic.clone(),
            field: self.field().map(|m| m.path),
        }
    }
}

/// One line as drawn.
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

/// Seconds of values `props` keeps to look back through while paused:
/// `history_kept` (a minute if not set, at most 10 minutes), and never less
/// than the time shown.
///
/// ```
/// use raptaboard::widgets::builtin::graph::history_kept;
/// assert_eq!(history_kept(&serde_json::Map::new()), 60.0);
/// let long = serde_json::json!({"history_kept": 300, "time_displayed": 5});
/// assert_eq!(history_kept(long.as_object().unwrap()), 300.0);
/// let wide = serde_json::json!({"history_kept": 10, "time_displayed": 30});
/// assert_eq!(history_kept(wide.as_object().unwrap()), 30.0);
/// ```
pub fn history_kept(props: &Map<String, Json>) -> f64 {
    props::number(props, HISTORY_KEPT, DEFAULT_HISTORY)
        .clamp(1.0, MAX_HISTORY)
        .max(time_displayed(props))
}

/// `points` (time, value; oldest first) between `from` and `to`, thinned
/// for drawing `columns` wide: where there are more than two points per
/// column, each column keeps only its lowest and highest (in the order they
/// came), so peaks still show. The points just outside the range are kept
/// too, so lines reach the edges.
///
/// ```
/// use raptaboard::widgets::builtin::graph::thin;
/// let points: Vec<[f64; 2]> = (0..1000).map(|i| [i as f64, (i % 10) as f64]).collect();
/// let drawn = thin(&points, 0.0, 999.0, 100);
/// assert!(drawn.len() <= 2 * 100 + 2);
/// // Every column still reaches 0 and 9.
/// assert!(drawn.iter().any(|p| p[1] == 9.0) && drawn.iter().any(|p| p[1] == 0.0));
/// // Few points: all of them, between the edges.
/// assert_eq!(thin(&points[..5], 1.0, 3.0, 100), &points[..5]);
/// ```
pub fn thin(points: &[[f64; 2]], from: f64, to: f64, columns: usize) -> Vec<[f64; 2]> {
    let start = points.partition_point(|p| p[0] < from).saturating_sub(1);
    let end = (points.partition_point(|p| p[0] <= to) + 1).min(points.len());
    let shown = &points[start..end.max(start)];
    let columns = columns.max(1);
    if shown.len() <= 2 * columns + 2 || to <= from {
        return shown.to_vec();
    }
    let per = columns as f64 / (to - from);
    let column = |x: f64| ((x - from) * per).floor().clamp(-1.0, columns as f64) as i64;
    let mut out = Vec::with_capacity(2 * columns + 4);
    let mut i = 0;
    while i < shown.len() {
        let c = column(shown[i][0]);
        let mut j = i;
        let (mut low, mut high) = (i, i);
        while j < shown.len() && column(shown[j][0]) == c {
            if shown[j][1] < shown[low][1] {
                low = j;
            }
            if shown[j][1] > shown[high][1] {
                high = j;
            }
            j += 1;
        }
        let (a, b) = (low.min(high), low.max(high));
        out.push(shown[a]);
        if b != a {
            out.push(shown[b]);
        }
        i = j;
    }
    out
}

/// The fixed range, if both ends are set and the bottom is below the top.
pub fn fixed_range(props: &Map<String, Json>) -> Option<(f64, f64)> {
    let low = props.get(MIN_VALUE)?.as_f64()?;
    let high = props.get(MAX_VALUE)?.as_f64()?;
    (low < high).then_some((low, high))
}

/// Every line in a graph's settings, main first.
pub fn entries(props: &Map<String, Json>) -> Vec<Entry> {
    let mut all = Vec::new();
    if let Some(topic) = props.get("topic").and_then(Json::as_str) {
        all.push(Entry {
            topic: topic.to_owned(),
            data_type: props
                .get(DATA_TYPE)
                .and_then(Json::as_str)
                .map(str::to_owned),
            struct_meta: props.get("struct_meta").cloned(),
            color: props::color_property(props, COLOR).unwrap_or(DEFAULT_COLOR),
            name: None,
        });
    }
    let extra = props
        .get(SERIES)
        .and_then(Json::as_array)
        .into_iter()
        .flatten();
    for (i, item) in extra.enumerate() {
        let Some(topic) = item.get("topic").and_then(Json::as_str) else {
            continue;
        };
        all.push(Entry {
            topic: topic.to_owned(),
            data_type: item
                .get("data_type")
                .and_then(Json::as_str)
                .map(str::to_owned),
            struct_meta: item.get("struct_meta").cloned(),
            color: item
                .as_object()
                .and_then(|o| props::color_property(o, "color"))
                .unwrap_or(PALETTE[i % PALETTE.len()]),
            name: item.get("name").and_then(Json::as_str).map(str::to_owned),
        });
    }
    all
}

/// Save `lines` into a graph's settings: the first as the main line (the
/// widget's own `topic`, `data_type`, `struct_meta` and `color`), the rest
/// as `series`. Colors are saved with each line, so a line keeps its color
/// when the order changes. Does nothing with no lines: a graph always has
/// one.
pub fn write_entries(props: &mut Map<String, Json>, lines: &[Entry]) {
    let Some((main, rest)) = lines.split_first() else {
        return;
    };
    set_property(props, "topic", Some(main.topic.clone().into()));
    set_property(props, DATA_TYPE, main.data_type.clone().map(Json::from));
    set_property(props, "struct_meta", main.struct_meta.clone());
    set_property(props, COLOR, Some(props::color_json(main.color)));
    let extra: Vec<Json> = rest
        .iter()
        .map(|line| {
            let mut item = Map::new();
            item.insert("topic".into(), line.topic.clone().into());
            if let Some(ty) = &line.data_type {
                item.insert("data_type".into(), ty.clone().into());
            }
            if let Some(meta) = &line.struct_meta {
                item.insert("struct_meta".into(), meta.clone());
            }
            item.insert("color".into(), props::color_json(line.color));
            if let Some(name) = &line.name {
                item.insert("name".into(), name.clone().into());
            }
            Json::Object(item)
        })
        .collect();
    set_property(props, SERIES, (!extra.is_empty()).then(|| extra.into()));
}

/// The lines a graph widget shows, main first.
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
    entries(&widget.properties)
        .into_iter()
        .map(|e| Series {
            key: e.key(),
            name: e.label(),
            color: e.color,
        })
        .collect()
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

/// Add a line to a graph: topic `topic` of NT type `nt_type`, or one of its
/// struct `field`s. Returns `false` (adding nothing) if the graph already
/// shows it.
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
    let mut lines = entries(&widget.properties);
    let key = SeriesKey {
        topic: topic.to_owned(),
        field: field.map(|f| f.path.clone()),
    };
    if lines.iter().any(|l| l.key() == key) {
        return false;
    }
    lines.push(Entry {
        topic: topic.to_owned(),
        data_type: Some(nt_type.to_owned()),
        struct_meta: field.map(|f| f.to_json(nt_type)),
        color: PALETTE[lines.len().saturating_sub(1) % PALETTE.len()],
        name: None,
    });
    write_entries(&mut widget.properties, &lines);
    true
}

/// Move line `from` to place `to` (0 is main). Out of range does nothing.
pub fn move_line(props: &mut Map<String, Json>, from: usize, to: usize) {
    let mut lines = entries(props);
    if from < lines.len() && to < lines.len() {
        let line = lines.remove(from);
        lines.insert(to, line);
        write_entries(props, &lines);
    }
}

/// Remove line `index`, unless it's the last one left.
pub fn remove_line(props: &mut Map<String, Json>, index: usize) {
    let mut lines = entries(props);
    if lines.len() > 1 && index < lines.len() {
        lines.remove(index);
        write_entries(props, &lines);
    }
}

/// Whether the legend shows: as set, or when there's more than one line.
fn show_legend(props: &Map<String, Json>, lines: usize) -> bool {
    props::flag(props, SHOW_LEGEND, lines > 1)
}

/// `points` (sorted by time) read at time `x`: between two points, the
/// straight line between them. `None` outside the points.
///
/// ```
/// use raptaboard::widgets::builtin::graph::value_at;
/// let points = [[0.0, 0.0], [1.0, 10.0], [2.0, 0.0]];
/// assert_eq!(value_at(&points, 0.5), Some(5.0));
/// assert_eq!(value_at(&points, 2.0), Some(0.0));
/// assert_eq!(value_at(&points, 3.0), None);
/// ```
pub fn value_at(points: &[[f64; 2]], x: f64) -> Option<f64> {
    let after = points.partition_point(|p| p[0] < x);
    if after == points.len() {
        return None;
    }
    let [x1, y1] = points[after];
    if after == 0 {
        return (x1 == x).then_some(y1);
    }
    let [x0, y0] = points[after - 1];
    if x1 == x0 {
        return Some(y1);
    }
    Some(y0 + (y1 - y0) * (x - x0) / (x1 - x0))
}

/// The cursor readout: the cursors' times, the time between them, and each
/// line's values there, one line of text each.
///
/// ```
/// use raptaboard::widgets::builtin::graph::readout;
/// let points = [[-2.0, 0.0], [0.0, 4.0]];
/// let text = readout(&[-2.0, -1.0], &[("speed", &points[..])]);
/// assert_eq!(text[0], "A -2.000 s   B -1.000 s   Δt 1.000 s   (click again to clear)");
/// assert_eq!(text[1], "speed   0.000 → 2.000   Δ 2.000");
/// ```
pub fn readout(cursors: &[f64], lines: &[(&str, &[[f64; 2]])]) -> Vec<String> {
    let mut text = Vec::new();
    match cursors {
        [a] => {
            text.push(format!("A {a:.3} s   (click again for B)"));
            for (name, points) in lines {
                text.push(format!("{name}   {}", show(value_at(points, *a))));
            }
        }
        [a, b, ..] => {
            text.push(format!(
                "A {a:.3} s   B {b:.3} s   Δt {:.3} s   (click again to clear)",
                (b - a).abs()
            ));
            for (name, points) in lines {
                let (va, vb) = (value_at(points, *a), value_at(points, *b));
                let delta = va.zip(vb).map(|(va, vb)| vb - va);
                text.push(format!(
                    "{name}   {} → {}   Δ {}",
                    show(va),
                    show(vb),
                    show(delta)
                ));
            }
        }
        [] => {}
    }
    text
}

/// A readout number, or a dash where there's no value.
fn show(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |v| format!("{v:.3}"))
}

/// One line as drawn: its name, color and points (seconds before now,
/// value).
type Drawn = (String, Color32, Vec<[f64; 2]>);

/// What a graph on screen is doing: frozen or live, where the measuring
/// cursors are, and (paused) which stretch of time shows. Kept in egui's
/// memory for the widget, not in the layout.
#[derive(Debug, Clone, Default)]
struct View {
    /// While paused: each line's name, color and every point kept (time in
    /// seconds before the pause). Shared, so keeping the view each frame
    /// doesn't copy them.
    frozen: Option<Arc<Vec<Drawn>>>,
    /// Measuring cursors' times, in the same seconds: A, then B.
    cursors: Vec<f64>,
    /// While paused, what shows.
    window: Window,
}

/// The stretch of time a paused graph shows, in seconds before the pause.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Window {
    /// The time at the right edge: 0 is the pause, -10 ten seconds before.
    pub end: f64,
    /// How many seconds show.
    pub span: f64,
}

impl Window {
    /// Kept to what's recorded: no later than the pause, no earlier than
    /// `oldest` (the first point's time) where there's enough to fill it,
    /// and no wider than all of it (or `shown`, the time the graph shows
    /// live, if that's wider).
    ///
    /// ```
    /// use raptaboard::widgets::builtin::graph::Window;
    /// let w = Window { end: -100.0, span: 5.0 }.kept(-60.0, 5.0);
    /// assert_eq!(w, Window { end: -55.0, span: 5.0 });
    /// let w = Window { end: 3.0, span: 500.0 }.kept(-60.0, 5.0);
    /// assert_eq!(w, Window { end: 0.0, span: 60.0 });
    /// ```
    pub fn kept(self, oldest: f64, shown: f64) -> Window {
        let span = self.span.clamp(MIN_SPAN, (-oldest).max(shown));
        let end = self.end.clamp((oldest + span).min(0.0), 0.0);
        Window { end, span }
    }

    /// Moved `seconds` later (negative: earlier).
    pub fn moved(self, seconds: f64) -> Window {
        Window {
            end: self.end + seconds,
            ..self
        }
    }

    /// Zoomed by `factor` (above 1 shows more time) about time `at`, which
    /// stays where it is on screen.
    ///
    /// ```
    /// use raptaboard::widgets::builtin::graph::Window;
    /// let w = Window { end: 0.0, span: 10.0 }.zoomed(2.0, -5.0);
    /// assert_eq!(w, Window { end: 5.0, span: 20.0 });
    /// ```
    pub fn zoomed(self, factor: f64, at: f64) -> Window {
        Window {
            end: at + (self.end - at) * factor,
            span: self.span * factor,
        }
    }
}

/// The time of the oldest point in `lines`, or 0 if there are none.
fn oldest(lines: &[Drawn]) -> f64 {
    lines
        .iter()
        .filter_map(|(_, _, points)| points.first().map(|p| p[0]))
        .fold(0.0, f64::min)
}

/// Each line's points from the traces, in seconds before now: those from
/// `from` seconds back (and the one before, so the line reaches the edge),
/// or every one kept if `from` is `None`. The last value holds until a new
/// one arrives, so a line reaches "now" even for a topic that rarely
/// changes.
fn lines(widget: &WidgetContainer, live: &Live<'_>, from: Option<f64>) -> Vec<Drawn> {
    series(widget)
        .into_iter()
        .map(|line| {
            let traces = live.frame.traces;
            let now = live.frame.now_us;
            let to_point = |(t, v): (i64, f64)| [(t - now) as f64 / 1e6, v];
            let mut points: Vec<[f64; 2]> = match from {
                Some(from) => traces
                    .points_since(&line.key, now + (from * 1e6) as i64)
                    .map(to_point)
                    .collect(),
                None => traces.points(&line.key).map(to_point).collect(),
            };
            if let Some(&[x, y]) = points.last()
                && x < 0.0
            {
                points.push([0.0, y]);
            }
            (line.name, line.color, points)
        })
        .collect()
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

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title)
            .shrink(card::BODY_MARGIN);
        let p = &widget.properties;
        let view_id = ui.scope_id().with("graph_view");
        let mut view: View = ui.ctx().data(|d| d.get_temp(view_id)).unwrap_or_default();
        let seconds = time_displayed(p);
        let range = fixed_range(p);
        let width = props::number(p, LINE_WIDTH, DEFAULT_WIDTH).clamp(0.5, 20.0) as f32;

        // The lines: as frozen while paused, else live from the traces
        // (just the time shown).
        let paused = view.frozen.is_some();
        let shown: Arc<Vec<Drawn>> = match &view.frozen {
            Some(frozen) => Arc::clone(frozen),
            None => Arc::new(lines(widget, live, Some(-seconds))),
        };
        if paused {
            view.window = view.window.kept(oldest(&shown), seconds);
        }
        let (from, to) = if paused {
            (view.window.end - view.window.span, view.window.end)
        } else {
            (-seconds, 0.0)
        };

        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        // Live, it only senses hover, so it never takes the drags that move
        // the widget; paused, it takes clicks for the cursors and drags to
        // move through time.
        let mut plot = Plot::new("graph")
            .sense(if paused {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            })
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .allow_axis_zoom_drag(false)
            .show_background(false)
            .width(body.width())
            .height(body.height());
        // egui_plot's own hover shows the pointer's coordinates, not the
        // lines'; ours below snaps to the lines instead.
        plot = plot.show_x(false).show_y(false);
        let hover = props::flag(p, SHOW_HOVER, true);
        if show_legend(p, shown.len()) {
            plot = plot.legend(Legend::default().position(Corner::LeftTop));
        }
        let cursors = view.cursors.clone();
        let columns = body.width().max(1.0) as usize;
        let clicked_at = plot.show(&mut child, |plot_ui| {
            plot_ui.set_plot_bounds_x(from..=to);
            match range {
                Some((low, high)) => plot_ui.set_plot_bounds_y(low..=high),
                None => plot_ui.set_auto_bounds([false, true]),
            }
            for (name, color, points) in shown.iter() {
                plot_ui.line(
                    Line::new(
                        name.clone(),
                        PlotPoints::new(thin(points, from, to, columns)),
                    )
                    .color(*color)
                    .width(width),
                );
            }
            for x in &cursors {
                // No name: cursors stay out of the legend.
                plot_ui.vline(VLine::new("", *x).color(CURSOR_COLOR).width(1.0));
            }
            // Hovering: a faint line at the pointer's time and a dot on each
            // line where it crosses, at the line's value there.
            let hovered = plot_ui.response().hovered();
            let pointer_x = plot_ui.pointer_coordinate().map(|p| p.x);
            let hover_x = pointer_x.filter(|_| hover && hovered);
            if let Some(x) = hover_x {
                plot_ui.vline(VLine::new("", x).color(HOVER_COLOR).width(1.0));
                for (_, color, points) in shown.iter() {
                    if let Some(y) = value_at(points, x) {
                        plot_ui.points(
                            Points::new("", vec![[x, y]])
                                .color(*color)
                                .radius(HOVER_DOT)
                                .filled(true),
                        );
                    }
                }
            }
            // Near a cursor (in pixels, so the same at any zoom), a click
            // takes that cursor away; the hand pointer says so.
            let near = plot_ui.response().hover_pos().and_then(|pointer| {
                cursors.iter().position(|&x| {
                    let line = plot_ui.screen_from_plot(egui_plot::PlotPoint::new(x, 0.0));
                    (line.x - pointer.x).abs() <= CURSOR_GRAB
                })
            });
            if near.is_some() {
                plot_ui
                    .ctx()
                    .set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let click = if plot_ui.response().clicked() {
                match near {
                    Some(i) => Some(CursorClick::Remove(i)),
                    None => pointer_x.map(CursorClick::Place),
                }
            } else {
                None
            };
            // Paused: dragging along the plot or the wheel moves through
            // time (in points; seconds below), Ctrl or ⌘ with the wheel
            // zooms about the pointer.
            let response = plot_ui.response();
            let mut moved = 0.0;
            let mut zoom = None;
            if paused {
                if response.dragged() {
                    moved -= response.drag_delta().x;
                }
                if response.hovered() {
                    let (scroll, factor) = plot_ui
                        .ctx()
                        .input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
                    moved -= scroll.x + scroll.y;
                    if factor != 1.0
                        && let Some(x) = pointer_x
                    {
                        zoom = Some((1.0 / f64::from(factor), x));
                    }
                }
            }
            (click, hover_x, moved, zoom, response.rect.width())
        });
        let (click, hover_x, moved, zoom, plot_width) = clicked_at.inner;
        if paused {
            if moved != 0.0 {
                let per_point = view.window.span / f64::from(plot_width.max(1.0));
                view.window = view.window.moved(f64::from(moved) * per_point);
            }
            if let Some((factor, at)) = zoom {
                view.window = view.window.zoomed(factor, at);
            }
            if moved != 0.0 || zoom.is_some() {
                // The wheel moved the graph, not the grid behind it.
                // (The window is kept to what's recorded next frame, before
                // it's drawn.)
                ui.ctx()
                    .input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }
        // The hovered values, bottom right, each in its line's color.
        if let Some(x) = hover_x {
            let mut job = egui::text::LayoutJob::default();
            let font = egui::FontId::monospace(11.0);
            let mut add = |text: String, color: Color32| {
                job.append(&text, 0.0, egui::TextFormat::simple(font.clone(), color));
            };
            add(format!("{x:.2} s"), ui.visuals().weak_text_color());
            for (name, color, points) in shown.iter() {
                add(format!("\n{name} {}", show(value_at(points, x))), *color);
            }
            let galley = ui.painter().layout_job(job);
            let size = galley.size();
            let at = egui::pos2(body.max.x - size.x - 4.0, body.max.y - size.y - 22.0);
            let back = Rect::from_min_size(at, size).expand(3.0);
            ui.painter().rect_filled(
                back,
                egui::CornerRadius::same(3),
                ui.visuals().extreme_bg_color.gamma_multiply(0.85),
            );
            ui.painter().galley(at, galley, ui.visuals().text_color());
        }
        match click {
            // With A and B placed, any click clears both.
            Some(_) if view.cursors.len() >= 2 => view.cursors.clear(),
            // With only A: a click on it takes it away...
            Some(CursorClick::Remove(i)) => {
                view.cursors.remove(i);
            }
            // ...and anywhere else places the next.
            Some(CursorClick::Place(x)) => view.cursors.push(x),
            None => {}
        }

        // The readout, bottom left, while measuring.
        if paused && !view.cursors.is_empty() {
            let lines: Vec<(&str, &[[f64; 2]])> = shown
                .iter()
                .map(|(name, _, points)| (name.as_str(), points.as_slice()))
                .collect();
            let text = readout(&view.cursors, &lines).join("\n");
            let galley = ui.painter().layout(
                text,
                egui::FontId::monospace(11.0),
                ui.visuals().text_color(),
                body.width(),
            );
            let at = egui::pos2(body.min.x + 4.0, body.max.y - galley.size().y - 22.0);
            let back = Rect::from_min_size(at, galley.size()).expand(3.0);
            ui.painter().rect_filled(
                back,
                egui::CornerRadius::same(3),
                ui.visuals().extreme_bg_color.gamma_multiply(0.85),
            );
            ui.painter().galley(at, galley, ui.visuals().text_color());
        }

        // The pause button, top right.
        if props::flag(p, SHOW_PAUSE, true) {
            let place = Rect::from_min_size(
                egui::pos2(body.max.x - BUTTON_SIZE.x, body.min.y),
                BUTTON_SIZE,
            );
            let (label, hint) = if paused {
                ("▶", "Back to live (values kept recording while paused)")
            } else {
                (
                    "⏸",
                    "Pause: freeze the picture; values keep recording. \
                     Then click the plot twice to measure between two times, \
                     and drag it or use the mouse wheel to look back \
                     (Ctrl or ⌘ with the wheel zooms).",
                )
            };
            if ui
                .put(place, egui::Button::new(label).small())
                .on_hover_text(hint)
                .clicked()
            {
                view = if paused {
                    View::default()
                } else {
                    View {
                        // Everything kept, to look back through.
                        frozen: Some(Arc::new(lines(widget, live, None))),
                        cursors: Vec::new(),
                        window: Window {
                            end: 0.0,
                            span: seconds,
                        },
                    }
                };
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(view_id, view));
        None
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

    /// About 60 a second: graphs show how values move.
    fn default_period(&self) -> f64 {
        DEFAULT_PERIOD_S
    }

    fn extra_topics(&self, props: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        entries(props)
            .into_iter()
            .skip(1)
            .map(|e| e.topic)
            .collect()
    }

    fn traced(&self, widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        // Everything kept, not just what shows, to look back through.
        let seconds = history_kept(&widget.properties);
        series(widget)
            .into_iter()
            .map(|s| (s.key, seconds))
            .collect()
    }

    fn takes_topics(&self) -> bool {
        true
    }

    /// Each topic that's a number or boolean, and not already a line,
    /// becomes a line.
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        let added = topics
            .iter()
            .filter(|t| self.shows(&t.ty))
            .filter(|t| add_series(widget, &t.name, &t.ty, t.field.as_ref()))
            .count();
        match added {
            0 => "Nothing new to add to that graph (numbers and booleans only)".to_owned(),
            1 => format!("Added a line to \"{}\"", widget.title),
            n => format!("Added {n} lines to \"{}\"", widget.title),
        }
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
        props::number_row(
            ui,
            p,
            "History kept (s)",
            HISTORY_KEPT,
            DEFAULT_HISTORY,
            1.0..=MAX_HISTORY,
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
        let count = entries(p).len();
        props::flag_row(ui, p, "Legend", "Show the legend", SHOW_LEGEND, count > 1);
        props::flag_row(
            ui,
            p,
            "Pause",
            "Show the pause button (and measuring)",
            SHOW_PAUSE,
            true,
        );
        props::flag_row(
            ui,
            p,
            "Hover",
            "Show values where the pointer is",
            SHOW_HOVER,
            true,
        );

        // Every line, main first: color, name, and buttons to reorder or
        // remove it. Moving a line to the top makes it main.
        ui.label("Lines");
        ui.vertical(|ui| {
            let lines = entries(p);
            let mut change = None;
            for (i, line) in lines.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut color = line.color;
                    if egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    )
                    .changed()
                    {
                        change = Some(LineChange::Color(i, color));
                    }
                    ui.label(line.label()).on_hover_text(&line.topic);
                    if i == 0 {
                        ui.weak("main")
                            .on_hover_text("Saved as the widget's own topic, as Elastic saves it");
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("↑").small())
                        .clicked()
                    {
                        change = Some(LineChange::Move(i, i - 1));
                    }
                    if ui
                        .add_enabled(i + 1 < lines.len(), egui::Button::new("↓").small())
                        .clicked()
                    {
                        change = Some(LineChange::Move(i, i + 1));
                    }
                    if ui
                        .add_enabled(lines.len() > 1, egui::Button::new("Remove").small())
                        .on_disabled_hover_text("A graph needs one line")
                        .clicked()
                    {
                        change = Some(LineChange::Remove(i));
                    }
                });
            }
            ui.weak("Drop topics from the tree onto the graph to add lines.");
            match change {
                Some(LineChange::Color(i, color)) => {
                    let mut lines = lines;
                    lines[i].color = color;
                    write_entries(p, &lines);
                }
                Some(LineChange::Move(from, to)) => move_line(p, from, to),
                Some(LineChange::Remove(i)) => remove_line(p, i),
                None => {}
            }
        });
        ui.end_row();
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        // Not the lines or their colors: Reset settings keeps what's shown.
        &KEYS
    }
}

/// A click on a paused plot.
enum CursorClick {
    /// Place a cursor at this time.
    Place(f64),
    /// Take away the cursor at this place in the list.
    Remove(usize),
}

/// A change to a graph's lines, made after drawing the list.
enum LineChange {
    Color(usize, Color32),
    Move(usize, usize),
    Remove(usize),
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::widgets::{Frame, Place};
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

    fn names(w: &WidgetContainer) -> Vec<String> {
        series(w).into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn graphs_default_to_about_60_a_second() {
        let mut w = graph();
        w.properties.remove("period");
        assert_eq!(props::period_s(&w), DEFAULT_PERIOD_S);
        // Other widgets keep Elastic's 0.06.
        w.widget_type = "Text Display".into();
        assert_eq!(props::period_s(&w), 0.06);
        // A saved period wins.
        w.widget_type = "Graph".into();
        w.properties.insert("period".into(), 0.1.into());
        assert_eq!(props::period_s(&w), 0.1);
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
    fn lines_are_added_once_reordered_and_removed() {
        let mut w = graph();
        let x = StructMeta {
            path: vec!["translation".into(), "x".into()],
            schema_name: "Pose2d".into(),
        };
        assert!(add_series(&mut w, "/Drive/pose", "double", Some(&x)));
        assert!(add_series(&mut w, "/Drive/target", "double", None));
        assert!(!add_series(&mut w, "/Drive/pose", "double", Some(&x)));
        assert_eq!(names(&w), ["speed", "pose.translation.x", "target"]);
        let colors: Vec<Color32> = series(&w).iter().map(|s| s.color).collect();
        assert_eq!(colors, [DEFAULT_COLOR, PALETTE[0], PALETTE[1]]);
        assert_eq!(
            Graph.extra_topics(&w.properties, Server::default()),
            ["/Drive/pose", "/Drive/target"]
        );
        assert!(show_legend(&w.properties, 3));

        // Moving the struct field to the top makes it the main line: the
        // widget's own topic and struct_meta, as Elastic saves it. Each
        // line keeps its color.
        move_line(&mut w.properties, 1, 0);
        assert_eq!(names(&w), ["pose.translation.x", "speed", "target"]);
        assert_eq!(w.topic(), Some("/Drive/pose"));
        assert_eq!(w.struct_meta(), Some(x));
        let moved: Vec<Color32> = series(&w).iter().map(|s| s.color).collect();
        assert_eq!(moved, [PALETTE[0], DEFAULT_COLOR, PALETTE[1]]);

        // Removing the main line promotes the next; the last can't go.
        remove_line(&mut w.properties, 0);
        assert_eq!(names(&w), ["speed", "target"]);
        assert_eq!(w.topic(), Some("/Drive/speed"));
        assert!(w.struct_meta().is_none());
        remove_line(&mut w.properties, 1);
        assert!(!w.properties.contains_key(SERIES));
        remove_line(&mut w.properties, 0);
        assert_eq!(names(&w), ["speed"]);
        assert!(!show_legend(&w.properties, 1));
    }

    #[test]
    fn cursors_read_values_between_points() {
        let points = vec![[-4.0, 1.0], [-2.0, 3.0], [0.0, 3.0]];
        assert_eq!(value_at(&points, -3.0), Some(2.0));
        assert_eq!(value_at(&points, -4.0), Some(1.0));
        assert_eq!(value_at(&points, -5.0), None);
        let lines = [("speed", &points[..])];
        let one = readout(&[-3.0], &lines);
        assert_eq!(one, ["A -3.000 s   (click again for B)", "speed   2.000"]);
        let two = readout(&[-1.0, -4.0], &lines);
        assert_eq!(
            two[0],
            "A -1.000 s   B -4.000 s   Δt 3.000 s   (click again to clear)"
        );
        assert_eq!(two[1], "speed   3.000 → 1.000   Δ -2.000");
        // Outside the recorded points: a dash.
        let gap = readout(&[-9.0, -1.0], &lines);
        assert_eq!(gap[1], "speed   — → 3.000   Δ —");
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
            let (others, effects) = (Default::default(), Default::default());
            let frame = Frame {
                traces: &traces,
                now_us: 2_000_000,
                others: &others,
                connected: true,
                effects: &effects,
                structs: None,
            };
            let live = frame.live(None, Place::default());
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
        // Both names in the legend, a line for each, and the pause button.
        for name in ["speed", "target", "⏸"] {
            assert!(texts.iter().any(|t| t == name), "{name} in {texts:?}");
        }
        assert!(paths >= 2, "{paths} lines drawn");
    }

    #[test]
    fn pausing_then_clicking_twice_measures() {
        use egui::{Event, PointerButton, Pos2, RawInput};
        let w = graph();
        let mut traces = crate::traces::Traces::default();
        let key = series(&w)[0].key.clone();
        traces.want(&key, 5.0);
        for i in 0..50 {
            traces.record(&key, i * 100_000, i as f64);
        }
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let frame = |events: Vec<Event>| -> Vec<String> {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                let (others, effects) = (Default::default(), Default::default());
                let frame = Frame {
                    traces: &traces,
                    now_us: 5_000_000,
                    others: &others,
                    connected: true,
                    effects: &effects,
                    structs: None,
                };
                let live = frame.live(None, Place::default());
                Graph.show(ui, rect, &w, &live);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn walk(shape: &egui::Shape, texts: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
                    _ => {}
                }
            }
            for s in &output.shapes {
                walk(&s.shape, &mut texts);
            }
            texts
        };
        let click = |at: Pos2| {
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        // Where the card's body is: below the title, inside the margin.
        let body = rect
            .with_min_y(rect.min.y + card::TITLE_HEIGHT)
            .shrink(card::BODY_MARGIN);
        let button = egui::pos2(
            body.max.x - BUTTON_SIZE.x / 2.0,
            body.min.y + BUTTON_SIZE.y / 2.0,
        );
        frame(vec![]);
        // Pause: the button turns to ▶.
        frame(click(button));
        assert!(frame(vec![]).iter().any(|t| t == "▶"));
        // Two clicks on the plot: cursors A and B, and the readout.
        frame(click(body.center() - egui::vec2(80.0, 0.0)));
        frame(vec![]);
        frame(click(body.center() + egui::vec2(60.0, 0.0)));
        let texts = frame(vec![]);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Δt") && t.contains("speed")),
            "{texts:?}"
        );
        // With both placed, a click on one of them clears both.
        frame(click(body.center() + egui::vec2(61.0, 0.0)));
        let after = frame(vec![]);
        assert!(
            !after
                .iter()
                .any(|t| t.contains("Δt") || t.starts_with("A ")),
            "{after:?}"
        );
        // With only A, clicking on A takes it away.
        frame(click(body.center()));
        assert!(frame(vec![]).iter().any(|t| t.starts_with("A ")));
        frame(click(body.center() + egui::vec2(1.0, 0.0)));
        assert!(!frame(vec![]).iter().any(|t| t.starts_with("A ")));
        // Place both again; a click elsewhere clears them too.
        frame(click(body.center() - egui::vec2(80.0, 0.0)));
        frame(vec![]);
        frame(click(body.center() + egui::vec2(60.0, 0.0)));
        assert!(frame(vec![]).iter().any(|t| t.contains("Δt")));
        frame(click(body.center()));
        assert!(
            !frame(vec![])
                .iter()
                .any(|t| t.contains("Δt") || t.starts_with("A "))
        );
        frame(click(body.center()));
        assert!(frame(vec![]).iter().any(|t| t.starts_with("A ")));
        // Back to live: no readout, the button is ⏸ again.
        frame(click(button));
        let texts = frame(vec![]);
        assert!(texts.iter().any(|t| t == "⏸"));
        assert!(!texts.iter().any(|t| t.contains("Δt")));
    }

    #[test]
    fn hovering_reads_the_lines_where_the_pointer_is() {
        use egui::{Event, Pos2, RawInput};
        let w = graph();
        let mut traces = crate::traces::Traces::default();
        let key = series(&w)[0].key.clone();
        traces.want(&key, 5.0);
        // A flat line at 7.5, so the value read is the same anywhere on it.
        for i in 0..=50 {
            traces.record(&key, i * 100_000, 7.5);
        }
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let frame = |events: Vec<Event>| -> String {
            let raw = RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                let (others, effects) = (Default::default(), Default::default());
                let frame = Frame {
                    traces: &traces,
                    now_us: 5_000_000,
                    others: &others,
                    connected: true,
                    effects: &effects,
                    structs: None,
                };
                let live = frame.live(None, Place::default());
                Graph.show(ui, rect, &w, &live);
            });
            output.textures_delta.clear();
            let mut texts = String::new();
            fn walk(shape: &egui::Shape, texts: &mut String) {
                match shape {
                    egui::Shape::Text(t) => {
                        texts.push_str(t.galley.text());
                        texts.push('|');
                    }
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
                    _ => {}
                }
            }
            for s in &output.shapes {
                walk(&s.shape, &mut texts);
            }
            texts
        };
        frame(vec![]);
        assert!(!frame(vec![]).contains("speed 7.500"));
        // Over the plot: the line's value there, not the pointer's height.
        frame(vec![Event::PointerMoved(
            rect.center() + egui::vec2(0.0, 40.0),
        )]);
        let texts = frame(vec![]);
        assert!(texts.contains("speed 7.500"), "{texts}");
    }

    #[test]
    fn paused_it_looks_back_through_everything_kept() {
        use egui::{Event, Modifiers, PointerButton, Pos2, RawInput};
        let w = graph();
        let mut traces = crate::traces::Traces::default();
        let key = series(&w)[0].key.clone();
        // It asks for a minute of values recorded, not just the 5 s shown.
        assert_eq!(Graph.traced(&w), [(key.clone(), 60.0)]);
        traces.want(&key, history_kept(&w.properties));
        // A minute at 10 a second; each value is its time in seconds.
        for i in 0..=600 {
            traces.record(&key, i * 100_000, i as f64 / 10.0);
        }
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let frame = |events: Vec<Event>| -> Vec<String> {
            let raw = RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                let (others, effects) = (Default::default(), Default::default());
                let frame = Frame {
                    traces: &traces,
                    now_us: 60_000_000,
                    others: &others,
                    connected: true,
                    effects: &effects,
                    structs: None,
                };
                let live = frame.live(None, Place::default());
                Graph.show(ui, rect, &w, &live);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn walk(shape: &egui::Shape, texts: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
                    _ => {}
                }
            }
            for s in &output.shapes {
                walk(&s.shape, &mut texts);
            }
            texts
        };
        let press = |at: Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let click = |at: Pos2| vec![Event::PointerMoved(at), press(at, true), press(at, false)];
        let wheel = |dy: f32, modifiers: Modifiers| Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers,
        };
        let body = rect
            .with_min_y(rect.min.y + card::TITLE_HEIGHT)
            .shrink(card::BODY_MARGIN);
        let button = egui::pos2(
            body.max.x - BUTTON_SIZE.x / 2.0,
            body.min.y + BUTTON_SIZE.y / 2.0,
        );
        let middle = body.center();
        // Where cursor A is, in seconds before the pause, and the line's
        // value there: place it at `at` and read the readout, then take it
        // away.
        let measure = |at: Pos2| -> (f64, f64) {
            frame(click(at));
            let texts = frame(vec![]);
            let found = texts
                .iter()
                .find(|t| t.starts_with("A "))
                .unwrap_or_else(|| panic!("a readout in {texts:?}"))
                .clone();
            frame(click(at));
            frame(vec![]);
            let mut lines = found.lines();
            let time: f64 = lines.next().unwrap()[2..]
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let value: f64 = lines
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse()
                .unwrap();
            (time, value)
        };
        frame(vec![]);
        frame(click(button));
        frame(vec![]);
        // At first, the last 5 s: the middle is about 2.5 s back.
        let (t, v) = measure(middle);
        assert!((-3.5..-1.5).contains(&t), "{t}");
        assert!((v - (60.0 + t)).abs() < 0.11, "{t} {v}");

        // The wheel moves back in time, past what shows live.
        frame(vec![
            Event::PointerMoved(middle),
            wheel(300.0, Modifiers::NONE),
        ]);
        for _ in 0..30 {
            frame(vec![]);
        }
        let (back, v) = measure(middle);
        assert!(back < -6.0, "moved back: {back}");
        assert!((v - (60.0 + back)).abs() < 0.11, "{back} {v}");

        // Dragging to the left moves later again.
        let from = middle;
        let to = middle - egui::vec2(100.0, 0.0);
        frame(vec![Event::PointerMoved(from), press(from, true)]);
        for step in 1..=10 {
            let at = from + (to - from) * (step as f32 / 10.0);
            frame(vec![Event::PointerMoved(at)]);
        }
        frame(vec![press(to, false)]);
        frame(vec![]);
        let (dragged, _) = measure(middle);
        assert!(dragged > back + 0.5, "dragged later: {back} → {dragged}");

        // Zoomed out with Ctrl and the wheel: the left edge reaches further.
        let left = egui::pos2(body.min.x + 30.0, middle.y);
        let (before, _) = measure(left);
        frame(vec![
            Event::PointerMoved(middle),
            wheel(-400.0, Modifiers::CTRL),
        ]);
        for _ in 0..30 {
            frame(vec![]);
        }
        let (after, _) = measure(left);
        assert!(after < before - 1.0, "zoomed out: {before} → {after}");

        // Never before the oldest value kept, however far back it goes.
        for _ in 0..20 {
            frame(vec![
                Event::PointerMoved(middle),
                wheel(2000.0, Modifiers::NONE),
            ]);
        }
        for _ in 0..30 {
            frame(vec![]);
        }
        let (oldest, v) = measure(left);
        assert!(oldest >= -60.01, "{oldest}");
        assert!(oldest < -50.0, "{oldest}");
        assert!((v - (60.0 + oldest)).abs() < 0.11, "{oldest} {v}");
    }
}
