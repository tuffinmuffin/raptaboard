//! Enum Dropdown and Enum Toggle: a topic that holds one of a few named
//! states (an intake's mode, an arm's position), shown by name and changed
//! from the dashboard.
//!
//! The topic is a plain string or int topic that robot code publishes and
//! reads back, so both sides can change it: the newest value wins, from the
//! robot or any dashboard. The states' names come from one of
//! (`enum_source`):
//!
//! - `list`: listed in the widget's settings (`enum_values`), one per line.
//!   For a string topic each line is a value. For an int topic a line is
//!   `3 = Climb`, or just `Climb` for the number after the line before
//!   (counting from 0).
//! - `topic`: a string[] topic robot code publishes (`options_topic`): its
//!   strings are the values of a string topic, or the names of 0, 1, 2 ...
//!   for an int topic.
//! - `chooser`: the widget's topic is a SendableChooser table, used like
//!   the [choosers](super::chooser) (the pick goes to `selected`).
//!
//! When a widget becomes one of these in its settings, it guesses the
//! source ([`guess`]): a chooser table if the topic is one, else a string[]
//! topic beside it named for it (`modeOptions`, `mode_names`, `modes` for
//! `mode`), else a list. "Guess again" in the settings does it over.
//!
//! The Enum Dropdown lists the states; the Enum Toggle is one button with
//! the state's name that steps to the next state on each click, so two
//! states make a switch between them. A value that isn't a listed state is
//! shown as `unnamed (7)` (or the text itself), with nothing picked. With
//! `editable` off (on unless set) they only show the state. Struct enum
//! fields aren't sent (that would rewrite the robot's struct); a Text
//! Display shows their names.

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Rect, Sense};
use nt4::{NtType, Value};
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::chooser;
use crate::types::chooser::{self as chooser_table, Chooser};
use crate::types::enums::{
    Source, State, guess, listed_states, locate, named_states, next_state, pair_target,
};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// Where the states' names come from: `list`, `topic` or `chooser`.
pub const ENUM_SOURCE: &str = "enum_source";
/// The states listed in the settings, one string per line.
pub const ENUM_VALUES: &str = "enum_values";
/// The string[] topic holding the states' names.
pub const OPTIONS_TOPIC: &str = "options_topic";
/// The two states an Enum Toggle switches between, by name, like a Toggle
/// Button's off and on; not set, it steps through every state.
pub const SWITCHES_BETWEEN: &str = "switches_between";

const KEYS: [&str; 5] = [
    ENUM_SOURCE,
    ENUM_VALUES,
    OPTIONS_TOPIC,
    props::EDITABLE,
    props::FONT_SIZE,
];
const TOGGLE_KEYS: [&str; 6] = [
    ENUM_SOURCE,
    ENUM_VALUES,
    OPTIONS_TOPIC,
    props::EDITABLE,
    props::FONT_SIZE,
    SWITCHES_BETWEEN,
];

/// The source set in `p` (`list` if none).
fn source_of(p: &Map<String, Json>) -> Source {
    Source::from_key(props::text(p, ENUM_SOURCE, "list"))
}

/// How the settings name `source`.
fn source_label(source: Source) -> &'static str {
    match source {
        Source::List => "A list here",
        Source::Topic => "A topic of names (string[])",
        Source::Chooser => "A chooser table",
    }
}

/// What a widget shows: its states, which is current, and what to say
/// otherwise.
#[derive(Debug, Clone, Default)]
struct Shown {
    states: Vec<State>,
    current: Option<usize>,
    /// The value, when it isn't one of the states (`unnamed (7)`).
    unnamed: Option<String>,
    /// The chooser, for a chooser table.
    chooser: Option<Chooser>,
}

/// Whether the widget's value is an int (else it's text): from the value,
/// or the saved `data_type` before there's one.
fn is_whole(widget: &WidgetContainer, value: Option<&Value>) -> bool {
    match value {
        Some(Value::Int(_)) => true,
        Some(_) => false,
        None => props::text(&widget.properties, props::DATA_TYPE, "") == "int",
    }
}

