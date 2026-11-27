//! X/Y Graph and Polar Graph: one value plotted against another, recorded
//! at the same time, drawn with `egui_plot`. An X/Y Graph plots x against
//! y (a joystick's position, a robot's path, an arm's reach); a Polar
//! Graph plots a distance at an angle (a turret's aim and range).
//!
//! A graph shows one or more **pairs**, each in its own color. A pair is
//! two numbers, from two topics or two struct fields, or a whole struct
//! with a known pair (a Translation2d's or Pose2d's x and y, a
//! ChassisSpeeds' vx and vy; X/Y only). Values that come at different
//! times are paired by holding each one's last value until the other
//! changes.
//!
//! Made from a topic's "Add as" menu: a struct with a known pair is a whole
//! pair; a number is the first of one (x, or the angle), and the next
//! number dropped onto the graph from the tree finishes it (y, or the
//! distance). Further drops start new pairs.
//!
//! Each pair draws a trail of its last `trail_time` seconds (3), fading
//! as it ages, and a dot at the newest point; a trail of 0 shows the dot
//! only. The ⏸ button freezes the picture while values keep being
//! recorded, as on a Graph.
//!
//! Settings, all raptaboard's own: `pairs`, `trail_time`, `line_width`,
//! `show_legend`, `show_pause`, `show_hover`; X/Y's `x_min`, `x_max`,
//! `y_min`, `y_max` (fixed ranges; unset fits the values) and
//! `same_scale`; Polar's `max_distance` (unset fits), `angle_unit`
//! (`"degrees"` or `"radians"`), `zero_at` (`"right"`, `"up"`, `"left"` or
//! `"down"`) and `clockwise`.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

use eframe::egui::{self, Color32, Rect, Sense};
use egui_plot::{Corner, Legend, Line, Plot, PlotPoint, PlotPoints, Points, Text};
use nt4::Value;
use raptaboard_layout::{StructMeta, WidgetContainer};
use serde_json::{Map, Value as Json};

use super::graph::{self, LINE_WIDTH, SHOW_HOVER, SHOW_LEGEND, SHOW_PAUSE};
use crate::traces::SeriesKey;
use crate::tree::DraggedTopic;
use crate::widgets::shared::card;
use crate::widgets::shared::format::number_of;
use crate::widgets::shared::props::{self, DATA_TYPE, set_property};
use crate::widgets::{Live, Server, ValueInfo, WidgetType, source_title};

/// The pairs shown, as saved: a list of `{ "x": source, "y": source,
/// "color", "name" }`, each source `{ "topic", "data_type", "struct_meta" }`
/// as a Graph's lines are saved. A Polar Graph's x is the angle and its y
/// the distance. Unset, the widget's own topic makes the first pair.
pub const PAIRS: &str = "pairs";
/// Seconds of trail behind each pair's newest point; 0 shows the point only.
pub const TRAIL_TIME: &str = "trail_time";
/// X/Y: the x axis' fixed range (both ends set), else it fits the values.
pub const X_MIN: &str = "x_min";
/// X/Y: see [`X_MIN`].
pub const X_MAX: &str = "x_max";
/// X/Y: the y axis' fixed range (both ends set), else it fits the values.
pub const Y_MIN: &str = "y_min";
/// X/Y: see [`Y_MIN`].
pub const Y_MAX: &str = "y_max";
/// X/Y: one unit is as long across as up (for positions on the floor).
pub const SAME_SCALE: &str = "same_scale";
/// Polar: the outer ring's distance; unset fits the values.
pub const MAX_DISTANCE: &str = "max_distance";
/// Polar: `"degrees"` (the default) or `"radians"`.
pub const ANGLE_UNIT: &str = "angle_unit";
/// Polar: where angle 0 points: `"right"` (the default), `"up"`, `"left"`
/// or `"down"`.
pub const ZERO_AT: &str = "zero_at";
/// Polar: whether angles grow clockwise (anticlockwise by default, as math
/// and WPILib's angles do).
pub const CLOCKWISE: &str = "clockwise";

/// The trail's seconds unless set.
pub const DEFAULT_TRAIL: f64 = 3.0;
/// The longest trail, seconds.
pub const MAX_TRAIL: f64 = 60.0;
const DEFAULT_WIDTH: f64 = 2.0;
/// The newest point's dot, radius in points.
const DOT: f32 = 4.0;
/// How many steps a trail fades in.
const FADE_STEPS: usize = 8;
/// The faintest a trail gets, at its old end.
const FADE_LEAST: f32 = 0.15;
/// The angle between a polar graph's spokes, degrees.
const SPOKE_DEGREES: u32 = 30;
const BUTTON_SIZE: egui::Vec2 = egui::vec2(22.0, 18.0);

const XY_KEYS: [&str; 10] = [
    TRAIL_TIME,
    LINE_WIDTH,
    SHOW_LEGEND,
    SHOW_PAUSE,
    SHOW_HOVER,
    X_MIN,
    X_MAX,
    Y_MIN,
    Y_MAX,
    SAME_SCALE,
];
const POLAR_KEYS: [&str; 9] = [
    TRAIL_TIME,
    LINE_WIDTH,
    SHOW_LEGEND,
    SHOW_PAUSE,
    SHOW_HOVER,
    MAX_DISTANCE,
    ANGLE_UNIT,
    ZERO_AT,
    CLOCKWISE,
];

/// The X/Y Graph widget (raptaboard's own `"X/Y Graph"`).
pub struct XyGraph;
/// The Polar Graph widget (raptaboard's own `"Polar Graph"`).
pub struct PolarGraph;

/// Which of the two a widget is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Xy,
    Polar,
}

impl Kind {
    /// The names of a pair's two values.
    fn axes(self) -> (&'static str, &'static str) {
        match self {
            Kind::Xy => ("x", "y"),
            Kind::Polar => ("angle", "distance"),
        }
    }
}

/// One value of a pair: a topic, or one field of a struct topic.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The topic.
    pub topic: String,
    /// Its NT type (the field's, for a field), if known.
    pub data_type: Option<String>,
    /// The struct field, if it's one.
    pub field: Option<StructMeta>,
}

impl Source {
    /// What it records as.
    pub fn key(&self) -> SeriesKey {
        SeriesKey {
            topic: self.topic.clone(),
            field: self.field.as_ref().map(|f| f.path.clone()),
        }
    }

