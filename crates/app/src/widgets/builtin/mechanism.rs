//! Mechanism2d: WPILib's `Mechanism2d` drawing of an arm, elevator or
//! other mechanism, as robot code publishes it (read by
//! [`crate::types::mechanism`]): each ligament as a line in its color and
//! weight from where the one before it ends, y up, on the canvas's color.
//!
//! The view fills the card at one scale for x and y (angles stay true). By
//! default it zooms to the part of the canvas the mechanism has used (its
//! roots and every place a ligament has reached, plus a margin), which only
//! grows (a ligament counts once its angle and length have both arrived), so a tall elevator fills a tall card and the view doesn't jump as
//! it moves; a double-click starts it over. `fit` set to `canvas` shows the
//! whole canvas (`dims`) instead.
//!
//! Bound to the Mechanism2d's table (`.type` "Mechanism2d"); dragging the
//! table from the tree makes one. Ligament weights are WPILib's line
//! widths, scaled with the card.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::color_view::parse_color;
use crate::types::mechanism::{self, MECHANISM_TYPE, Mechanism};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// What the view fits (raptaboard): `mechanism` (the part of the canvas
/// it has used) or `canvas` (all of `dims`).
pub const FIT: &str = "fit";
/// [`FIT`]'s values.
const FIT_MECHANISM: &str = "mechanism";
const FIT_CANVAS: &str = "canvas";

const KEYS: [&str; 1] = [FIT];

/// A ligament's color when it has none (WPILib's default, orange).
const DEFAULT_LINE: Color32 = Color32::from_rgb(0xFF, 0xA5, 0x00);
/// The card size (the side of a square of the same area), points, at
/// which a weight is drawn that many points wide; on a bigger or smaller
/// card lines scale with it.
const WEIGHT_AT: f32 = 300.0;
/// The margin round the mechanism, as a part of its larger side.
const MARGIN: f64 = 0.1;

/// The Mechanism2d widget.
pub struct Mechanism2d;

/// An area of the canvas: left, bottom, right, top (canvas units, y up).
type Area = [f64; 4];

/// The part of the canvas `m` uses now: its roots and every ligament's
/// ends.
fn used(m: &Mechanism) -> Option<Area> {
    let points = m
        .roots
        .iter()
        .map(|r| (r.x, r.y))
        .chain(m.segments().into_iter().flat_map(|s| [s.start, s.end]));
    points.fold(None, |area: Option<Area>, (x, y)| {
        Some(match area {
            None => [x, y, x, y],
            Some([l, b, r, t]) => [l.min(x), b.min(y), r.max(x), t.max(y)],
        })
    })
}

/// `a` and `b` together.
fn union(a: Area, b: Area) -> Area {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// `area` with a margin of [`MARGIN`] of its larger side (or of the
/// canvas's, for a mechanism with no size yet) all round.
fn padded(area: Area, m: &Mechanism) -> Area {
    let side = (area[2] - area[0]).max(area[3] - area[1]);
    let side = if side > 1e-9 {
        side
    } else {
        m.width.max(m.height)
    };
    let pad = side * MARGIN;
    [area[0] - pad, area[1] - pad, area[2] + pad, area[3] + pad]
}

/// How canvas units land on screen: an area centered in a rect, as big as
/// fits at one scale for x and y.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    /// The canvas point at the rect's center.
    middle: (f64, f64),
    /// The rect's center.
    center: Pos2,
    /// Points per canvas unit.
    scale: f32,
}