/// The lines of the `enum_values` setting.
fn value_lines(p: &Map<String, Json>) -> Vec<&str> {
    p.get(ENUM_VALUES)
        .and_then(Json::as_array)
        .map(|lines| lines.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default()
}

/// What `widget` shows now, or why it can't show anything.
fn shown(widget: &WidgetContainer, live: &Live<'_>) -> Result<Shown, &'static str> {
    let p = &widget.properties;
    let whole = is_whole(widget, live.value);
    let editable = props::flag(p, props::EDITABLE, true);
    let states = match source_of(p) {
        Source::List => listed_states(&value_lines(p), whole),
        Source::Topic => {
            let topic = props::text(p, OPTIONS_TOPIC, "");
            match live.frame.others.get(topic) {
                Some(Value::StringArray(names)) => named_states(names, whole),
                _ => return Err("no names yet (from the options topic)"),
            }
        }
        Source::Chooser => {
            let table = widget.topic().unwrap_or_default();
            let c = Chooser::read(table, live.frame.others);
            let pick = if editable { c.pick() } else { c.robot_pick() };
            let states = named_states(&c.options, false);
            let current = pick.and_then(|p| c.options.iter().position(|o| o == p));
            return Ok(Shown {
                states,
                current,
                unnamed: None,
                chooser: Some(c),
            });
        }
    };
    if states.is_empty() {
        return Err("no states: list them in its settings");
    }
    let (current, unnamed) = locate(&states, live.value);
    Ok(Shown {
        states,
        current,
        unnamed,
        chooser: None,
    })
}

/// The text shown for the current state.
fn shown_text(s: &Shown) -> String {
    match (s.current, &s.unnamed) {
        (Some(i), _) => s.states[i].name.clone(),
        (None, Some(text)) => text.clone(),
        (None, None) => "no value".to_owned(),
    }
}

/// Put a guess for `p` into it: the source and options topic.
fn apply_guess(p: &mut Map<String, Json>, topics: &[(String, String)]) {
    let topic = props::text(p, "topic", "").to_owned();
    let (source, options) = guess(&topic, topics);
    props::set_property(p, ENUM_SOURCE, Some(source.key().into()));
    props::set_property(p, OPTIONS_TOPIC, options.map(Into::into));
}

/// The value to send for state `i` of `s`: a chooser's option goes to its
/// `selected` as text, anything else as the state's value.
fn send_state(s: &Shown, i: usize) -> Option<Value> {
    let state = s.states.get(i)?;
    // Picking the chooser's pick again sends nothing.
    if let Some(c) = &s.chooser
        && c.selected.as_deref() == Some(state.name.as_str())
    {
        return None;
    }
    Some(state.value.clone())
}

/// The example and the help under the "States" box, for an int topic
/// (`whole`) or a string one.
fn list_help(whole: bool) -> (&'static str, &'static str) {
    if whole {
        (
            "Idle\nIntake\n5 = Shoot",
            "The topic is a number. Type one state per line: its name, which stands for 0, \
             1, 2 ... in order, or `5 = Shoot` to give the number. The widget shows the name \
             and sends the number.",
        )
    } else {
        (
            "Stowed\nIntaking\nShooting",
            "The topic is text. Type one state per line, spelled exactly as robot code sends \
             it. The widget shows and sends that text.",
        )
    }
}

/// The settings both enum widgets have: where the names come from, the
/// list or topic, Guess again, Editable and Text size.
fn enum_settings(ui: &mut egui::Ui, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
    ui.label("Names from");
    ui.horizontal(|ui| {
        let mut source = source_of(p);
        egui::ComboBox::from_id_salt("enum_source")
            .selected_text(source_label(source))
            .show_ui(ui, |ui| {
                for s in [Source::List, Source::Topic, Source::Chooser] {
                    ui.selectable_value(&mut source, s, source_label(s));
                }
            });
        if source != source_of(p) {
            props::set_property(p, ENUM_SOURCE, Some(source.key().into()));
        }
        if ui
            .button("Guess again")
            .on_hover_text("Look for a chooser table, or a string[] topic of names beside this one")
            .clicked()
        {
            apply_guess(p, value.topics);
        }
    });
    ui.end_row();
    match source_of(p) {
        Source::List => {
            let whole = value.nt_type.map_or_else(
                || props::text(p, props::DATA_TYPE, "") == "int",
                |t| t == "int",
            );
            ui.label("States");
            ui.vertical(|ui| {
                let mut text = value_lines(p).join("\n");
                let (hint, help) = list_help(whole);
                let edit = egui::TextEdit::multiline(&mut text)
                    .desired_rows(4)
                    .desired_width(240.0)
                    .hint_text(hint);
                if ui.add(edit).changed() {
                    let lines: Vec<Json> =
                        text.split('\n').map(|l| Json::from(l.to_owned())).collect();
                    props::set_property(p, ENUM_VALUES, Some(Json::Array(lines)));
                }
                ui.set_max_width(240.0);
                ui.add(egui::Label::new(egui::RichText::new(help).weak()).wrap());
            });
            ui.end_row();
        }
        Source::Topic => {
            ui.label("Names topic");
            let mut name = props::text(p, OPTIONS_TOPIC, "").to_owned();
            egui::ComboBox::from_id_salt("options_topic")
                .selected_text(if name.is_empty() {
                    "(pick a string[] topic)"
                } else {
                    &name
                })
                .show_ui(ui, |ui| {
                    for (topic, ty) in value.topics {
                        if ty == "string[]" {
                            ui.selectable_value(&mut name, topic.clone(), topic);
                        }
                    }
                });
            if name != props::text(p, OPTIONS_TOPIC, "") {
                props::set_property(p, OPTIONS_TOPIC, Some(name.into()));
            }
            ui.end_row();
        }
        Source::Chooser => {}
    }
    props::editable_row(
        ui,
        p,
        true,
        "On, picking a state sends it. Off, it only shows the state (for watching a \
         subsystem without changing it).",
    );
}

/// The Enum Dropdown widget (raptaboard's): the states in a drop-down.
pub struct EnumDropdown;

/// The Enum Toggle widget (raptaboard's): one button that steps through
/// the states.
pub struct EnumToggle;

/// The NT types the enum widgets show.
fn is_enum_type(nt_type: &str) -> bool {
    matches!(nt_type, "string" | "int")
}

/// The topics both widgets need besides their own: the options topic, or
/// the chooser table's.
fn enum_topics(p: &Map<String, Json>) -> Vec<String> {
    match source_of(p) {
        Source::List => Vec::new(),
        Source::Topic => vec![props::text(p, OPTIONS_TOPIC, "").to_owned()],
        Source::Chooser => chooser_table::chooser_topics(props::text(p, "topic", "")),
    }
}

/// Where both widgets send: a chooser table's `selected`, else the topic.
fn enum_sends_to(p: &Map<String, Json>) -> Option<(&'static str, NtType)> {
    (source_of(p) == Source::Chooser).then_some((chooser_table::SELECTED, NtType::String))
}

impl EnumDropdown {
    fn show_in(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let s = match shown(widget, live) {
            Ok(s) => s,
            Err(why) => {
                card::draw_message(ui.painter(), ui.visuals(), rect, why);
                return None;
            }
        };
        let p = &widget.properties;
        let names: Vec<String> = s.states.iter().map(|s| s.name.clone()).collect();
        let current = s.current.map(|i| names[i].clone());
        let shown = shown_text(&s);
        let picker = chooser::Picker {
            title: &widget.title,
            options: &names,
            shown: &shown,
            current: current.as_deref(),
            waiting: false,
            enabled: props::flag(p, props::EDITABLE, true),
        };
        let id = ui.scope_id().with("enum_dropdown");
        let (chosen, _) = chooser::dropdown(ui, rect, id, &picker, p);
        let i = names.iter().position(|n| Some(n) == chosen.as_ref())?;
        // Picking the state it's in sends nothing.
        (Some(i) != s.current).then(|| send_state(&s, i)).flatten()
    }
}

impl WidgetType for EnumDropdown {
    fn names(&self) -> &'static [&'static str] {
        &["Enum Dropdown"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_enum_type(nt_type)
    }
    fn sends_to(&self, p: &Map<String, Json>) -> Option<(&'static str, NtType)> {
        enum_sends_to(p)
    }
    fn chosen(&self, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        apply_guess(p, value.topics);
    }
    /// A chooser's table too (its options as the states).
    fn table_types(&self) -> &'static [&'static str] {
        &[crate::types::chooser::CHOOSER_TYPE]
    }
    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        enum_topics(p)
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
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live)
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, live.place.free.unwrap_or(rect), widget, live)
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        card::draw_fitted(
            painter,
            rect.shrink(rect.height() * 0.3),
            "Enum",
            visuals.weak_text_color(),
        );
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        enum_settings(ui, p, value);
        chooser::text_size_row(ui, p);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