    /// Its name: the topic's last part, and the field's path.
    pub fn label(&self) -> String {
        source_title(&self.topic, self.field.as_ref())
    }

    fn to_json(&self) -> Json {
        let mut item = Map::new();
        item.insert("topic".into(), self.topic.clone().into());
        if let Some(ty) = &self.data_type {
            item.insert(DATA_TYPE.into(), ty.clone().into());
        }
        if let Some(field) = &self.field {
            let ty = self.data_type.as_deref().unwrap_or("double");
            item.insert("struct_meta".into(), field.to_json(ty));
        }
        Json::Object(item)
    }

    fn from_json(json: &Json) -> Option<Source> {
        Some(Source {
            topic: json.get("topic")?.as_str()?.to_owned(),
            data_type: json
                .get(DATA_TYPE)
                .and_then(Json::as_str)
                .map(str::to_owned),
            field: json.get("struct_meta").and_then(struct_meta_of),
        })
    }
}

/// One pair as saved: `x` (or the angle) and `y` (or the distance), which
/// is `None` until a second value is dropped on.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    /// X, or a polar graph's angle.
    pub x: Source,
    /// Y, or a polar graph's distance.
    pub y: Option<Source>,
    /// Its color.
    pub color: Color32,
    /// A name for the legend, if set.
    pub name: Option<String>,
}

impl Pair {
    /// Its name in the legend: as set; the struct's name for a struct's
    /// two fields; the folder's for two topics in one folder; else both
    /// values' names.
    ///
    /// ```
    /// use raptaboard::widgets::builtin::xy_graph::{Pair, Source};
    /// let source = |t: &str| Source { topic: t.into(), data_type: None, field: None };
    /// let pair = |x: &str, y: &str| Pair {
    ///     x: source(x), y: Some(source(y)), color: Default::default(), name: None,
    /// };
    /// assert_eq!(pair("/Stick/x", "/Stick/y").label(), "Stick");
    /// assert_eq!(pair("/Arm/angle", "/Turret/range").label(), "angle / range");
    /// ```
    pub fn label(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        let folder = |topic: &str| topic.rsplit_once('/').map(|(f, _)| f.to_owned());
        match &self.y {
            Some(y) if y.topic == self.x.topic && self.x.field.is_some() => {
                source_title(&self.x.topic, None)
            }
            Some(y)
                if self.x.field.is_none()
                    && y.field.is_none()
                    && folder(&self.x.topic).is_some_and(|f| !f.is_empty())
                    && folder(&self.x.topic) == folder(&y.topic) =>
            {
                source_title(&folder(&self.x.topic).unwrap_or_default(), None)
            }
            Some(y) => format!("{} / {}", self.x.label(), y.label()),
            None => self.x.label(),
        }
    }
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

/// The two fields of a struct that make an x and y, for the structs that
/// have them: a Translation2d or 3d (x, y), a Pose2d or 3d (its
/// translation's x and y), a ChassisSpeeds (vx, vy).
///
/// ```
/// use raptaboard::widgets::builtin::xy_graph::known_pair;
/// let (x, y) = known_pair("struct:Pose2d").unwrap();
/// assert_eq!((x.path, y.path), (vec!["translation".to_owned(), "x".to_owned()],
///                               vec!["translation".to_owned(), "y".to_owned()]));
/// assert!(known_pair("struct:Rotation2d").is_none());
/// assert!(known_pair("double").is_none());
/// ```
pub fn known_pair(nt_type: &str) -> Option<(StructMeta, StructMeta)> {
    let schema = nt_type.strip_prefix("struct:")?;
    let (x, y): (&[&str], &[&str]) = match schema {
        "Translation2d" | "Translation3d" => (&["x"], &["y"]),
        "Pose2d" | "Pose3d" => (&["translation", "x"], &["translation", "y"]),
        "ChassisSpeeds" => (&["vx"], &["vy"]),
        _ => return None,
    };
    let meta = |path: &[&str]| StructMeta {
        path: path.iter().map(|s| (*s).to_owned()).collect(),
        schema_name: schema.to_owned(),
    };
    Some((meta(x), meta(y)))
}

/// Whether a value of NT type `nt_type` is one number of a pair.
fn is_number(nt_type: &str) -> bool {
    matches!(nt_type, "double" | "float" | "int" | "boolean")
}

/// A pair made from one topic (or field) of NT type `nt_type`: a whole
/// pair for a struct with a [`known_pair`], else the first value of one.
/// `None` for a type that's neither.
fn pair_from(
    topic: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
    color: Color32,
) -> Option<Pair> {
    if field.is_none()
        && let Some((x, y)) = known_pair(nt_type)
    {
        let source = |meta: StructMeta| Source {
            topic: topic.to_owned(),
            data_type: Some("double".to_owned()),
            field: Some(meta),
        };
        return Some(Pair {
            x: source(x),
            y: Some(source(y)),
            color,
            name: None,
        });
    }
    is_number(nt_type).then(|| Pair {
        x: Source {
            topic: topic.to_owned(),
            data_type: Some(nt_type.to_owned()),
            field: field.cloned(),
        },
        y: None,
        color,
        name: None,
    })
}

/// The color of pair `i`: the Graph's line colors, in order.
fn color_of(i: usize) -> Color32 {
    if i == 0 {
        graph::DEFAULT_COLOR
    } else {
        graph::PALETTE[(i - 1) % graph::PALETTE.len()]
    }
}

/// The pairs a graph's settings `props` show: as saved, or (none saved)
/// the one its own topic makes.
///
/// ```
/// use raptaboard::widgets::builtin::xy_graph::pairs;
/// let p = serde_json::json!({"topic": "/Drive/pose", "data_type": "struct:Pose2d"});
/// let pairs = pairs(p.as_object().unwrap());
/// assert_eq!(pairs.len(), 1);
/// assert_eq!(pairs[0].label(), "pose");
/// ```
pub fn pairs(props: &Map<String, Json>) -> Vec<Pair> {
    if let Some(saved) = props.get(PAIRS).and_then(Json::as_array) {
        return saved
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                Some(Pair {
                    x: Source::from_json(item.get("x")?)?,
                    y: item.get("y").and_then(Source::from_json),
                    color: item
                        .as_object()
                        .and_then(|o| props::color_property(o, "color"))
                        .unwrap_or_else(|| color_of(i)),
                    name: item.get("name").and_then(Json::as_str).map(str::to_owned),
                })
            })
            .collect();
    }
    let Some(topic) = props.get("topic").and_then(Json::as_str) else {
        return Vec::new();
    };
    let nt_type = props::text(props, DATA_TYPE, "double");
    let field = props.get("struct_meta").and_then(struct_meta_of);
    pair_from(topic, nt_type, field.as_ref(), color_of(0))
        .into_iter()
        .collect()
}