impl View {
    fn fit(rect: Rect, area: Area) -> View {
        let (w, h) = ((area[2] - area[0]).max(1e-9), (area[3] - area[1]).max(1e-9));
        View {
            middle: ((area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0),
            center: rect.center(),
            scale: (rect.width() / w as f32).min(rect.height() / h as f32),
        }
    }

    /// A canvas point (y up) on screen.
    fn to_screen(self, p: (f64, f64)) -> Pos2 {
        self.center
            + egui::vec2(
                (p.0 - self.middle.0) as f32 * self.scale,
                -((p.1 - self.middle.1) as f32) * self.scale,
            )
    }
}

/// Draw `m` in `rect` as `view` shows it: the canvas's color over the
/// whole rect, then the ligaments.
fn draw(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, m: &Mechanism, view: View) {
    let background = m
        .background
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(visuals.extreme_bg_color);
    painter.rect_filled(rect, 2.0, background);
    // By the card's area, so a tall or wide card draws lines as a square
    // one of the same size does.
    let scale = ((rect.width() * rect.height()).sqrt() / WEIGHT_AT).clamp(0.3, 2.0);
    for s in m.segments() {
        let color = s
            .color
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(DEFAULT_LINE);
        let width = (s.weight as f32 * scale).clamp(1.0, 40.0);
        let (a, b) = (view.to_screen(s.start), view.to_screen(s.end));
        painter.line_segment([a, b], Stroke::new(width, color));
        // Round the joints, as WPILib's sim GUI does.
        painter.circle_filled(a, width / 2.0, color);
        painter.circle_filled(b, width / 2.0, color);
    }
}

/// `n` to `places` decimals, never `-0`.
fn rounded(n: f64, places: usize) -> String {
    let text = format!("{n:.places$}");
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => text,
    }
}

/// The widget's summary, for screen readers and tests: each ligament's
/// path, length and its own angle: `base/elevator: 0.80 at 90°`.
fn summary(title: &str, m: Option<&Mechanism>) -> String {
    let Some(m) = m else {
        return format!("{title}: no mechanism");
    };
    let parts: Vec<String> = m
        .segments()
        .iter()
        .map(|s| {
            format!(
                "{}: {} at {}°",
                s.path,
                rounded(s.length, 2),
                rounded(s.angle, 0)
            )
        })
        .collect();
    if parts.is_empty() {
        format!("{title}: no ligaments")
    } else {
        format!("{title}: {}", parts.join("; "))
    }
}

/// The area shown, as text, for screen readers (and the tests): `showing
/// 1.20 to 1.80 across, 0.10 to 1.30 up`.
fn area_text(a: Area) -> String {
    format!(
        "showing {} to {} across, {} to {} up",
        rounded(a[0], 2),
        rounded(a[2], 2),
        rounded(a[1], 2),
        rounded(a[3], 2)
    )
}

impl Mechanism2d {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let m = widget
            .topic()
            .and_then(|t| mechanism::mechanism_of(t, live.frame.others));
        let id = ui.scope_id().with("mechanism");
        let response = ui.interact(rect, id, Sense::click());
        // The area the mechanism has used, kept while the program runs; a
        // double-click starts it over.
        if response.double_clicked() {
            ui.data_mut(|d| d.remove::<Area>(id));
        }
        let mut shown = None;
        match &m {
            Some(m) => {
                let fit = props::text(&widget.properties, FIT, FIT_MECHANISM);
                let area = if fit == FIT_CANVAS {
                    [0.0, 0.0, m.width, m.height]
                } else {
                    let seen: Option<Area> = ui.data(|d| d.get_temp(id));
                    // What the mechanism has really used is kept, without
                    // the margin (added to whatever is kept, so it fits
                    // all of it); the whole canvas, shown before any root
                    // arrives, isn't, or the view could never zoom in again.
                    let kept = match (seen, used(m)) {
                        (Some(s), Some(n)) => Some(union(s, n)),
                        (s, n) => s.or(n),
                    };
                    match kept {
                        Some(area) => {
                            ui.data_mut(|d| d.insert_temp(id, area));
                            padded(area, m)
                        }
                        None => [0.0, 0.0, m.width, m.height],
                    }
                };
                draw(&painter, &visuals, rect, m, View::fit(rect, area));
                shown = Some(area_text(area));
            }
            None => card::draw_message(&painter, &visuals, rect, "no mechanism"),
        }
        let text = summary(&widget.title, m.as_ref());
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text);
            info.current_text_value = shown.clone();
            info
        });
    }
}