/// The dots under the Enum Toggle's name: one per state, `current` filled.
fn draw_dots(
    painter: &egui::Painter,
    center: egui::Pos2,
    n: usize,
    current: Option<usize>,
    color: Color32,
) {
    const SPACING: f32 = 12.0;
    let left = center.x - SPACING * (n as f32 - 1.0) / 2.0;
    for i in 0..n {
        let at = egui::pos2(left + SPACING * i as f32, center.y);
        if Some(i) == current {
            painter.circle_filled(at, 3.5, color);
        } else {
            painter.circle_stroke(at, 3.0, egui::Stroke::new(1.0, color));
        }
    }
}

/// The names set in `switches_between`, if two are.
fn pair_names(p: &Map<String, Json>) -> Option<(&str, &str)> {
    let list = p.get(SWITCHES_BETWEEN)?.as_array()?;
    match list.as_slice() {
        [a, b] => Some((a.as_str()?, b.as_str()?)).filter(|(a, b)| !a.is_empty() && !b.is_empty()),
        _ => None,
    }
}

/// The two states (indexes into `s.states`) the toggle switches between,
/// if `switches_between` names two of them.
fn pair(p: &Map<String, Json>, s: &Shown) -> Option<(usize, usize)> {
    let (a, b) = pair_names(p)?;
    let find = |name: &str| s.states.iter().position(|st| st.name == name);
    Some((find(a)?, find(b)?))
}

