//! Dropdown Chooser and Split Button Chooser: pick one of a
//! SendableChooser's options, such as the auto mode, and send it to the
//! robot.
//!
//! Both are bound to the chooser's table (`topic` is the table, such as
//! `/SmartDashboard/Auto choices`), whose `.type` is `"String Chooser"`.
//! Robot code publishes `options` (every choice), `default` and `active`
//! (the choice it's using); the dashboard publishes `selected`, which robot
//! code reads. That's the same table WPILib's SendableChooser makes.
//!
//! What's shown as picked: `selected` if it's one of the options (whoever
//! sent it, this dashboard or another), else the robot's `active`, else
//! `default`. While the robot's `active` isn't the pick yet, the widget
//! says what the robot has. The robot publishes its whole list at the
//! start; if the list changes later, any pick is dropped and the robot's
//! own value is sent as the pick instead, so a choice made from the old
//! list doesn't linger.
//!
//! The Dropdown Chooser is a drop-down list (Elastic's `sort_options`
//! sorts it); the Split Button Chooser is a row of buttons, one per
//! option, so two options make a two-way switch. Both size their text to
//! the widget unless `font_size` is set (the "Text size" setting). How a
//! chooser table is read is in [`crate::types::chooser`].

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Rect, Sense, Stroke};
use nt4::{NtType, Value};
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::types::chooser::{CHOOSER_TYPE, Chooser, SELECTED, chooser_topics};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// Sort the options by name in the drop-down (Elastic; off unless set).
pub const SORT_OPTIONS: &str = "sort_options";

/// The color of the "robot hasn't taken it yet" note and outline.
const WAITING_COLOR: Color32 = Color32::from_rgb(0xFF, 0x98, 0x00);
/// Height of the buttons' text when there's room, in points.
const ROW_HEIGHT: f32 = 24.0;
/// The drop-down's height for its text size.
const ROW_PER_FONT: f32 = 1.7;
/// A fitted drop-down's text size against the widget's height.
const FIT_HEIGHT: f32 = 0.33;
/// Room beside the text for the drop-down's arrow and padding, in points.
const ARROW_ROOM: f32 = 40.0;
/// The smallest fitted text, in points.
pub const MIN_FIT: f32 = 12.0;
/// The largest fitted text, in points.
pub const MAX_FIT: f32 = 36.0;
/// Size of the note under the choice, in points.
const NOTE_SIZE: f32 = 12.0;
/// Space between buttons, in points.
const GAP: f32 = 4.0;

/// The chooser in `widget`'s table, and anything to send because the
/// robot's list changed since the last frame (remembered under `id`).
fn read_and_check(
    ui: &egui::Ui,
    id: egui::Id,
    widget: &WidgetContainer,
    live: &Live<'_>,
) -> (Chooser, Option<String>) {
    let Some(table) = widget.topic() else {
        return (Chooser::default(), None);
    };
    let chooser = Chooser::read(table, live.frame.others);
    let before: Option<Vec<String>> = ui.data(|d| d.get_temp(id));
    let dropped = before.and_then(|b| chooser.dropped_pick(&b));
    if !chooser.options.is_empty() {
        ui.data_mut(|d| d.insert_temp(id, chooser.options.clone()));
    }
    (chooser, dropped)
}

/// Why there's nothing to pick from, if there isn't.
fn nothing_to_pick(chooser: &Chooser) -> Option<&'static str> {
    chooser.options.is_empty().then_some("no options")
}

/// The note under the choice while the robot hasn't taken it.
fn draw_waiting(painter: &egui::Painter, rect: Rect, chooser: &Chooser) {
    let text = match &chooser.active {
        Some(active) => format!("Robot has: {active}"),
        None => "Robot hasn't taken it".to_owned(),
    };
    painter.text(
        rect.center_top() + egui::vec2(0.0, 2.0),
        egui::Align2::CENTER_TOP,
        text,
        egui::FontId::proportional(NOTE_SIZE),
        WAITING_COLOR,
    );
}

/// The drop-down's text size when it's fitted (no `font_size` set): about
/// a third of the widget's height, but no wider than the longest option
/// fits beside the arrow, and kept between [`MIN_FIT`] and [`MAX_FIT`] so
/// a very wide or very tall widget still gets usable text.
fn fitted_size(painter: &egui::Painter, rect: Rect, options: &[String]) -> f32 {
    const MEASURE: f32 = 100.0;
    let widest = options
        .iter()
        .map(|o| {
            painter
                .layout_no_wrap(
                    o.clone(),
                    egui::FontId::proportional(MEASURE),
                    Color32::WHITE,
                )
                .size()
                .x
        })
        .fold(1.0_f32, f32::max);
    let by_height = rect.height() * FIT_HEIGHT;
    let by_width = MEASURE * (rect.width() - ARROW_ROOM).max(0.0) / widest;
    fit_between(by_height.min(by_width))
}

