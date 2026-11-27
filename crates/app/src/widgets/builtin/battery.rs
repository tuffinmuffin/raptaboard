//! Battery (raptaboard): the robot's battery voltage as a battery icon,
//! filled by charge (the voltage between `min_value` and `max_value`) and
//! colored by level: green, yellow below `warn_below`, red below
//! `low_below`. The volts and the charge are written on it. It lies on its
//! side on a wide card and stands up on a tall one.
//!
//! Bound to a number of volts (such as robot code's battery voltage); "Add
//! as" in the tree, or the settings dialog's Widget list, makes one (a
//! number dragged from the tree makes a Text Display).

use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::number_bar::{MAX_VALUE, MIN_VALUE};
use crate::widgets::shared::format::number_of;
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, ValueInfo, WidgetType};

/// Below this many volts the battery is yellow (raptaboard).
pub const WARN_BELOW: &str = "warn_below";
/// Below this many volts it's red (raptaboard).
pub const LOW_BELOW: &str = "low_below";

const KEYS: [&str; 4] = [MIN_VALUE, MAX_VALUE, WARN_BELOW, LOW_BELOW];

/// An empty battery, volts (a 12 V lead-acid battery under load).
const DEFAULT_EMPTY: f64 = 11.0;
/// A full one, volts (charged, at rest).
const DEFAULT_FULL: f64 = 13.0;
const DEFAULT_WARN: f64 = 12.0;
const DEFAULT_LOW: f64 = 11.5;

const GREEN: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
const YELLOW: Color32 = Color32::from_rgb(0xFF, 0xC1, 0x07);
const RED: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);

/// The Battery widget.
pub struct Battery;

/// How full a battery at `volts` is, 0 to 1, between `empty` and `full`.
///
/// ```
/// use raptaboard::widgets::builtin::battery::charge;
/// assert_eq!(charge(12.0, 11.0, 13.0), 0.5);
/// assert_eq!(charge(14.0, 11.0, 13.0), 1.0);
/// assert_eq!(charge(9.0, 11.0, 13.0), 0.0);
/// ```
pub fn charge(volts: f64, empty: f64, full: f64) -> f64 {
    if full <= empty {
        return if volts >= full { 1.0 } else { 0.0 };
    }
    ((volts - empty) / (full - empty)).clamp(0.0, 1.0)
}

/// The battery's color at `volts`: red below `low`, yellow below `warn`,
/// else green.
fn level_color(volts: f64, warn: f64, low: f64) -> Color32 {
    if volts < low {
        RED
    } else if volts < warn {
        YELLOW
    } else {
        GREEN
    }
}

/// Draw the battery in `rect`: its outline and terminal, filled to
/// `charge` (0 to 1) in `color`, standing up if `rect` is taller than wide.
fn draw_battery(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    charge: f64,
    color: Color32,
) {
    let upright = rect.height() > rect.width();
    // The body is twice as long as it's wide, plus the terminal.
    let (long, short) = if upright {
        (rect.height(), rect.width())
    } else {
        (rect.width(), rect.height())
    };
    let thick = (short * 0.9).min(long / 2.2);
    let length = thick * 2.0;
    let nub = (thick * 0.12).max(3.0);
    let size = if upright {
        egui::vec2(thick, length)
    } else {
        egui::vec2(length, thick)
    };
    let body = Rect::from_center_size(rect.center(), size);
    let ink = visuals.text_color();
    let edge = (thick * 0.06).clamp(1.5, 4.0);
    let round = CornerRadius::same((thick * 0.12).clamp(2.0, 10.0) as u8);
    // The terminal, at the top or the right.
    let terminal = if upright {
        Rect::from_center_size(
            egui::pos2(body.center().x, body.min.y - nub / 2.0),
            egui::vec2(thick * 0.4, nub),
        )
    } else {
        Rect::from_center_size(
            egui::pos2(body.max.x + nub / 2.0, body.center().y),
            egui::vec2(nub, thick * 0.4),
        )
    };
    painter.rect_filled(terminal, 1.0, ink);
    painter.rect_filled(body, round, visuals.extreme_bg_color);
    // The charge, from the bottom (upright) or the left.
    let inner = body.shrink(edge * 1.8);
    let fill = charge as f32;
    let level = if upright {
        Rect::from_min_max(
            egui::pos2(inner.min.x, inner.max.y - inner.height() * fill),
            inner.max,
        )
    } else {
        Rect::from_min_max(
            inner.min,
            egui::pos2(inner.min.x + inner.width() * fill, inner.max.y),
        )
    };
    if fill > 0.0 {
        painter.rect_filled(level, CornerRadius::same(2), color);
    }
    painter.rect_stroke(
        body,
        round,
        Stroke::new(edge, ink),
        egui::StrokeKind::Inside,
    );
}