/// Save `pairs` into a graph's settings.
pub fn write_pairs(props: &mut Map<String, Json>, pairs: &[Pair]) {
    let saved: Vec<Json> = pairs
        .iter()
        .map(|pair| {
            let mut item = Map::new();
            item.insert("x".into(), pair.x.to_json());
            if let Some(y) = &pair.y {
                item.insert("y".into(), y.to_json());
            }
            item.insert("color".into(), props::color_json(pair.color));
            if let Some(name) = &pair.name {
                item.insert("name".into(), name.clone().into());
            }
            Json::Object(item)
        })
        .collect();
    set_property(props, PAIRS, Some(saved.into()));
}

/// Add dropped `topics` to a graph's pairs: a struct with a known pair (X/Y
/// only) is a new pair; a number finishes the last pair if it's waiting for
/// its second value, else starts a new one. Returns how many were taken.
fn add_to_pairs(props: &mut Map<String, Json>, topics: &[DraggedTopic], kind: Kind) -> usize {
    let mut all = pairs(props);
    let mut taken = 0;
    for t in topics {
        let color = color_of(all.len());
        let Some(new) = pair_from(&t.name, &t.ty, t.field.as_ref(), color) else {
            continue;
        };
        if new.y.is_some() && kind == Kind::Polar {
            continue;
        }
        match all.last_mut() {
            Some(last) if last.y.is_none() && new.y.is_none() => last.y = Some(new.x),
            _ => all.push(new),
        }
        taken += 1;
    }
    if taken > 0 {
        write_pairs(props, &all);
    }
    taken
}

/// Pair two series of (time, value), each sorted by time: at each time
/// either changes, once both have a value, (time, a's last value, b's last
/// value). Values at the same time make one point.
///
/// ```
/// use raptaboard::widgets::builtin::xy_graph::paired;
/// let a = [(0, 1.0), (10, 2.0)];
/// let b = [(5, 7.0), (10, 8.0), (20, 9.0)];
/// assert_eq!(paired(&a, &b), [(5, 1.0, 7.0), (10, 2.0, 8.0), (20, 2.0, 9.0)]);
/// ```
pub fn paired(a: &[(i64, f64)], b: &[(i64, f64)]) -> Vec<(i64, f64, f64)> {
    let (mut i, mut j) = (0, 0);
    let (mut last_a, mut last_b) = (None, None);
    let mut out: Vec<(i64, f64, f64)> = Vec::new();
    while i < a.len() || j < b.len() {
        let t = match (a.get(i), b.get(j)) {
            (Some(p), Some(q)) => p.0.min(q.0),
            (Some(p), None) => p.0,
            (None, Some(q)) => q.0,
            (None, None) => break,
        };
        while i < a.len() && a[i].0 == t {
            last_a = Some(a[i].1);
            i += 1;
        }
        while j < b.len() && b[j].0 == t {
            last_b = Some(b[j].1);
            j += 1;
        }
        if let (Some(x), Some(y)) = (last_a, last_b) {
            out.push((t, x, y));
        }
    }
    out
}

/// How a polar graph turns an angle and distance into a place on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarAxes {
    /// Angles are in radians (else degrees).
    pub radians: bool,
    /// Where 0 points, radians anticlockwise from the right.
    pub zero: f64,
    /// Angles grow clockwise.
    pub clockwise: bool,
}

impl PolarAxes {
    /// As a polar graph's settings say.
    pub fn of(props: &Map<String, Json>) -> PolarAxes {
        PolarAxes {
            radians: props::text(props, ANGLE_UNIT, "degrees") == "radians",
            zero: match props::text(props, ZERO_AT, "right") {
                "up" => FRAC_PI_2,
                "left" => PI,
                "down" => -FRAC_PI_2,
                _ => 0.0,
            },
            clockwise: props::flag(props, CLOCKWISE, false),
        }
    }

    /// The screen direction (radians anticlockwise from the right) of `angle`.
    fn direction(&self, angle: f64) -> f64 {
        let angle = if self.radians {
            angle
        } else {
            angle.to_radians()
        };
        self.zero + if self.clockwise { -angle } else { angle }
    }

    /// The plot's (x, y) for `distance` at `angle`.
    ///
    /// ```
    /// use raptaboard::widgets::builtin::xy_graph::PolarAxes;
    /// let up_clockwise = PolarAxes { radians: false, zero: std::f64::consts::FRAC_PI_2, clockwise: true };
    /// let [x, y] = up_clockwise.place(90.0, 2.0);
    /// assert!((x - 2.0).abs() < 1e-9 && y.abs() < 1e-9, "90° clockwise from up is right");
    /// ```
    pub fn place(&self, angle: f64, distance: f64) -> [f64; 2] {
        let d = self.direction(angle);
        [distance * d.cos(), distance * d.sin()]
    }

    /// The angle and distance at the plot's (x, y): [`place`](Self::place)
    /// undone, the angle from 0 up to a full turn.
    pub fn read(&self, x: f64, y: f64) -> (f64, f64) {
        let d = y.atan2(x) - self.zero;
        let angle = (if self.clockwise { -d } else { d }).rem_euclid(TAU);
        let angle = if self.radians {
            angle
        } else {
            angle.to_degrees()
        };
        (angle, x.hypot(y))
    }

