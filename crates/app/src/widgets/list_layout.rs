//! List Layout: a card that stacks widgets top to bottom, as Elastic does.
//!
//! Children are drawn compactly, without a card each: a label with the
//! child's title, placed as the list's `label_position` says, and the
//! child's body ([`WidgetType::draw_body`](super::WidgetType::draw_body)). Each child's row is as tall
//! as Elastic makes it: the child's saved height less the card's title and
//! padding, and at least [`MIN_CHILD_CONTENT`].
//!
//! A List Layout isn't a [`WidgetType`](super::WidgetType): it holds widgets rather than
//! showing a topic, and is saved in a tab's `layouts`, not `containers`.

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::{LayoutContainer, WidgetContainer};
use serde_json::{Map, Value as Json};

use super::find;
use super::shared::card;
use super::shared::props::set_property;

/// The `type` Elastic saves List Layouts as.
pub const TYPE_NAME: &str = "List Layout";
/// Where each child's title goes (Elastic): `"TOP"`, `"LEFT"`, `"RIGHT"`,
/// `"BOTTOM"` or `"HIDDEN"`.
pub const LABEL_POSITION: &str = "label_position";
/// On a child: `true` keeps it in the list without showing it (raptaboard;
/// Elastic ignores it and shows the child). Left out when shown.
pub const HIDDEN: &str = "hidden";

// Sizes, in screen points.

/// Space between children, in points.
pub const CHILD_GAP: f32 = 6.0;
/// What Elastic takes off a child's saved height in a list: the title
/// strip and padding its card would have, in points.
const CHILD_CHROME: f32 = 48.0;
/// The least height a child's value gets, in points (Elastic's).
pub const MIN_CHILD_CONTENT: f32 = 32.0;
/// Height of a label above or below a child's value, in points.
pub const LABEL_HEIGHT: f32 = 18.0;
/// Size of the label text, in points.
const LABEL_FONT_SIZE: f32 = 13.0;
/// Space between a side label and the child's value, in points.
const SIDE_LABEL_GAP: f32 = 5.0;
/// The most of a row's width a side label may take, from 0 to 1.
const MAX_SIDE_LABEL: f32 = 0.5;

/// Where a List Layout puts each child's title.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LabelPosition {
    /// Above the value (Elastic's default).
    #[default]
    Top,
    /// Left of the value.
    Left,
    /// Right of the value.
    Right,
    /// Below the value.
    Bottom,
    /// No label.
    Hidden,
}

impl LabelPosition {
    /// All choices, in the order a menu lists them.
    pub const ALL: [LabelPosition; 5] = [
        LabelPosition::Top,
        LabelPosition::Left,
        LabelPosition::Right,
        LabelPosition::Bottom,
        LabelPosition::Hidden,
    ];

    /// The word Elastic saves: `"TOP"`.
    pub fn key(self) -> &'static str {
        match self {
            LabelPosition::Top => "TOP",
            LabelPosition::Left => "LEFT",
            LabelPosition::Right => "RIGHT",
            LabelPosition::Bottom => "BOTTOM",
            LabelPosition::Hidden => "HIDDEN",
        }
    }

    /// The choice saved as `key`, in any case (`"Top"` or `"TOP"`).
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.key().eq_ignore_ascii_case(key))
    }

    /// Words for a menu.
    pub fn label(self) -> &'static str {
        match self {
            LabelPosition::Top => "Top",
            LabelPosition::Left => "Left",
            LabelPosition::Right => "Right",
            LabelPosition::Bottom => "Bottom",
            LabelPosition::Hidden => "Hidden",
        }
    }
}

/// The `label_position` setting; [`LabelPosition::Top`] if not set or not
/// one Elastic knows.
///
/// ```
/// use raptaboard::widgets::list_layout::{label_position, LabelPosition};
/// let props = serde_json::json!({"label_position": "LEFT"});
/// assert_eq!(label_position(props.as_object().unwrap()), LabelPosition::Left);
/// assert_eq!(label_position(&serde_json::Map::new()), LabelPosition::Top);
/// ```
pub fn label_position(props: &Map<String, Json>) -> LabelPosition {
    props
        .get(LABEL_POSITION)
        .and_then(Json::as_str)
        .and_then(LabelPosition::from_key)
        .unwrap_or_default()
}

/// Whether `child` is kept in its list without being shown (`hidden`).
pub fn is_hidden(child: &WidgetContainer) -> bool {
    child
        .properties
        .get(HIDDEN)
        .and_then(Json::as_bool)
        .unwrap_or(false)
}