/// The text on the battery: `12.45 V` and the charge, `73%`.
fn label(volts: f64, charge: f64) -> (String, String) {
    (format!("{volts:.2} V"), format!("{:.0}%", charge * 100.0))
}

impl Battery {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let p = &widget.properties;
        let Some(volts) = live.value.and_then(number_of) else {
            let why = if live.value.is_some() {
                "not a number"
            } else {
                "no value"
            };
            draw_battery(&painter, &visuals, rect, 0.0, GREEN);
            card::draw_message(&painter, &visuals, rect, why);
            let text = format!("{}: {why}", widget.title);
            ui.interact(rect, ui.scope_id().with("battery"), Sense::hover())
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text)
                });
            return;
        };
        let empty = props::number(p, MIN_VALUE, DEFAULT_EMPTY);
        let full = props::number(p, MAX_VALUE, DEFAULT_FULL);
        let warn = props::number(p, WARN_BELOW, DEFAULT_WARN);
        let low = props::number(p, LOW_BELOW, DEFAULT_LOW);
        let charge = charge(volts, empty, full);
        let color = level_color(volts, warn, low);
        draw_battery(&painter, &visuals, rect, charge, color);
        let (v, pct) = label(volts, charge);
        let size = (rect.width().min(rect.height()) * 0.18).clamp(10.0, 36.0);
        // The text on a see-through backing, so it reads over the fill.
        let volts_text = painter.layout_no_wrap(
            v.clone(),
            egui::FontId::monospace(size),
            visuals.text_color(),
        );
        let pct_text = painter.layout_no_wrap(
            pct.clone(),
            egui::FontId::proportional(size * 0.7),
            visuals.text_color(),
        );
        let gap = size * 0.1;
        let height = volts_text.size().y + gap + pct_text.size().y;
        let width = volts_text.size().x.max(pct_text.size().x);
        let block = Rect::from_center_size(rect.center(), egui::vec2(width, height));
        painter.rect_filled(
            block.expand(size * 0.2),
            CornerRadius::same((size * 0.25) as u8),
            visuals.extreme_bg_color.gamma_multiply(0.8),
        );
        painter.galley(
            egui::pos2(block.center().x - volts_text.size().x / 2.0, block.min.y),
            volts_text,
            visuals.text_color(),
        );
        painter.galley(
            egui::pos2(
                block.center().x - pct_text.size().x / 2.0,
                block.max.y - pct_text.size().y,
            ),
            pct_text,
            visuals.text_color(),
        );
        let level = if volts < low {
            "low"
        } else if volts < warn {
            "warning"
        } else {
            "ok"
        };
        let text = format!("{}: {v}, {pct}, {level}", widget.title);
        ui.interact(rect, ui.scope_id().with("battery"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text));
    }
}

impl WidgetType for Battery {
    fn names(&self) -> &'static [&'static str] {
        &["Battery"]
    }

    /// Not a default: a number makes a Text Display.
    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int")
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(
            ui.painter(),
            ui.visuals(),
            rect,
            &widget.title,
            &widget.properties,
        );
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
        draw_battery(painter, visuals, rect, 0.7, GREEN);
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::number_row(ui, p, "Empty (V)", MIN_VALUE, DEFAULT_EMPTY, 0.0..=30.0);
        props::number_row(ui, p, "Full (V)", MAX_VALUE, DEFAULT_FULL, 0.0..=30.0);
        props::number_row(
            ui,
            p,
            "Yellow below (V)",
            WARN_BELOW,
            DEFAULT_WARN,
            0.0..=30.0,
        );
        props::number_row(ui, p, "Red below (V)", LOW_BELOW, DEFAULT_LOW, 0.0..=30.0);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn the_color_follows_the_level_and_the_text_rounds() {
        assert_eq!(level_color(12.6, 12.0, 11.5), GREEN);
        assert_eq!(level_color(11.9, 12.0, 11.5), YELLOW);
        assert_eq!(level_color(11.2, 12.0, 11.5), RED);
        assert_eq!(
            label(12.456, 0.728),
            ("12.46 V".to_owned(), "73%".to_owned())
        );
        // A range set backwards is full at or above "full", else empty,
        // not a division by zero.
        assert_eq!(charge(12.0, 13.0, 11.0), 1.0);
        assert_eq!(charge(10.0, 13.0, 11.0), 0.0);
        assert!(Battery.shows("double") && !Battery.accepts("double"));
    }
}