    /// The label of the spoke `degrees` round from 0, in the graph's unit.
    fn spoke_label(&self, degrees: u32) -> String {
        if !self.radians {
            return format!("{degrees}°");
        }
        // Multiples of π/6 as fractions of π.
        let sixths = degrees / 30;
        let (n, d) = match sixths {
            0 => return "0".to_owned(),
            6 => return "π".to_owned(),
            s => {
                let g = gcd(s, 6);
                (s / g, 6 / g)
            }
        };
        let top = if n == 1 {
            "π".to_owned()
        } else {
            format!("{n}π")
        };
        if d == 1 { top } else { format!("{top}/{d}") }
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A round step for rings or grid lines so about 4 fit in `max`.
///
/// ```
/// use raptaboard::widgets::builtin::xy_graph::ring_step;
/// assert_eq!(ring_step(10.0), 2.0);
/// assert_eq!(ring_step(3.0), 1.0);
/// assert_eq!(ring_step(0.7), 0.2);
/// ```
pub fn ring_step(max: f64) -> f64 {
    let rough = (max / 4.0).max(f64::MIN_POSITIVE);
    let power = 10f64.powf(rough.log10().floor());
    let scaled = rough / power;
    let nice = if scaled < 1.5 {
        1.0
    } else if scaled < 3.5 {
        2.0
    } else if scaled < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * power
}

/// One pair as drawn: its name, color and points (seconds before now, x or
/// angle, y or distance), oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Trail {
    /// Its name in the legend.
    pub name: String,
    /// Its color.
    pub color: Color32,
    /// (seconds before now, first value, second value), oldest first.
    pub points: Vec<(f64, f64, f64)>,
}

/// Each finished pair's points from the traces: the last `seconds`, and at
/// least the newest point however old it is.
fn trails(widget: &WidgetContainer, live: &Live<'_>, seconds: f64) -> Vec<Trail> {
    let traces = live.frame.traces;
    let now = live.frame.now_us;
    pairs(&widget.properties)
        .into_iter()
        .filter_map(|pair| {
            let y = pair.y.as_ref()?;
            let a: Vec<(i64, f64)> = traces.points(&pair.x.key()).collect();
            let b: Vec<(i64, f64)> = traces.points(&y.key()).collect();
            let all = paired(&a, &b);
            let from = now - (seconds * 1e6) as i64;
            let first = all
                .iter()
                .position(|p| p.0 >= from)
                .unwrap_or(all.len())
                .min(all.len().saturating_sub(1));
            let points = all[first..]
                .iter()
                .map(|&(t, a, b)| ((t - now) as f64 / 1e6, a, b))
                .collect();
            Some(Trail {
                name: pair.label(),
                color: pair.color,
                points,
            })
        })
        .collect()
}

/// A trail cut into runs of the same fade, oldest first: (alpha, points).
/// Each run starts where the last ended, so the line is unbroken. The newest
/// is solid, the oldest [`FADE_LEAST`].
pub fn fade_runs(points: &[[f64; 2]], ages: &[f64], trail: f64) -> Vec<(f32, Vec<[f64; 2]>)> {
    let mut runs: Vec<(f32, Vec<[f64; 2]>)> = Vec::new();
    let step_of = |age: f64| -> usize {
        if trail <= 0.0 {
            return FADE_STEPS;
        }
        let fresh = (1.0 + age / trail).clamp(0.0, 1.0);
        ((fresh * FADE_STEPS as f64).ceil() as usize).clamp(1, FADE_STEPS)
    };
    for i in 1..points.len() {
        let step = step_of(ages[i]);
        let alpha = FADE_LEAST + (1.0 - FADE_LEAST) * step as f32 / FADE_STEPS as f32;
        match runs.last_mut() {
            Some((a, run)) if *a == alpha => run.push(points[i]),
            _ => runs.push((alpha, vec![points[i - 1], points[i]])),
        }
    }
    runs
}

/// What a graph keeps between frames, in egui's memory: the trails while
/// paused.
#[derive(Debug, Clone, Default)]
struct View {
    frozen: Option<Arc<Vec<Trail>>>,
}

/// A fixed range from two settings, if both are set the right way round.
fn range(props: &Map<String, Json>, low: &str, high: &str) -> Option<(f64, f64)> {
    let low = props.get(low).and_then(Json::as_f64)?;
    let high = props.get(high).and_then(Json::as_f64)?;
    (low < high).then_some((low, high))
}

/// The trail's seconds, as set.
fn trail_time(props: &Map<String, Json>) -> f64 {
    props::number(props, TRAIL_TIME, DEFAULT_TRAIL).clamp(0.0, MAX_TRAIL)
}

/// Draw either kind of graph.
fn show(kind: Kind, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
    let body = card::draw_card(
        ui.painter(),
        ui.visuals(),
        rect,
        &widget.title,
        &widget.properties,
    )
    .shrink(card::BODY_MARGIN);
    let p = &widget.properties;
    let all = pairs(p);
    if !all.iter().any(|pair| pair.y.is_some()) {
        let (_, second) = kind.axes();
        let why = if all.is_empty() {
            "no topic".to_owned()
        } else {
            format!("drop a topic on for the {second}")
        };
        card::draw_message(ui.painter(), ui.visuals(), body, &why);
        return;
    }
    let view_id = ui.scope_id().with("xy_graph_view");
    let mut view: View = ui.ctx().data(|d| d.get_temp(view_id)).unwrap_or_default();
    let trail = trail_time(p);
    let paused = view.frozen.is_some();
    let shown: Arc<Vec<Trail>> = match &view.frozen {
        Some(frozen) => Arc::clone(frozen),
        None => Arc::new(trails(widget, live, trail)),
    };
    let width = props::number(p, LINE_WIDTH, DEFAULT_WIDTH).clamp(0.5, 20.0) as f32;
    let polar = PolarAxes::of(p);
    let place = |a: f64, b: f64| match kind {
        Kind::Xy => [a, b],
        Kind::Polar => polar.place(a, b),
    };

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    child.set_clip_rect(body.intersect(ui.clip_rect()));
    let mut plot = Plot::new("xy_graph")
        .sense(Sense::hover())
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .allow_axis_zoom_drag(false)
        .show_background(false)
        .show_x(false)
        .show_y(false)
        .width(body.width())
        .height(body.height());
    if kind == Kind::Polar || props::flag(p, SAME_SCALE, false) {
        plot = plot.data_aspect(1.0);
    }
    if kind == Kind::Polar {
        plot = plot.show_axes(false).show_grid(false);
    }
    if props::flag(p, SHOW_LEGEND, shown.len() > 1) {
        plot = plot.legend(Legend::default().position(Corner::LeftTop));
    }
    // The outer ring: as set, else round above the farthest point.
    let reach = props::number(p, MAX_DISTANCE, 0.0);
    let reach = if reach > 0.0 {
        reach
    } else {
        let far = shown
            .iter()
            .flat_map(|t| t.points.iter().map(|q| q.2.abs()))
            .fold(0.0, f64::max);
        let step = ring_step(far.max(1e-6));
        ((far / step).ceil().max(1.0)) * step
    };
    let weak = ui.visuals().weak_text_color();
    let grid = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let hover = props::flag(p, SHOW_HOVER, true);
    let out = plot.show(&mut child, |plot_ui| {
        match kind {
            Kind::Xy => {
                match range(p, X_MIN, X_MAX) {
                    Some((low, high)) => plot_ui.set_plot_bounds_x(low..=high),
                    None => plot_ui.set_auto_bounds([true, false]),
                }
                match range(p, Y_MIN, Y_MAX) {
                    Some((low, high)) => plot_ui.set_plot_bounds_y(low..=high),
                    None => plot_ui.set_auto_bounds([false, true]),
                }
                if range(p, X_MIN, X_MAX).is_none() && range(p, Y_MIN, Y_MAX).is_none() {
                    plot_ui.set_auto_bounds(true);
                }
            }
            Kind::Polar => {
                let edge = reach * 1.18;
                plot_ui.set_plot_bounds_x(-edge..=edge);
                plot_ui.set_plot_bounds_y(-edge..=edge);
                // Rings, spokes and their labels.
                let step = ring_step(reach);
                let mut r = step;
                while r <= reach * 1.0001 {
                    let circle: Vec<[f64; 2]> = (0..=96)
                        .map(|i| {
                            let a = TAU * f64::from(i) / 96.0;
                            [r * a.cos(), r * a.sin()]
                        })
                        .collect();
                    plot_ui.line(
                        Line::new("", PlotPoints::new(circle))
                            .color(grid)
                            .width(1.0),
                    );
                    let at = polar.place(0.0, r);
                    plot_ui.text(
                        Text::new("", PlotPoint::new(at[0], at[1]), number(r))
                            .color(weak)
                            .anchor(egui::Align2::LEFT_BOTTOM),
                    );
                    r += step;
                }
                for degrees in (0..360).step_by(SPOKE_DEGREES as usize) {
                    let angle = if polar.radians {
                        f64::from(degrees).to_radians()
                    } else {
                        f64::from(degrees)
                    };
                    let end = polar.place(angle, reach);
                    plot_ui.line(
                        Line::new("", PlotPoints::new(vec![[0.0, 0.0], end]))
                            .color(grid)
                            .width(1.0),
                    );
                    let label = polar.place(angle, reach * 1.09);
                    plot_ui.text(
                        Text::new(
                            "",
                            PlotPoint::new(label[0], label[1]),
                            polar.spoke_label(degrees),
                        )
                        .color(weak),
                    );
                }
            }
        }
        for t in shown.iter() {
            let points: Vec<[f64; 2]> = t.points.iter().map(|q| place(q.1, q.2)).collect();
            let ages: Vec<f64> = t.points.iter().map(|q| q.0).collect();
            for (alpha, run) in fade_runs(&points, &ages, trail) {
                plot_ui.line(
                    Line::new("", PlotPoints::new(run))
                        .color(t.color.gamma_multiply(alpha))
                        .width(width),
                );
            }
            if let Some(&newest) = points.last() {
                plot_ui.points(
                    Points::new(t.name.clone(), vec![newest])
                        .color(t.color)
                        .radius(DOT)
                        .filled(true),
                );
            }
        }
        let hovered = plot_ui.response().hovered();
        plot_ui
            .pointer_coordinate()
            .filter(|_| hover && hovered)
            .map(|at| (at.x, at.y))
    });

    // Hovering: where the pointer is, then each pair's newest values.
    if let Some((x, y)) = out.inner {
        let (first, second) = kind.axes();
        let (a, b) = match kind {
            Kind::Xy => (x, y),
            Kind::Polar => polar.read(x, y),
        };
        let mut job = egui::text::LayoutJob::default();
        let font = egui::FontId::monospace(11.0);
        let mut add = |text: String, color: Color32| {
            job.append(&text, 0.0, egui::TextFormat::simple(font.clone(), color));
        };
        add(
            format!("pointer {first} {}  {second} {}", number(a), number(b)),
            weak,
        );
        for t in shown.iter() {
            if let Some(&(_, a, b)) = t.points.last() {
                add(
                    format!(
                        "\n{}  {first} {}  {second} {}",
                        t.name,
                        number(a),
                        number(b)
                    ),
                    t.color,
                );
            }
        }
        let galley = ui.painter().layout_job(job);
        let size = galley.size();
        let at = egui::pos2(body.max.x - size.x - 4.0, body.max.y - size.y - 4.0);
        let back = Rect::from_min_size(at, size).expand(3.0);
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
            ("⏸", "Pause: freeze the picture; values keep recording")
        };
        if ui
            .put(place, egui::Button::new(label).small())
            .on_hover_text(hint)
            .clicked()
        {
            view.frozen = if paused {
                None
            } else {
                Some(Arc::clone(&shown))
            };
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(view_id, view));
}

/// A value for a label: up to 3 decimals, no trailing zeros.
fn number(v: f64) -> String {
    let text = format!("{v:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_owned()
}

/// Without a full Ui (in a List Layout, or while dragged): the newest value
/// as text, if it's a number.
fn draw_body(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, value: Option<&Value>) {
    match value.and_then(number_of) {
        Some(v) => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                number(v),
                egui::FontId::monospace(16.0),
                visuals.text_color(),
            );
        }
        None => card::draw_message(painter, visuals, rect, "shown on the grid"),
    }
}