/// Where one child goes in a list's body, in screen points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildRow {
    /// Which child this is, by its index in the list's `children`.
    pub index: usize,
    /// The whole row: label and value.
    pub row: Rect,
    /// Where the child's value is drawn.
    pub content: Rect,
    /// Where its title is written; `None` when labels are hidden.
    pub label: Option<Rect>,
}

/// How tall a child's value is in a list, in points: its saved height less
/// the card title and padding it doesn't have here, and at least
/// [`MIN_CHILD_CONTENT`]. A one-cell (128) widget gets 80.
pub fn content_height(child: &WidgetContainer) -> f32 {
    (child.rect.height as f32 - CHILD_CHROME).max(MIN_CHILD_CONTENT)
}

/// Lay the shown children out top to bottom in `body`; hidden ones take no
/// space. `label_width` gives a child's title width (by its index), for
/// labels at the side (they take at most half the row). Rows past the
/// bottom of `body` are still returned; drawing cuts them off.
pub fn rows(
    body: Rect,
    children: &[WidgetContainer],
    position: LabelPosition,
    label_width: impl Fn(usize) -> f32,
) -> Vec<ChildRow> {
    let mut top = body.min.y;
    children
        .iter()
        .enumerate()
        .filter(|(_, child)| !is_hidden(child))
        .map(|(i, child)| {
            let content_height = content_height(child);
            let height = match position {
                LabelPosition::Top | LabelPosition::Bottom => LABEL_HEIGHT + content_height,
                _ => content_height,
            };
            let row = Rect::from_min_size(
                egui::pos2(body.min.x, top),
                egui::vec2(body.width(), height),
            );
            top += height + CHILD_GAP;
            let side = (label_width(i)).min(row.width() * MAX_SIDE_LABEL);
            let (content, label) = match position {
                LabelPosition::Top => {
                    let (label, content) = row.split_top_bottom_at_y(row.min.y + LABEL_HEIGHT);
                    (content, Some(label))
                }
                LabelPosition::Bottom => {
                    let (content, label) = row.split_top_bottom_at_y(row.max.y - LABEL_HEIGHT);
                    (content, Some(label))
                }
                LabelPosition::Left => {
                    let (label, rest) = row.split_left_right_at_x(row.min.x + side);
                    (rest.with_min_x(rest.min.x + SIDE_LABEL_GAP), Some(label))
                }
                LabelPosition::Right => {
                    let (rest, label) = row.split_left_right_at_x(row.max.x - side);
                    (rest.with_max_x(rest.max.x - SIDE_LABEL_GAP), Some(label))
                }
                LabelPosition::Hidden => (row, None),
            };
            ChildRow {
                index: i,
                row,
                content,
                label,
            }
        })
        .collect()
}

/// Draw a List Layout in `rect`: its card and title, then each child's
/// label and value. `value_of` gives a child's value.
pub fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    list: &LayoutContainer,
    value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
) {
    let body = card::draw_card(painter, visuals, rect, &list.title);
    let position = label_position(&list.properties);
    let font = egui::FontId::proportional(LABEL_FONT_SIZE);
    card::draw_clipped(painter, body, |painter, body| {
        let width = |i: usize| {
            let title = list.children[i].title.clone();
            painter
                .layout_no_wrap(title, font.clone(), visuals.text_color())
                .size()
                .x
        };
        for place in rows(body, &list.children, position, width) {
            let child = &list.children[place.index];
            if let Some(label) = place.label {
                painter
                    .with_clip_rect(label.intersect(painter.clip_rect()))
                    .text(
                        label.left_center(),
                        egui::Align2::LEFT_CENTER,
                        &child.title,
                        font.clone(),
                        visuals.text_color(),
                    );
            }
            match find(&child.widget_type) {
                Some(ty) => ty.draw_body(
                    painter,
                    visuals,
                    place.content,
                    &child.properties,
                    value_of(child).as_ref(),
                ),
                None => card::draw_message(painter, visuals, place.content, &child.widget_type),
            }
        }
    });
}