impl WidgetType for Mechanism2d {
    fn names(&self) -> &'static [&'static str] {
        &["Mechanism2d"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[MECHANISM_TYPE]
    }

    /// Everything under the table, whatever ligaments robot code adds.
    fn extra_topics(&self, p: &Map<String, Json>, server: Server<'_>) -> Vec<String> {
        let table = props::text(p, "topic", "").trim();
        if table.is_empty() {
            return Vec::new();
        }
        mechanism::table_topics(table, server.topics.iter().map(|t| t.name.as_ref()))
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live);
        None
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, rect, widget, live);
        None
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        card::draw_message(painter, visuals, rect, "Mechanism2d");
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::choice_row(
            ui,
            p,
            "Fit",
            FIT,
            FIT_MECHANISM,
            &[FIT_MECHANISM, FIT_CANVAS],
        );
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::types::mechanism::{Ligament, Root};

    fn lift() -> Mechanism {
        let arm = Ligament {
            name: "arm".into(),
            angle: -45.0,
            length: 0.5,
            color: Some("#00FF00".into()),
            weight: 4.0,
            children: vec![],
        };
        Mechanism {
            width: 3.0,
            height: 2.0,
            background: None,
            roots: vec![Root {
                name: "base".into(),
                x: 1.5,
                y: 0.0,
                ligaments: vec![Ligament {
                    name: "elevator".into(),
                    angle: 90.0,
                    length: 0.8,
                    color: None,
                    weight: 6.0,
                    children: vec![arm],
                }],
            }],
        }
    }

    #[test]
    fn the_view_fills_the_card_at_one_scale_with_y_up() {
        let m = lift();
        // A tall card and a tall mechanism (about 0.6 across, 1.4 up with
        // its margin): it fills the height.
        let tall = Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 400.0));
        let area = padded(used(&m).unwrap(), &m);
        let view = View::fit(tall, area);
        let bottom = view.to_screen((1.5, area[1]));
        let top = view.to_screen((1.5, area[3]));
        assert!(
            (bottom.y - 400.0).abs() < 1e-3 && top.y.abs() < 1e-3,
            "{bottom:?} {top:?}"
        );
        // One scale both ways, y up: a 45° line stays 45°.
        let a = view.to_screen((1.5, 0.5));
        let b = view.to_screen((1.6, 0.6));
        assert!(((b.x - a.x) + (b.y - a.y)).abs() < 1e-3);
        assert!(b.x > a.x && b.y < a.y);
        // The whole canvas on the same card is far smaller.
        let canvas = View::fit(tall, [0.0, 0.0, 3.0, 2.0]);
        assert!(canvas.scale < view.scale / 3.0);
    }

    #[test]
    fn the_used_area_covers_roots_and_ligament_ends_and_only_grows() {
        let m = lift();
        let a = used(&m).unwrap();
        // From the base up the elevator (0.8), then the arm at 45° up and
        // to the right (0.5 long).
        let reach = 0.5 * std::f64::consts::FRAC_1_SQRT_2;
        assert!((a[0] - 1.5).abs() < 1e-9 && a[1].abs() < 1e-9);
        assert!((a[2] - (1.5 + reach)).abs() < 1e-9);
        assert!((a[3] - (0.8 + reach)).abs() < 1e-9);
        let wider = union(a, [0.0, 0.0, 2.0, 0.5]);
        assert_eq!(wider[0], 0.0);
        assert_eq!(wider[3], a[3]);
    }

    #[test]
    fn the_summary_lists_each_ligament_as_robot_code_set_it() {
        assert_eq!(
            summary("Lift", Some(&lift())),
            "Lift: base/elevator: 0.80 at 90°; base/elevator/arm: 0.50 at -45°"
        );
        assert_eq!(summary("Lift", None), "Lift: no mechanism");
        assert!(Mechanism2d.table_types().contains(&"Mechanism2d"));
    }
}