impl EnumToggle {
    /// The button in `rect`, taking clicks in `area`. The value to send.
    fn show_in(
        &self,
        ui: &mut egui::Ui,
        area: Rect,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let s = match shown(widget, live) {
            Ok(s) => s,
            Err(why) => {
                card::draw_message(ui.painter(), ui.visuals(), rect, why);
                return None;
            }
        };
        let p = &widget.properties;
        let editable = props::flag(p, props::EDITABLE, true);
        let id = ui.scope_id().with("enum_toggle");
        let sense = if editable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(area, id, sense);
        let text = shown_text(&s);
        // Named by its title, with the state it shows as its value, so a
        // screen reader (and the tests) hear which state it's in.
        response.widget_info(|| {
            let mut info =
                egui::WidgetInfo::labeled(egui::accesskit::Role::Button, editable, &widget.title);
            info.current_text_value = Some(text.clone());
            info
        });
        if editable && response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let visuals = ui.visuals().clone();
        let pair = pair(p, &s);
        // Switching between two: lit on the second, like a Toggle Button
        // that's on.
        let fill = if pair.is_some_and(|(_, b)| s.current == Some(b)) {
            visuals.selection.bg_fill
        } else if editable && response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, CornerRadius::same(card::BODY_CORNER_RADIUS), fill);
        let dots = 14.0_f32.min(rect.height() * 0.25);
        let (name_area, dot_area) = rect.split_top_bottom_at_y(rect.max.y - dots);
        let color = if s.current.is_some() {
            props::text_color_on(fill)
        } else {
            visuals.warn_fg_color
        };
        match props::font_size_set(p) {
            Some(size) => {
                painter.text(
                    name_area.center(),
                    egui::Align2::CENTER_CENTER,
                    &text,
                    egui::FontId::proportional(size),
                    color,
                );
            }
            None => card::draw_fitted(
                &painter,
                name_area.shrink(name_area.height() * 0.15),
                &text,
                color,
            ),
        }
        // One dot per state it moves through.
        let (dot_count, dot) = match pair {
            Some((a, b)) => (2, [a, b].iter().position(|&i| Some(i) == s.current)),
            None => (s.states.len(), s.current),
        };
        draw_dots(
            &painter,
            dot_area.center(),
            dot_count,
            dot,
            props::text_color_on(fill),
        );
        if !(editable && response.clicked()) {
            return None;
        }
        let target = match pair {
            Some((a, b)) => pair_target(s.current, a, b),
            None => next_state(s.current, s.states.len()),
        };
        send_state(&s, target)
    }
}