/// Move the child at `from` to `to` in `list`, shifting the ones between.
/// Does nothing if either is out of range.
///
/// ```
/// use raptaboard::widgets::list_layout::move_child;
/// # let child = |t: &str| serde_json::from_value(serde_json::json!({"title": t,
/// #     "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"})).unwrap();
/// # let mut list: raptaboard_layout::LayoutContainer = serde_json::from_value(serde_json::json!({
/// #     "title": "L", "x": 0.0, "y": 0.0, "width": 256.0, "height": 256.0,
/// #     "type": "List Layout"})).unwrap();
/// # list.children = vec![child("a"), child("b"), child("c")];
/// move_child(&mut list, 2, 0);
/// let titles: Vec<_> = list.children.iter().map(|c| c.title.as_str()).collect();
/// assert_eq!(titles, ["c", "a", "b"]);
/// ```
pub fn move_child(list: &mut LayoutContainer, from: usize, to: usize) {
    if from < list.children.len() && to < list.children.len() {
        let child = list.children.remove(from);
        list.children.insert(to, child);
    }
}

/// What the list settings dialog asks the window to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListAction {
    /// Open the settings of the child at this index.
    EditChild(usize),
}

/// Draw the settings for `list` and apply changes at once: its title, the
/// label position, and its children in order, each with a Shown checkbox
/// and buttons to move it up or down or edit it. Edit… is returned as
/// [`ListAction::EditChild`] for the window to carry out. Children can't be
/// deleted here: until widgets can be dragged into a list, a deleted child
/// couldn't be put back, so hiding one is the way to take it out of view.
pub fn settings(ui: &mut egui::Ui, list: &mut LayoutContainer) -> Option<ListAction> {
    let mut action = None;
    egui::Grid::new("list_settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut list.title).desired_width(180.0));
            ui.end_row();

            ui.label("Labels");
            let mut position = label_position(&list.properties);
            egui::ComboBox::from_id_salt("label_position")
                .selected_text(position.label())
                .show_ui(ui, |ui| {
                    for choice in LabelPosition::ALL {
                        ui.selectable_value(&mut position, choice, choice.label());
                    }
                });
            if position != label_position(&list.properties) {
                // Elastic always saves it, so it's always written.
                set_property(
                    &mut list.properties,
                    LABEL_POSITION,
                    Some(position.key().into()),
                );
            }
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.strong("Children");
    if list.children.is_empty() {
        ui.weak("None yet.");
        return action;
    }
    let count = list.children.len();
    let mut change = None;
    egui::Grid::new("list_children")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (i, child) in list.children.iter().enumerate() {
                let mut shown = !is_hidden(child);
                if ui
                    .checkbox(&mut shown, "")
                    .on_hover_text("Shown in the list. Unticked, it stays in the list, hidden.")
                    .changed()
                {
                    change = Some((i, Change::Show(shown)));
                }
                if shown {
                    ui.label(&child.title);
                } else {
                    ui.weak(&child.title);
                }
                ui.weak(&child.widget_type);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("↑"))
                        .on_hover_text("Move up")
                        .clicked()
                    {
                        change = Some((i, Change::Move(i - 1)));
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new("↓"))
                        .on_hover_text("Move down")
                        .clicked()
                    {
                        change = Some((i, Change::Move(i + 1)));
                    }
                    if ui.button("Edit…").clicked() {
                        action = Some(ListAction::EditChild(i));
                    }
                });
                ui.end_row();
            }
        });
    match change {
        Some((i, Change::Move(to))) => move_child(list, i, to),
        Some((i, Change::Show(shown))) => {
            set_property(
                &mut list.children[i].properties,
                HIDDEN,
                (!shown).then_some(true.into()),
            );
        }
        None => {}
    }
    action
}

/// A change to the children list, made after drawing it.
enum Change {
    Move(usize),
    Show(bool),
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn child(title: &str, height: f64) -> WidgetContainer {
        serde_json::from_value(serde_json::json!({
            "title": title, "x": 0.0, "y": 0.0, "width": 256.0, "height": height,
            "type": "Text Display"}))
        .unwrap()
    }

    fn list(children: Vec<WidgetContainer>, position: Option<&str>) -> LayoutContainer {
        let mut list: LayoutContainer = serde_json::from_value(serde_json::json!({
            "title": "List", "x": 0.0, "y": 0.0, "width": 256.0, "height": 512.0,
            "type": TYPE_NAME}))
        .unwrap();
        list.children = children;
        if let Some(p) = position {
            list.properties.insert(LABEL_POSITION.into(), p.into());
        }
        list
    }