/// Every topic a graph's pairs read, but its own.
fn extra_topics(props: &Map<String, Json>) -> Vec<String> {
    let own = props.get("topic").and_then(Json::as_str);
    let mut topics: Vec<String> = pairs(props)
        .into_iter()
        .flat_map(|pair| std::iter::once(pair.x).chain(pair.y))
        .map(|s| s.topic)
        .filter(|t| Some(t.as_str()) != own)
        .collect();
    topics.sort();
    topics.dedup();
    topics
}

/// What a graph records: each value of each pair, a second more than the
/// trail so the oldest point has a value to pair with.
fn traced(widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
    let seconds = trail_time(&widget.properties) + 1.0;
    pairs(&widget.properties)
        .into_iter()
        .filter(|pair| pair.y.is_some())
        .flat_map(|pair| std::iter::once(pair.x).chain(pair.y))
        .map(|s| (s.key(), seconds))
        .collect()
}

/// The dropped topics, taken as pairs, said in the status line.
fn add_topics(kind: Kind, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
    match add_to_pairs(&mut widget.properties, topics, kind) {
        0 => match kind {
            Kind::Xy => "Nothing to add to that graph (numbers, or a Translation, Pose or \
                         ChassisSpeeds)"
                .to_owned(),
            Kind::Polar => "Nothing to add to that graph (numbers only)".to_owned(),
        },
        n => format!(
            "Added {n} value{} to \"{}\"",
            if n == 1 { "" } else { "s" },
            widget.title
        ),
    }
}