impl WidgetType for EnumToggle {
    fn names(&self) -> &'static [&'static str] {
        &["Enum Toggle"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_enum_type(nt_type)
    }
    fn sends_to(&self, p: &Map<String, Json>) -> Option<(&'static str, NtType)> {
        enum_sends_to(p)
    }
    fn chosen(&self, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        apply_guess(p, value.topics);
    }
    /// A chooser's table too (its options as the states).
    fn table_types(&self) -> &'static [&'static str] {
        &[crate::types::chooser::CHOOSER_TYPE]
    }
    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        enum_topics(p)
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
        // Locked, the whole card takes the click; unlocked, the title bar
        // moves the widget.
        let area = if live.place.locked { rect } else { body };
        self.show_in(ui, area, body.shrink(card::BODY_MARGIN), widget, live)
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = live.place.free.unwrap_or(rect);
        self.show_in(ui, live.place.input.unwrap_or(rect), body, widget, live)
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        painter.rect_filled(
            rect,
            CornerRadius::same(card::BODY_CORNER_RADIUS),
            visuals.widgets.inactive.bg_fill,
        );
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        enum_settings(ui, p, value);
        switches_between_row(ui, p);
        chooser::text_size_row(ui, p);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &TOGGLE_KEYS
    }
}

/// The Enum Toggle's "Switches between" row: two states by name, picked
/// from the list when the states are listed in the settings, else typed.
/// Both empty: it steps through every state.
fn switches_between_row(ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    let hover = "Two states to switch between, like a Toggle Button's off and on: a click \
                 goes from the first to the second and back (from any other state, to the \
                 first). Empty, a click steps through every state.";
    ui.label("Switches between").on_hover_text(hover);
    let (mut a, mut b) = pair_names(p).map_or((String::new(), String::new()), |(a, b)| {
        (a.to_owned(), b.to_owned())
    });
    let known: Vec<String> = match source_of(p) {
        Source::List => listed_states(
            &value_lines(p),
            props::text(p, props::DATA_TYPE, "") == "int",
        )
        .into_iter()
        .map(|s| s.name)
        .collect(),
        _ => Vec::new(),
    };
    ui.horizontal(|ui| {
        for (which, name) in [("first", &mut a), ("second", &mut b)] {
            if known.is_empty() {
                ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text(which)
                        .desired_width(90.0),
                );
            } else {
                egui::ComboBox::from_id_salt(("switches_between", which))
                    .selected_text(if name.is_empty() {
                        "(every state)"
                    } else {
                        name.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(name, String::new(), "(every state)");
                        for state in &known {
                            ui.selectable_value(name, state.clone(), state);
                        }
                    });
            }
            if which == "first" {
                ui.label("and");
            }
        }
    })
    .response
    .on_hover_text(hover);
    let value = (!a.is_empty() || !b.is_empty()).then(|| Json::from(vec![a, b]));
    if value.as_ref() != p.get(SWITCHES_BETWEEN) {
        props::set_property(p, SWITCHES_BETWEEN, value);
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn values_not_listed_say_so() {
        let mut p = Map::new();
        p.insert(ENUM_VALUES.into(), serde_json::json!(["Idle", "Run"]));
        let widget = WidgetContainer {
            title: "x".into(),
            rect: raptaboard_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            widget_type: "Enum Toggle".into(),
            properties: p,
            extra: Map::new(),
        };
        let others = Default::default();
        let traces = crate::traces::Traces::default();
        let effects = Default::default();
        let frame = crate::widgets::Frame {
            traces: &traces,
            now_us: 0,
            others: &others,
            connected: true,
            effects: &effects,
            structs: None,
        };
        let place = crate::widgets::Place {
            locked: true,
            ..Default::default()
        };
        let live = |value| frame.live(value, place);
        let seven = Value::Int(7);
        let s = shown(&widget, &live(Some(&seven))).unwrap();
        assert_eq!((s.current, shown_text(&s).as_str()), (None, "unnamed (7)"));
        let one = Value::Int(1);
        let s = shown(&widget, &live(Some(&one))).unwrap();
        assert_eq!(shown_text(&s), "Run");
    }
}