    fn body() -> Rect {
        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 1000.0))
    }

    #[test]
    fn label_positions_read_in_any_case() {
        for (saved, position) in [
            ("TOP", LabelPosition::Top),
            ("Left", LabelPosition::Left),
            ("right", LabelPosition::Right),
            ("BOTTOM", LabelPosition::Bottom),
            ("HIDDEN", LabelPosition::Hidden),
            ("SIDEWAYS", LabelPosition::Top),
        ] {
            let l = list(vec![], Some(saved));
            assert_eq!(label_position(&l.properties), position, "{saved}");
        }
    }

    #[test]
    fn rows_stack_with_elastics_heights() {
        // A one-cell child gets 128 - 48 = 80 for its value; a tiny one
        // still gets 32.
        let children = [child("a", 128.0), child("b", 10.0)];
        let top = rows(body(), &children, LabelPosition::Top, |_| 30.0);
        assert_eq!(top[0].label.unwrap().height(), LABEL_HEIGHT);
        assert_eq!(top[0].content.height(), 80.0);
        assert_eq!(top[0].content.min.y, LABEL_HEIGHT);
        // The next row starts after the first and the gap.
        assert_eq!(top[1].row.min.y, LABEL_HEIGHT + 80.0 + CHILD_GAP);
        assert_eq!(top[1].content.height(), MIN_CHILD_CONTENT);

        // Below: value first, then the label.
        let bottom = rows(body(), &children, LabelPosition::Bottom, |_| 30.0);
        assert_eq!(bottom[0].content.min.y, 0.0);
        assert_eq!(bottom[0].label.unwrap().min.y, 80.0);

        // Hidden: the value takes the whole row, and rows are shorter.
        let hidden = rows(body(), &children, LabelPosition::Hidden, |_| 30.0);
        assert_eq!(hidden[0].label, None);
        assert_eq!(hidden[0].content, hidden[0].row);
        assert_eq!(hidden[1].row.min.y, 80.0 + CHILD_GAP);
    }

    #[test]
    fn side_labels_take_their_width_up_to_half() {
        let children = [child("a", 128.0)];
        let left = rows(body(), &children, LabelPosition::Left, |_| 30.0);
        assert_eq!(left[0].label.unwrap().width(), 30.0);
        assert_eq!(left[0].content.min.x, 30.0 + SIDE_LABEL_GAP);
        assert_eq!(left[0].content.height(), 80.0);
        // A long title is cut to half the row.
        let right = rows(body(), &children, LabelPosition::Right, |_| 500.0);
        assert_eq!(right[0].label.unwrap().width(), 100.0);
        assert_eq!(right[0].content.max.x, 100.0 - SIDE_LABEL_GAP);
    }

    #[test]
    fn children_move_and_stay_in_range() {
        let mut l = list(
            vec![child("a", 128.0), child("b", 128.0), child("c", 128.0)],
            None,
        );
        let titles = |l: &LayoutContainer| -> Vec<String> {
            l.children.iter().map(|c| c.title.clone()).collect()
        };
        move_child(&mut l, 0, 2);
        assert_eq!(titles(&l), ["b", "c", "a"]);
        move_child(&mut l, 1, 0);
        assert_eq!(titles(&l), ["c", "b", "a"]);
        move_child(&mut l, 0, 9);
        assert_eq!(titles(&l), ["c", "b", "a"]);
    }

    /// Draw the list settings once in a window-less egui frame.
    fn draw_settings(l: &mut LayoutContainer) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            settings(ui, l);
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|s| match s.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn settings_list_the_children_and_change_nothing_by_themselves() {
        let mut l = list(
            vec![child("speed", 128.0), child("angle", 128.0)],
            Some("TOP"),
        );
        let before = l.clone();
        let text = draw_settings(&mut l);
        assert_eq!(l, before);
        assert!(!text.iter().any(|t| t == "Delete"), "{text:?}");
        for expected in ["Labels", "Top", "Children", "speed", "angle", "Edit…"] {
            assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
        }
        // An empty list says so.
        let mut empty = list(vec![], None);
        assert!(draw_settings(&mut empty).iter().any(|t| t == "None yet."));
    }

    #[test]
    fn hidden_children_take_no_space() {
        let mut children = vec![child("a", 128.0), child("b", 128.0), child("c", 128.0)];
        children[1].properties.insert(HIDDEN.into(), true.into());
        assert!(is_hidden(&children[1]));
        assert!(!is_hidden(&children[0]));
        let placed = rows(body(), &children, LabelPosition::Hidden, |_| 0.0);
        // "b" is skipped, and "c" moves up into its place.
        assert_eq!(placed.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(placed[1].row.min.y, 80.0 + CHILD_GAP);
    }
}