/// The settings both kinds share, then each one's own.
fn settings(kind: Kind, ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    props::number_row(
        ui,
        p,
        "Trail (s)",
        TRAIL_TIME,
        DEFAULT_TRAIL,
        0.0..=MAX_TRAIL,
    );
    match kind {
        Kind::Xy => {
            range_row(ui, p, "X range", X_MIN, X_MAX);
            range_row(ui, p, "Y range", Y_MIN, Y_MAX);
            props::flag_row(
                ui,
                p,
                "Same scale",
                "A unit as long across as up (for places on the floor)",
                SAME_SCALE,
                false,
            );
        }
        Kind::Polar => {
            ui.label("Distance");
            ui.horizontal(|ui| {
                let mut fixed = p.get(MAX_DISTANCE).and_then(Json::as_f64).is_some();
                if ui
                    .checkbox(&mut fixed, "Fixed")
                    .on_hover_text("Off: fit the values shown")
                    .changed()
                {
                    set_property(p, MAX_DISTANCE, fixed.then(|| 1.0.into()));
                }
                if let Some(mut most) = p.get(MAX_DISTANCE).and_then(Json::as_f64) {
                    let edit = egui::DragValue::new(&mut most)
                        .speed(0.1)
                        .range(0.001..=f64::MAX)
                        .prefix("up to ");
                    if ui.add(edit).changed() {
                        set_property(p, MAX_DISTANCE, Some(most.into()));
                    }
                }
            });
            ui.end_row();
            props::choice_row(
                ui,
                p,
                "Angle in",
                ANGLE_UNIT,
                "degrees",
                &["degrees", "radians"],
            );
            props::choice_row(
                ui,
                p,
                "Zero points",
                ZERO_AT,
                "right",
                &["right", "up", "left", "down"],
            );
            props::flag_row(
                ui,
                p,
                "Clockwise",
                "Angles grow clockwise",
                CLOCKWISE,
                false,
            );
        }
    }
    props::number_row(ui, p, "Line width", LINE_WIDTH, DEFAULT_WIDTH, 0.5..=20.0);
    let count = pairs(p).len();
    props::flag_row(ui, p, "Legend", "Show the legend", SHOW_LEGEND, count > 1);
    props::flag_row(ui, p, "Pause", "Show the pause button", SHOW_PAUSE, true);
    props::flag_row(
        ui,
        p,
        "Hover",
        "Show values where the pointer is",
        SHOW_HOVER,
        true,
    );

    // Every pair: color, name, and Remove.
    let (first, second) = kind.axes();
    ui.label("Pairs");
    ui.vertical(|ui| {
        let mut all = pairs(p);
        let count = all.len();
        let mut changed = false;
        let mut remove = None;
        for (i, pair) in all.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                changed |= egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut pair.color,
                    egui::color_picker::Alpha::Opaque,
                )
                .changed();
                let what = match &pair.y {
                    Some(y) => format!("{first}: {}\n{second}: {}", pair.x.topic, y.topic),
                    None => format!("{first}: {}", pair.x.topic),
                };
                ui.label(pair.label()).on_hover_text(what);
                if pair.y.is_none() {
                    ui.weak(format!("drop a topic on for the {second}"));
                }
                if ui
                    .add_enabled(count > 1, egui::Button::new("Remove").small())
                    .on_disabled_hover_text("A graph needs one pair")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            all.remove(i);
            changed = true;
        }
        if changed {
            write_pairs(p, &all);
        }
        ui.weak(format!(
            "Drop topics from the tree onto the graph: the first number is the {first}, the \
             next the {second}."
        ));
    });
    ui.end_row();
}

/// A fixed range row: Fixed, then from and to.
fn range_row(ui: &mut egui::Ui, p: &mut Map<String, Json>, label: &str, low: &str, high: &str) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut fixed = range(p, low, high).is_some();
        if ui
            .checkbox(&mut fixed, "Fixed")
            .on_hover_text("Off: fit the values shown")
            .changed()
        {
            let ends = if fixed {
                (Some(-1.0), Some(1.0))
            } else {
                (None, None)
            };
            set_property(p, low, ends.0.map(Json::from));
            set_property(p, high, ends.1.map(Json::from));
        }
        if let Some((mut from, mut to)) = range(p, low, high) {
            let a = ui.add(egui::DragValue::new(&mut from).speed(0.1).prefix("from "));
            let b = ui.add(egui::DragValue::new(&mut to).speed(0.1).prefix("to "));
            if (a.changed() || b.changed()) && from < to {
                set_property(p, low, Some(from.into()));
                set_property(p, high, Some(to.into()));
            }
        }
    });
    ui.end_row();
}

impl WidgetType for XyGraph {
    fn names(&self) -> &'static [&'static str] {
        &["X/Y Graph"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_number(nt_type) || known_pair(nt_type).is_some()
    }
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        show(Kind::Xy, ui, rect, widget, live);
        None
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        draw_body(painter, visuals, rect, value);
    }
    fn default_period(&self) -> f64 {
        graph::DEFAULT_PERIOD_S
    }
    fn extra_topics(&self, props: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        extra_topics(props)
    }
    fn traced(&self, widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        traced(widget)
    }
    fn takes_topics(&self) -> bool {
        true
    }
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        add_topics(Kind::Xy, widget, topics)
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        settings(Kind::Xy, ui, p);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        // Not the pairs: Reset settings keeps what's shown.
        &XY_KEYS
    }
}