/// A fitted text size kept usable: between [`MIN_FIT`] and [`MAX_FIT`]
/// points.
///
/// ```
/// use raptaboard::widgets::builtin::chooser::fit_between;
/// assert_eq!(fit_between(3.0), 12.0);
/// assert_eq!(fit_between(20.0), 20.0);
/// assert_eq!(fit_between(500.0), 36.0);
/// ```
pub fn fit_between(size: f32) -> f32 {
    size.clamp(MIN_FIT, MAX_FIT)
}

/// The `font_size` setting, if one is set; `None` fits the text to the
/// widget.
fn fixed_size(p: &Map<String, Json>) -> Option<f32> {
    p.contains_key(props::FONT_SIZE)
        .then(|| props::font_size(p))
}

/// The "Text size" settings row: fitted to the widget (no `font_size`),
/// or a set size.
pub(crate) fn text_size_row(ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    ui.label("Text size");
    ui.horizontal(|ui| {
        let mut fit = !p.contains_key(props::FONT_SIZE);
        if ui
            .checkbox(&mut fit, "Fit to the widget")
            .on_hover_text("Size the text to the widget's space; off sets a size")
            .changed()
        {
            let size = f64::from(props::DEFAULT_FONT_SIZE);
            props::set_property(p, props::FONT_SIZE, (!fit).then(|| size.into()));
        }
        if !fit {
            let mut size = props::font_size(p);
            if ui
                .add(
                    egui::DragValue::new(&mut size)
                        .range(props::MIN_FONT_SIZE..=props::MAX_FONT_SIZE)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" pt"),
                )
                .changed()
            {
                props::set_property(p, props::FONT_SIZE, Some(f64::from(size).into()));
            }
        }
    });
    ui.end_row();
}

/// The options in the order the widget lists them.
fn listed(chooser: &Chooser, p: &Map<String, Json>) -> Vec<String> {
    let mut options = chooser.options.clone();
    if props::flag(p, SORT_OPTIONS, false) {
        options.sort();
    }
    options
}

/// The Dropdown Chooser widget (Elastic's `"ComboBox Chooser"`).
pub struct DropdownChooser;

/// The Split Button Chooser widget (Elastic's `"Split Button Chooser"`).
pub struct SplitButtonChooser;

/// Draw a chooser's body without a `Ui` (while dragged): the pick, or why
/// there's none.
fn draw_pick(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, value: Option<&str>) {
    match value {
        Some(pick) => card::draw_fitted(painter, rect, pick, visuals.text_color()),
        None => card::draw_message(painter, visuals, rect, "no options"),
    }
}

/// The part of a list child's row to lay out in: beside an inside label.
fn row_area(rect: Rect, live: &Live<'_>) -> Rect {
    live.place.free.unwrap_or(rect)
}

/// What a drop-down or button row offers and shows.
#[derive(Clone, Copy)]
pub(crate) struct Picker<'a> {
    /// The widget's title, its name for screen readers.
    pub title: &'a str,
    /// Every option, in order.
    pub options: &'a [String],
    /// The text shown for the current choice.
    pub shown: &'a str,
    /// The option that's picked, if any.
    pub current: Option<&'a str>,
    /// The robot hasn't taken the pick yet (an orange outline).
    pub waiting: bool,
    /// It takes picks (else it only shows).
    pub enabled: bool,
}

