//! Struct (raptaboard): any WPILib struct, or array of them, as a list of
//! its fields, the way the tree shows them: nested structs indented under
//! their name, struct arrays as `[0]`, `[1]`, ..., enum fields by name (as
//! the schema names them). It scrolls when the list is longer than the
//! card.
//!
//! Every struct topic can be dragged from the tree: a struct some other
//! widget is made for (a Pose2d's Field, swerve states' swerve view) makes
//! that widget; any other (a team's own `ArmState`) makes this one. Values
//! are decoded with the server's schemas ([`crate::types::structs`]).

use eframe::egui::{self, Rect, RichText, Sense};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::types::structs::{self, Shown};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, ValueInfo, WidgetType};

const KEYS: [&str; 1] = [props::FONT_SIZE];

/// The text size when `font_size` isn't set, points: small, as the list can
/// be long.
const DEFAULT_TEXT_SIZE: f32 = 13.0;
/// How far each level of nesting is indented, as a part of the text size.
const INDENT: f32 = 1.2;

/// The Struct widget.
pub struct StructView;

/// One line of the list: how deep it is, the field's name, and its value
/// (a struct's or array's line has its kind, such as `Translation2d` or
/// `[4]`, and `node` set).
#[derive(Debug, Clone, PartialEq)]
struct Row {
    depth: usize,
    name: String,
    text: String,
    node: bool,
}

/// The lines for `shown`'s parts, `depth` deep, after `out`'s.
fn rows(children: &[(String, Shown)], depth: usize, out: &mut Vec<Row>) {
    for (name, shown) in children {
        match shown {
            Shown::Leaf(text) => out.push(Row {
                depth,
                name: name.clone(),
                text: text.clone(),
                node: false,
            }),
            Shown::Node { summary, children } => {
                out.push(Row {
                    depth,
                    name: name.clone(),
                    text: summary.clone(),
                    node: true,
                });
                rows(children, depth + 1, out);
            }
        }
    }
}

/// What the widget shows now: the struct's kind (`ArmState`,
/// `ArmState[3]`) and its lines, or why there's nothing.
fn decoded(widget: &WidgetContainer, live: &Live<'_>) -> Result<(String, Vec<Row>), &'static str> {
    let ty = props::text(&widget.properties, props::DATA_TYPE, "");
    let Some(value) = live.value else {
        return Err("no value");
    };
    let Value::Raw(bytes) = value else {
        return Err("not a struct");
    };
    let Some(db) = live.frame.structs else {
        return Err("waiting for the schema");
    };
    match structs::decode(db, ty, bytes) {
        Some(Shown::Node { summary, children }) => {
            let mut out = Vec::new();
            rows(&children, 0, &mut out);
            Ok((summary, out))
        }
        Some(Shown::Leaf(text)) => Ok((text, Vec::new())),
        None => Err("waiting for the schema"),
    }
}

/// The widget's summary, for screen readers and tests: `Arm: ArmState;
/// mode = kIntake (3); tip.x = 1.5`, each value with its full path.
fn summary(title: &str, kind: &str, lines: &[Row]) -> String {
    let mut path: Vec<&str> = Vec::new();
    let mut parts = vec![format!("{title}: {kind}")];
    for row in lines {
        path.truncate(row.depth);
        if row.node {
            path.push(&row.name);
            continue;
        }
        let mut full = path.join(".");
        if !full.is_empty() && !row.name.starts_with('[') {
            full.push('.');
        }
        full.push_str(&row.name);
        parts.push(format!("{} = {}", full.replace(".[", "["), row.text));
    }
    parts.join("; ")
}

impl StructView {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let (kind, lines) = match decoded(widget, live) {
            Ok(found) => found,
            Err(why) => {
                card::draw_message(ui.painter(), ui.visuals(), rect, why);
                let text = format!("{}: {why}", widget.title);
                ui.interact(rect, ui.scope_id().with("struct"), Sense::hover())
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(egui::accesskit::Role::List, true, &text)
                    });
                return;
            }
        };
        let size = props::font_size_set(&widget.properties).unwrap_or(DEFAULT_TEXT_SIZE);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let weak = child.visuals().weak_text_color();
        egui::ScrollArea::vertical()
            .id_salt(child.scope_id().with("struct rows"))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for row in &lines {
                    ui.horizontal(|ui| {
                        ui.add_space(row.depth as f32 * size * INDENT);
                        let name = RichText::new(&row.name).size(size).color(weak);
                        ui.add(egui::Label::new(name).selectable(false));
                        let value = RichText::new(&row.text).size(size).monospace();
                        let value = if row.node {
                            value.color(weak).italics()
                        } else {
                            value
                        };
                        ui.add(egui::Label::new(value).selectable(false).truncate());
                    });
                }
            });
        let text = summary(&widget.title, &kind, &lines);
        ui.interact(rect, ui.scope_id().with("struct"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::List, true, &text));
    }
}

impl WidgetType for StructView {
    fn names(&self) -> &'static [&'static str] {
        &["Struct"]
    }

    /// Any struct or struct array. It's last in the list of widget types,
    /// so a struct another widget is made for makes that one.
    fn accepts(&self, nt_type: &str) -> bool {
        structs::struct_type(nt_type).is_some()
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

    /// The struct's kind, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        let ty = props::text(p, props::DATA_TYPE, "");
        let kind = ty.strip_prefix("struct:").unwrap_or(ty);
        card::draw_message(painter, visuals, rect, kind);
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::text_size_row(ui, p);
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
    fn nested_structs_and_arrays_flatten_into_indented_lines() {
        let shown = vec![
            ("mode".to_string(), Shown::Leaf("kIntake (3)".into())),
            (
                "tip".to_string(),
                Shown::Node {
                    summary: "Translation2d".into(),
                    children: vec![
                        ("x".into(), Shown::Leaf("1.5".into())),
                        ("y".into(), Shown::Leaf("-2".into())),
                    ],
                },
            ),
            (
                "rollers".to_string(),
                Shown::Node {
                    summary: "[2]".into(),
                    children: vec![
                        ("[0]".into(), Shown::Leaf("0.5".into())),
                        ("[1]".into(), Shown::Leaf("0.25".into())),
                    ],
                },
            ),
        ];
        let mut lines = Vec::new();
        rows(&shown, 0, &mut lines);
        let depths: Vec<usize> = lines.iter().map(|r| r.depth).collect();
        assert_eq!(depths, [0, 0, 1, 1, 0, 1, 1]);
        assert!(lines[1].node && lines[1].text == "Translation2d");
        assert_eq!(
            summary("Arm", "ArmState", &lines),
            "Arm: ArmState; mode = kIntake (3); tip.x = 1.5; tip.y = -2; \
             rollers[0] = 0.5; rollers[1] = 0.25"
        );
    }

    #[test]
    fn any_struct_but_no_plain_value_is_taken() {
        assert!(StructView.accepts("struct:ArmState"));
        assert!(StructView.accepts("struct:ArmState[]"));
        assert!(!StructView.accepts("double") && !StructView.accepts("structschema"));
    }
}