impl WidgetType for PolarGraph {
    fn names(&self) -> &'static [&'static str] {
        &["Polar Graph"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_number(nt_type)
    }
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        show(Kind::Polar, ui, rect, widget, live);
        None
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        draw_body(painter, visuals, rect, value);
    }
    fn default_period(&self) -> f64 {
        graph::DEFAULT_PERIOD_S
    }
    fn extra_topics(&self, props: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        extra_topics(props)
    }
    fn traced(&self, widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        traced(widget)
    }
    fn takes_topics(&self) -> bool {
        true
    }
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        add_topics(Kind::Polar, widget, topics)
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        settings(Kind::Polar, ui, p);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &POLAR_KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::widgets::{Frame, Place};
    use raptaboard_layout::Rect as LayoutRect;

    fn widget(topic: &str, nt_type: &str, kind: &str) -> WidgetContainer {
        let at = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 384.0,
            height: 384.0,
        };
        crate::widgets::for_topic_as(topic, nt_type, None, at, kind).unwrap()
    }

    fn dragged(name: &str, ty: &str) -> DraggedTopic {
        DraggedTopic {
            name: name.into(),
            ty: ty.into(),
            field: None,
        }
    }

    #[test]
    fn values_at_different_times_pair_by_holding_the_last() {
        assert!(paired(&[], &[(1, 1.0)]).is_empty());
        // b has nothing until 5: nothing before it.
        let a = [(0, 1.0), (3, 2.0), (3, 2.5), (8, 3.0)];
        let b = [(5, 10.0)];
        assert_eq!(paired(&a, &b), [(5, 2.5, 10.0), (8, 3.0, 10.0)]);
        // A struct's two fields arrive together: one point each time.
        let x = [(0, 1.0), (1, 2.0)];
        let y = [(0, 5.0), (1, 6.0)];
        assert_eq!(paired(&x, &y), [(0, 1.0, 5.0), (1, 2.0, 6.0)]);
    }

    #[test]
    fn numbers_dropped_on_finish_a_pair_then_start_the_next() {
        let mut w = widget("/Stick/x", "double", "X/Y Graph");
        let first = pairs(&w.properties);
        assert_eq!(first.len(), 1);
        assert!(first[0].y.is_none());
        assert!(traced(&w).is_empty(), "nothing to record until it's a pair");

        let said = XyGraph.add_topics(&mut w, &[dragged("/Stick/y", "double")]);
        assert_eq!(said, "Added 1 value to \"x\"");
        let now = pairs(&w.properties);
        assert_eq!(now[0].label(), "Stick");
        assert_eq!(now[0].y.as_ref().unwrap().topic, "/Stick/y");
        assert_eq!(extra_topics(&w.properties), ["/Stick/y"]);
        assert_eq!(traced(&w).len(), 2);

        // A Pose2d is a whole pair; the next number starts another.
        XyGraph.add_topics(
            &mut w,
            &[
                dragged("/Drive/pose", "struct:Pose2d"),
                dragged("/Arm/reach", "double"),
                dragged("/Raw/bytes", "raw"),
            ],
        );
        let names: Vec<String> = pairs(&w.properties).iter().map(Pair::label).collect();
        assert_eq!(names, ["Stick", "pose", "reach"]);
        let colors: Vec<Color32> = pairs(&w.properties).iter().map(|p| p.color).collect();
        assert_eq!(colors, [color_of(0), color_of(1), color_of(2)]);
        assert_eq!(
            extra_topics(&w.properties),
            ["/Arm/reach", "/Drive/pose", "/Stick/y"]
        );
        // Saved and read back the same.
        let again = pairs(&w.properties);
        write_pairs(&mut w.properties, &again);
        assert_eq!(pairs(&w.properties), again);

        // A polar graph takes numbers only.
        let mut polar = widget("/Turret/angle", "double", "Polar Graph");
        let said = PolarGraph.add_topics(&mut polar, &[dragged("/Drive/pose", "struct:Pose2d")]);
        assert!(said.starts_with("Nothing"), "{said}");
        PolarGraph.add_topics(&mut polar, &[dragged("/Turret/range", "double")]);
        assert_eq!(pairs(&polar.properties)[0].label(), "Turret");
    }

    #[test]
    fn a_pose_made_into_an_x_y_graph_is_a_pair_of_its_fields() {
        let w = widget("/Drive/pose", "struct:Pose2d", "X/Y Graph");
        let keys: Vec<SeriesKey> = traced(&w).into_iter().map(|(k, _)| k).collect();
        let path = |p: &[&str]| Some(p.iter().map(|s| (*s).to_owned()).collect());
        assert_eq!(keys[0].field, path(&["translation", "x"]));
        assert_eq!(keys[1].field, path(&["translation", "y"]));
        assert!(crate::widgets::types_showing("struct:Pose2d").contains(&"X/Y Graph"));
        assert!(!crate::widgets::types_showing("struct:Pose2d").contains(&"Polar Graph"));
        assert!(crate::widgets::types_showing("double").contains(&"Polar Graph"));
    }

    #[test]
    fn polar_places_read_back_for_every_setting() {
        for zero in ["right", "up", "left", "down"] {
            for clockwise in [false, true] {
                for unit in ["degrees", "radians"] {
                    let p = serde_json::json!({
                        "zero_at": zero, "clockwise": clockwise, "angle_unit": unit,
                    });
                    let axes = PolarAxes::of(p.as_object().unwrap());
                    let angle = if unit == "degrees" { 100.0 } else { 1.75 };
                    let [x, y] = axes.place(angle, 3.0);
                    let (a, d) = axes.read(x, y);
                    assert!((a - angle).abs() < 1e-9, "{zero} {clockwise} {unit}: {a}");
                    assert!((d - 3.0).abs() < 1e-9);
                }
            }
        }
        // 0 up and clockwise, as a compass or a turret: 90 is to the right.
        let p = serde_json::json!({"zero_at": "up", "clockwise": true});
        let [x, y] = PolarAxes::of(p.as_object().unwrap()).place(90.0, 1.0);
        assert!((x - 1.0).abs() < 1e-9 && y.abs() < 1e-9);
        // The usual: 0 right, anticlockwise, so 90 is up.
        let [x, y] = PolarAxes::of(&Map::new()).place(90.0, 1.0);
        assert!(x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spokes_are_labelled_in_the_graphs_unit() {
        let degrees = PolarAxes::of(&Map::new());
        assert_eq!(degrees.spoke_label(270), "270°");
        let p = serde_json::json!({"angle_unit": "radians"});
        let radians = PolarAxes::of(p.as_object().unwrap());
        let labels: Vec<String> = (0..360)
            .step_by(30)
            .map(|d| radians.spoke_label(d))
            .collect();
        assert_eq!(
            labels,
            [
                "0", "π/6", "π/3", "π/2", "2π/3", "5π/6", "π", "7π/6", "4π/3", "3π/2", "5π/3",
                "11π/6"
            ]
        );
    }

    #[test]
    fn a_trail_fades_from_old_to_new_without_breaks() {
        let points: Vec<[f64; 2]> = (0..30).map(|i| [f64::from(i), 0.0]).collect();
        let ages: Vec<f64> = (0..30).map(|i| f64::from(i - 29) * 0.1).collect();
        let runs = fade_runs(&points, &ages, 3.0);
        assert!(runs.len() > 3, "{} runs", runs.len());
        // Each run starts where the last ended, and gets more solid.
        for w in runs.windows(2) {
            assert_eq!(w[0].1.last(), w[1].1.first());
            assert!(w[0].0 < w[1].0);
        }
        assert_eq!(runs.last().unwrap().0, 1.0);
        let drawn: usize = runs.iter().map(|(_, r)| r.len() - 1).sum();
        assert_eq!(drawn, 29, "every segment drawn once");
    }

    /// Draw `w` with `traces` at `now_us` (clicking at `click`, if any);
    /// the texts and the dots' centers.
    fn draw(
        ctx: &egui::Context,
        w: &WidgetContainer,
        traces: &crate::traces::Traces,
        now_us: i64,
        click: Option<egui::Pos2>,
    ) -> (Vec<String>, Vec<egui::Pos2>) {
        use egui::{Event, PointerButton};
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0));
        let events = click
            .map(|at| {
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
            })
            .unwrap_or_default();
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            let (others, effects) = (Default::default(), Default::default());
            let frame = Frame {
                traces,
                now_us,
                others: &others,
                connected: true,
                effects: &effects,
                structs: None,
            };
            let live = frame.live(None, Place::default());
            let ty = crate::widgets::find(&w.widget_type).unwrap();
            ty.show(ui, screen, w, &live);
        });
        output.textures_delta.clear();
        let (mut texts, mut dots) = (Vec::new(), Vec::new());
        fn walk(shape: &egui::Shape, texts: &mut Vec<String>, dots: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
                egui::Shape::Circle(c) if c.radius == DOT => dots.push(c.center),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts, dots)),
                _ => {}
            }
        }
        for s in &output.shapes {
            walk(&s.shape, &mut texts, &mut dots);
        }
        (texts, dots)
    }

    fn record(traces: &mut crate::traces::Traces, w: &WidgetContainer, from: i64, to: i64) {
        for (key, seconds) in traced(w) {
            traces.want(&key, seconds);
        }
        let all = pairs(&w.properties);
        for pair in &all {
            let y = pair.y.as_ref().unwrap();
            for i in from..to {
                let t = i * 100_000;
                let a = i as f64 * 0.1;
                traces.record(&pair.x.key(), t, a.cos() * 30.0);
                traces.record(&y.key(), t, 1.0 + a.sin());
            }
        }
    }

    #[test]
    fn an_x_y_graph_draws_each_pair_and_pauses() {
        let mut w = widget("/Stick/x", "double", "X/Y Graph");
        let ctx = egui::Context::default();
        let empty = crate::traces::Traces::default();
        let (texts, _) = draw(&ctx, &w, &empty, 0, None);
        assert!(
            texts.iter().any(|t| t == "drop a topic on for the y"),
            "{texts:?}"
        );

        XyGraph.add_topics(&mut w, &[dragged("/Stick/y", "double")]);
        XyGraph.add_topics(
            &mut w,
            &[dragged("/A/x", "double"), dragged("/A/y", "double")],
        );
        let mut traces = crate::traces::Traces::default();
        record(&mut traces, &w, 0, 50);
        let (texts, dots) = draw(&ctx, &w, &traces, 5_000_000, None);
        for name in ["Stick", "A", "⏸"] {
            assert!(texts.iter().any(|t| t == name), "{name} in {texts:?}");
        }
        assert_eq!(dots.len(), 2, "a dot for each pair's newest point");

        // Paused, newer values don't move the dots.
        let button = egui::pos2(
            400.0 - card::BODY_MARGIN - 8.0,
            card::TITLE_HEIGHT + card::BODY_MARGIN + 6.0,
        );
        draw(&ctx, &w, &traces, 5_000_000, Some(button));
        let (texts, frozen) = draw(&ctx, &w, &traces, 5_000_000, None);
        assert!(texts.iter().any(|t| t == "▶"), "{texts:?}");
        record(&mut traces, &w, 50, 70);
        let (_, still) = draw(&ctx, &w, &traces, 7_000_000, None);
        assert_eq!(frozen, still);
        // Live again, they move.
        draw(&ctx, &w, &traces, 7_000_000, Some(button));
        let (_, live) = draw(&ctx, &w, &traces, 7_000_000, None);
        assert_ne!(live, still);
    }

    #[test]
    fn a_polar_graph_draws_rings_spokes_and_its_point() {
        let mut w = widget("/Turret/angle", "double", "Polar Graph");
        PolarGraph.add_topics(&mut w, &[dragged("/Turret/range", "double")]);
        let mut traces = crate::traces::Traces::default();
        record(&mut traces, &w, 0, 50);
        let ctx = egui::Context::default();
        let (texts, dots) = draw(&ctx, &w, &traces, 5_000_000, None);
        for label in ["0°", "90°", "330°", "1", "2"] {
            assert!(texts.iter().any(|t| t == label), "{label} in {texts:?}");
        }
        assert_eq!(dots.len(), 1);
        w.properties.insert(ANGLE_UNIT.into(), "radians".into());
        let (texts, _) = draw(&ctx, &w, &traces, 5_000_000, None);
        assert!(texts.iter().any(|t| t == "3π/2"), "{texts:?}");
    }
}