/// A drop-down of the picker's options in `rect`, sized to it (or to the
/// widget's `font_size`), showing its `shown` and with its `current` ticked
/// in the list; greyed out and closed unless `enabled`. The option picked
/// this frame, if any, and where the drop-down's button is.
pub(crate) fn dropdown(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    picker: &Picker<'_>,
    p: &Map<String, Json>,
) -> (Option<String>, Rect) {
    let Picker {
        title,
        options,
        shown,
        current,
        enabled,
        ..
    } = *picker;
    let font = match fixed_size(p) {
        Some(size) => size,
        None => fitted_size(ui.painter(), rect, options),
    };
    let row = (font * ROW_PER_FONT).min(rect.height());
    let combo = Rect::from_min_size(
        rect.min
            + egui::vec2(
                0.0,
                ((rect.height() - row) / 2.0 - NOTE_SIZE / 2.0).max(0.0),
            ),
        egui::vec2(rect.width(), row),
    );
    // The text and the button's height follow the size; the list that
    // opens is a popup with a style of its own, sized the same.
    let size_up = |ui: &mut egui::Ui| {
        let style = ui.style_mut();
        style.override_font_id = Some(egui::FontId::proportional(font));
        style.spacing.interact_size.y = row;
        style.spacing.button_padding.y = ((row - font) / 2.0).max(1.0);
    };
    let mut chosen = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(combo), |ui| {
        size_up(ui);
        if !enabled {
            ui.disable();
        }
        egui::ComboBox::from_id_salt(id)
            .width(combo.width())
            .selected_text(shown)
            .show_ui(ui, |ui| {
                size_up(ui);
                for option in options {
                    if ui
                        .selectable_label(current == Some(option.as_str()), option)
                        .clicked()
                    {
                        chosen = Some(option.clone());
                    }
                }
            })
            .response
            .widget_info(|| {
                // Named for screen readers, with the choice shown as its
                // value.
                let mut info =
                    egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, enabled, title);
                info.current_text_value = Some(shown.to_owned());
                info
            });
    });
    (chosen, combo)
}

/// Whether a chooser with settings `p` takes picks (`editable`, on unless
/// set). Off, it only shows the robot's choice.
fn editable(p: &Map<String, Json>) -> bool {
    props::flag(p, props::EDITABLE, true)
}

/// The hover text of the choosers' Editable setting.
const EDITABLE_HOVER: &str = "On, a pick is sent to the robot. Off, it only shows the \
                              robot's choice (its active option), for watching without \
                              changing it.";

impl DropdownChooser {
    /// The drop-down in `rect`. The choice to send, if any.
    fn show_in(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let id = ui.scope_id().with("dropdown_chooser");
        let p = &widget.properties;
        let (chooser, dropped) = read_and_check(ui, id.with("options"), widget, live);
        let editable = editable(p);
        // Only a widget that sends can drop a stale pick.
        let mut send = dropped.filter(|_| editable);
        if let Some(why) = nothing_to_pick(&chooser) {
            card::draw_message(ui.painter(), ui.visuals(), rect, why);
            return send.map(Value::String);
        }
        let pick = if editable {
            chooser.pick()
        } else {
            chooser.robot_pick()
        }
        .map(str::to_owned);
        let options = listed(&chooser, p);
        let shown = pick.clone().unwrap_or_default();
        let picker = Picker {
            title: &widget.title,
            options: &options,
            shown: &shown,
            current: pick.as_deref(),
            waiting: false,
            enabled: editable,
        };
        let (chosen, combo) = dropdown(ui, rect, id, &picker, p);
        if editable && chooser.waiting() {
            draw_waiting(ui.painter(), rect.with_min_y(combo.max.y), &chooser);
        }
        // A pick sends, unless it's the one already sent.
        if let Some(choice) = chosen
            && editable
            && chooser.selected.as_ref() != Some(&choice)
        {
            send = Some(choice);
        }
        send.map(Value::String)
    }
}

impl WidgetType for DropdownChooser {
    fn names(&self) -> &'static [&'static str] {
        &[
            "Dropdown Chooser",
            "ComboBox Chooser",
            "Selectable",
            "String Chooser",
        ]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[CHOOSER_TYPE]
    }

    fn sends_to(&self, _: &Map<String, Json>) -> Option<(&'static str, NtType)> {
        Some((SELECTED, NtType::String))
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        table_topics(p)
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live)
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, row_area(rect, live), widget, live)
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        // Without the table's values (while dragged): just the frame.
        draw_pick(painter, visuals, rect, Some("Chooser"));
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::flag_row(
            ui,
            p,
            "Order",
            "Sort the options by name",
            SORT_OPTIONS,
            false,
        );
        text_size_row(ui, p);
        props::editable_row(ui, p, true, EDITABLE_HOVER);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &[SORT_OPTIONS, props::FONT_SIZE, props::EDITABLE]
    }
}

/// A row of buttons in `rect`, one per option, with `current` lit (and
/// outlined in orange if `waiting`), sized to it unless the widget has a
/// `font_size`. Buttons only take clicks if `enabled`. The option clicked
/// this frame, if any, and the bottom of the buttons.
pub(crate) fn button_row(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    picker: &Picker<'_>,
    p: &Map<String, Json>,
) -> (Option<String>, f32) {
    let Picker {
        options,
        current,
        waiting,
        enabled,
        ..
    } = *picker;
    let fixed = fixed_size(p);
    let pick = current.map(str::to_owned);
    let n = options.len() as f32;
    let height =
        (rect.height() - NOTE_SIZE - 4.0).clamp(ROW_HEIGHT.min(rect.height()), ROW_HEIGHT * 2.0);
    let width = ((rect.width() - GAP * (n - 1.0)) / n).max(1.0);
    let top = rect.min.y + ((rect.height() - height - NOTE_SIZE) / 2.0).max(0.0);
    let visuals = ui.visuals().clone();
    let mut chosen = None;
    for (i, option) in options.iter().enumerate() {
        let button = Rect::from_min_size(
            egui::pos2(rect.min.x + i as f32 * (width + GAP), top),
            egui::vec2(width, height),
        );
        let response = ui.interact(
            button,
            id.with(i),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, option)
        });
        if enabled && response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            chosen = Some(option.clone());
        }
        let on = pick.as_ref() == Some(option);
        let fill = if on {
            visuals.selection.bg_fill
        } else if enabled && response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let radius = CornerRadius::same(card::BODY_CORNER_RADIUS);
        ui.painter().rect_filled(button, radius, fill);
        if on && waiting {
            ui.painter().rect_stroke(
                button,
                radius,
                Stroke::new(2.0, WAITING_COLOR),
                egui::StrokeKind::Inside,
            );
        }
        let color = props::text_color_on(fill);
        match fixed {
            Some(size) => {
                ui.painter()
                    .with_clip_rect(button.intersect(ui.clip_rect()))
                    .text(
                        button.center(),
                        egui::Align2::CENTER_CENTER,
                        option,
                        egui::FontId::proportional(size),
                        color,
                    );
            }
            None => card::draw_fitted(ui.painter(), button.shrink(4.0), option, color),
        }
    }
    (chosen, top + height)
}

impl SplitButtonChooser {
    /// The buttons in `rect`. The choice to send, if any.
    fn show_in(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let id = ui.scope_id().with("split_button_chooser");
        let p = &widget.properties;
        let (chooser, dropped) = read_and_check(ui, id.with("options"), widget, live);
        let editable = editable(p);
        let mut send = dropped.filter(|_| editable);
        if let Some(why) = nothing_to_pick(&chooser) {
            card::draw_message(ui.painter(), ui.visuals(), rect, why);
            return send.map(Value::String);
        }
        let pick = if editable {
            chooser.pick()
        } else {
            chooser.robot_pick()
        };
        let waiting = editable && chooser.waiting();
        let picker = Picker {
            title: &widget.title,
            options: &chooser.options,
            shown: pick.unwrap_or_default(),
            current: pick,
            waiting,
            enabled: editable,
        };
        let (chosen, bottom) = button_row(ui, rect, id, &picker, p);
        if waiting {
            draw_waiting(ui.painter(), rect.with_min_y(bottom), &chooser);
        }
        if let Some(choice) = chosen
            && chooser.selected.as_ref() != Some(&choice)
        {
            send = Some(choice);
        }
        send.map(Value::String)
    }
}

impl WidgetType for SplitButtonChooser {
    fn names(&self) -> &'static [&'static str] {
        &["Split Button Chooser"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[CHOOSER_TYPE]
    }

    fn sends_to(&self, _: &Map<String, Json>) -> Option<(&'static str, NtType)> {
        Some((SELECTED, NtType::String))
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        table_topics(p)
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live)
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, row_area(rect, live), widget, live)
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        draw_pick(painter, visuals, rect, Some("Chooser"));
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        text_size_row(ui, p);
        props::editable_row(ui, p, true, EDITABLE_HOVER);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &[props::FONT_SIZE, props::EDITABLE]
    }
}

/// The chooser topics a widget with settings `p` needs: those under its
/// `topic`, the table.
fn table_topics(p: &Map<String, Json>) -> Vec<String> {
    p.get("topic")
        .and_then(Json::as_str)
        .map_or_else(Vec::new, chooser_topics)
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn fitted_text_stays_usable_in_odd_shapes() {
        let options = vec!["Do nothing".to_string(), "Two piece".to_string()];
        let ctx = egui::Context::default();
        let mut sizes = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let size = |w: f32, h: f32| {
                let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
                fitted_size(ui.painter(), rect, &options)
            };
            sizes = vec![
                size(2000.0, 30.0), // very wide, very short
                size(60.0, 2000.0), // very tall, very thin
                size(4000.0, 4000.0),
                size(500.0, 90.0),
            ];
        });
        out.textures_delta.clear();
        assert_eq!(sizes[0], MIN_FIT);
        assert_eq!(sizes[1], MIN_FIT);
        assert_eq!(sizes[2], MAX_FIT);
        // In between it follows the height: a third of 90.
        assert!((sizes[3] - 90.0 * FIT_HEIGHT).abs() < 0.1, "{}", sizes[3]);
    }
}
