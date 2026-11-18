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
/// `true` writes each child's label over its value instead of beside it,
/// to save space (raptaboard; Elastic ignores it). Left out when off.
pub const LABEL_INSIDE: &str = "label_inside";

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
/// Space around a label drawn inside a child's value, in points.
const INSIDE_LABEL_PAD: f32 = 2.0;
/// How solid the backing behind an inside label is, from 0 (clear) to 1.
const INSIDE_LABEL_BACKING: f32 = 0.8;

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

/// How a list labels its children: `label_position` and `label_inside`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Labels {
    /// Where each label goes.
    pub position: LabelPosition,
    /// Over the value, rather than beside it.
    pub inside: bool,
}

/// The `label_position` and `label_inside` settings together.
///
/// ```
/// use raptaboard::widgets::list_layout::{labels, LabelPosition};
/// let props = serde_json::json!({"label_position": "TOP", "label_inside": true});
/// let l = labels(props.as_object().unwrap());
/// assert_eq!((l.position, l.inside), (LabelPosition::Top, true));
/// ```
pub fn labels(props: &Map<String, Json>) -> Labels {
    Labels {
        position: label_position(props),
        inside: props
            .get(LABEL_INSIDE)
            .and_then(Json::as_bool)
            .unwrap_or(false),
    }
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
    /// Where its title is written; `None` when labels are hidden. With
    /// labels inside, this is part of `content`, and the label is drawn
    /// over the value.
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
/// labels at the side (they take at most half the row). Labels inside take
/// no room of their own: the value gets the whole row. Rows past the
/// bottom of `body` are still returned; drawing cuts them off.
pub fn rows(
    body: Rect,
    children: &[WidgetContainer],
    labels: Labels,
    label_width: impl Fn(usize) -> f32,
) -> Vec<ChildRow> {
    let Labels { position, inside } = labels;
    let mut top = body.min.y;
    children
        .iter()
        .enumerate()
        .filter(|(_, child)| !is_hidden(child))
        .map(|(i, child)| {
            let content_height = content_height(child);
            let height = match position {
                LabelPosition::Top | LabelPosition::Bottom if !inside => {
                    LABEL_HEIGHT + content_height
                }
                _ => content_height,
            };
            let row = Rect::from_min_size(
                egui::pos2(body.min.x, top),
                egui::vec2(body.width(), height),
            );
            top += height + CHILD_GAP;
            let side = (label_width(i)).min(row.width() * MAX_SIDE_LABEL);
            if inside {
                let label = match position {
                    LabelPosition::Top => {
                        Some(row.with_max_y(row.min.y + LABEL_HEIGHT.min(row.height())))
                    }
                    LabelPosition::Bottom => {
                        Some(row.with_min_y(row.max.y - LABEL_HEIGHT.min(row.height())))
                    }
                    LabelPosition::Left => Some(row.with_max_x(row.min.x + side)),
                    LabelPosition::Right => Some(row.with_min_x(row.max.x - side)),
                    LabelPosition::Hidden => None,
                };
                return ChildRow {
                    index: i,
                    row,
                    content: row,
                    label,
                };
            }
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

/// The area children are laid out in, for a list whose card is `card`:
/// below the title, less the card's margin. Matches [`draw`].
pub fn body_rect(card: Rect) -> Rect {
    card.with_min_y(card.min.y + card::TITLE_HEIGHT)
        .shrink(card::BODY_MARGIN)
}

/// Each shown child's row for a list whose card is `card`, for finding
/// what's under the pointer. (Side labels' widths don't matter for that.)
pub fn rows_on_screen(card: Rect, list: &LayoutContainer) -> Vec<ChildRow> {
    rows(
        body_rect(card),
        &list.children,
        labels(&list.properties),
        |_| 0.0,
    )
}

/// Where a child dropped at height `y` goes among a list's children: before
/// the first shown child whose row's middle is below `y`, or last.
/// `children` is how many children the list has, hidden ones included.
///
/// ```
/// use eframe::egui::{pos2, vec2, Rect};
/// use raptaboard::widgets::list_layout::{insert_index, ChildRow};
/// let row = |index, top: f32| {
///     let r = Rect::from_min_size(pos2(0.0, top), vec2(100.0, 50.0));
///     ChildRow { index, row: r, content: r, label: None }
/// };
/// let rows = [row(0, 0.0), row(2, 60.0)]; // child 1 is hidden
/// assert_eq!(insert_index(&rows, 3, 10.0), 0);
/// assert_eq!(insert_index(&rows, 3, 40.0), 2);
/// assert_eq!(insert_index(&rows, 3, 200.0), 3);
/// ```
pub fn insert_index(rows: &[ChildRow], children: usize, y: f32) -> usize {
    rows.iter()
        .find(|r| r.row.center().y > y)
        .map_or(children, |r| r.index)
}

/// The height to draw the "drops here" line at, for inserting at `at`:
/// halfway into the gap above the first shown row at or after `at`, below
/// the last row, or at the top of `body` for an empty list.
pub fn insert_line_y(rows: &[ChildRow], at: usize, body: Rect) -> f32 {
    match rows.iter().find(|r| r.index >= at) {
        Some(r) => r.row.min.y - CHILD_GAP / 2.0,
        None => rows
            .last()
            .map_or(body.min.y, |r| r.row.max.y + CHILD_GAP / 2.0),
    }
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
    let labels = labels(&list.properties);
    let font = egui::FontId::proportional(LABEL_FONT_SIZE);
    card::draw_clipped(painter, body, |painter, body| {
        let width = |i: usize| {
            let title = list.children[i].title.clone();
            painter
                .layout_no_wrap(title, font.clone(), visuals.text_color())
                .size()
                .x
        };
        for place in rows(body, &list.children, labels, width) {
            let child = &list.children[place.index];
            if let Some(label) = place.label
                && !labels.inside
            {
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
            // Inside: over the value, on a backing so it reads over anything.
            if let Some(label) = place.label
                && labels.inside
            {
                draw_inside_label(
                    painter,
                    visuals,
                    label,
                    labels.position,
                    &child.title,
                    &font,
                );
            }
        }
    });
}

/// A label drawn over a child's value: the title on a slightly see-through
/// backing in the card's color, at the left of `area` (or the right, for
/// labels on the right).
fn draw_inside_label(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    area: Rect,
    position: LabelPosition,
    title: &str,
    font: &egui::FontId,
) {
    let painter = painter.with_clip_rect(area.intersect(painter.clip_rect()));
    let galley = painter.layout_no_wrap(title.to_owned(), font.clone(), visuals.text_color());
    let size = galley.size();
    let x = match position {
        LabelPosition::Right => area.max.x - size.x - INSIDE_LABEL_PAD,
        _ => area.min.x + INSIDE_LABEL_PAD,
    };
    let y = match position {
        LabelPosition::Top => area.min.y + INSIDE_LABEL_PAD,
        LabelPosition::Bottom => area.max.y - size.y - INSIDE_LABEL_PAD,
        _ => area.center().y - size.y / 2.0,
    };
    let text_rect = Rect::from_min_size(egui::pos2(x, y), size);
    painter.rect_filled(
        text_rect.expand(INSIDE_LABEL_PAD),
        egui::CornerRadius::same(card::BODY_CORNER_RADIUS / 2),
        visuals
            .extreme_bg_color
            .gamma_multiply(INSIDE_LABEL_BACKING),
    );
    painter.galley(text_rect.min, galley, visuals.text_color());
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
/// and buttons to move it up or down, edit it, or delete it. Edit… is
/// returned as [`ListAction::EditChild`] for the window to carry out.
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
            let mut inside = labels(&list.properties).inside;
            let checkbox = ui
                .add_enabled(
                    position != LabelPosition::Hidden,
                    egui::Checkbox::new(&mut inside, "Inside"),
                )
                .on_hover_text("Write each label over its value, to save space")
                .on_disabled_hover_text("Labels are hidden");
            if checkbox.changed() {
                set_property(
                    &mut list.properties,
                    LABEL_INSIDE,
                    inside.then_some(true.into()),
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
                    if ui
                        .button("Delete")
                        .on_hover_text(
                            "Delete it. Cancel or undo brings it back; to keep it but not \
                             show it, untick Shown.",
                        )
                        .clicked()
                    {
                        change = Some((i, Change::Delete));
                    }
                });
                ui.end_row();
            }
        });
    match change {
        Some((i, Change::Move(to))) => move_child(list, i, to),
        Some((i, Change::Delete)) => {
            list.children.remove(i);
        }
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
    Delete,
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
        let top = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Top,
                inside: false,
            },
            |_| 30.0,
        );
        assert_eq!(top[0].label.unwrap().height(), LABEL_HEIGHT);
        assert_eq!(top[0].content.height(), 80.0);
        assert_eq!(top[0].content.min.y, LABEL_HEIGHT);
        // The next row starts after the first and the gap.
        assert_eq!(top[1].row.min.y, LABEL_HEIGHT + 80.0 + CHILD_GAP);
        assert_eq!(top[1].content.height(), MIN_CHILD_CONTENT);

        // Below: value first, then the label.
        let bottom = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Bottom,
                inside: false,
            },
            |_| 30.0,
        );
        assert_eq!(bottom[0].content.min.y, 0.0);
        assert_eq!(bottom[0].label.unwrap().min.y, 80.0);

        // Hidden: the value takes the whole row, and rows are shorter.
        let hidden = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Hidden,
                inside: false,
            },
            |_| 30.0,
        );
        assert_eq!(hidden[0].label, None);
        assert_eq!(hidden[0].content, hidden[0].row);
        assert_eq!(hidden[1].row.min.y, 80.0 + CHILD_GAP);
    }

    #[test]
    fn side_labels_take_their_width_up_to_half() {
        let children = [child("a", 128.0)];
        let left = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Left,
                inside: false,
            },
            |_| 30.0,
        );
        assert_eq!(left[0].label.unwrap().width(), 30.0);
        assert_eq!(left[0].content.min.x, 30.0 + SIDE_LABEL_GAP);
        assert_eq!(left[0].content.height(), 80.0);
        // A long title is cut to half the row.
        let right = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Right,
                inside: false,
            },
            |_| 500.0,
        );
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
        for expected in [
            "Labels", "Top", "Children", "speed", "angle", "Edit…", "Delete",
        ] {
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
        let placed = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Hidden,
                inside: false,
            },
            |_| 0.0,
        );
        // "b" is skipped, and "c" moves up into its place.
        assert_eq!(placed.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(placed[1].row.min.y, 80.0 + CHILD_GAP);
    }

    #[test]
    fn drop_lines_sit_between_rows() {
        let children = [child("a", 128.0), child("b", 128.0)];
        let placed = rows(
            body(),
            &children,
            Labels {
                position: LabelPosition::Hidden,
                inside: false,
            },
            |_| 0.0,
        );
        // Rows are 80 tall with a 6 gap: 0..80 and 86..166.
        assert_eq!(insert_line_y(&placed, 0, body()), -CHILD_GAP / 2.0);
        assert_eq!(insert_line_y(&placed, 1, body()), 83.0);
        assert_eq!(insert_line_y(&placed, 2, body()), 166.0 + CHILD_GAP / 2.0);
        assert_eq!(insert_line_y(&[], 0, body()), 0.0);
        // The drop index follows the rows' middles.
        assert_eq!(insert_index(&placed, 2, 39.0), 0);
        assert_eq!(insert_index(&placed, 2, 41.0), 1);
        assert_eq!(insert_index(&placed, 2, 127.0), 2);
        // The body sits below the card's title, inside its margin.
        let card = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(256.0, 512.0));
        let inside = body_rect(card);
        assert_eq!(inside.min.y, card::TITLE_HEIGHT + card::BODY_MARGIN);
        assert_eq!(inside.width(), 256.0 - 2.0 * card::BODY_MARGIN);
    }

    #[test]
    fn inside_labels_take_no_room() {
        let children = [child("a", 128.0), child("b", 128.0)];
        let inside = |position| {
            rows(
                body(),
                &children,
                Labels {
                    position,
                    inside: true,
                },
                |_| 30.0,
            )
        };
        // Top: rows are just the value's height, and the label sits at the
        // top of the value.
        let top = inside(LabelPosition::Top);
        assert_eq!(top[0].content, top[0].row);
        assert_eq!(top[0].row.height(), 80.0);
        assert_eq!(top[1].row.min.y, 80.0 + CHILD_GAP);
        let label = top[0].label.unwrap();
        assert_eq!((label.min.y, label.height()), (0.0, LABEL_HEIGHT));
        assert!(top[0].content.contains_rect(label));
        // Bottom and the sides put it at that edge of the value.
        let bottom = inside(LabelPosition::Bottom)[0].label.unwrap();
        assert_eq!(bottom.max.y, 80.0);
        let left = inside(LabelPosition::Left)[0].label.unwrap();
        assert_eq!((left.min.x, left.width()), (0.0, 30.0));
        let right = inside(LabelPosition::Right)[0].label.unwrap();
        assert_eq!(right.max.x, 200.0);
        assert_eq!(inside(LabelPosition::Hidden)[0].label, None);
    }

    #[test]
    fn the_inside_box_is_saved_only_when_ticked() {
        let off = list(vec![], Some("TOP"));
        assert!(!labels(&off.properties).inside);
        let mut on = off.clone();
        on.properties.insert(LABEL_INSIDE.into(), true.into());
        assert!(labels(&on.properties).inside);
        // Drawing the settings adds nothing.
        let mut drawn = off.clone();
        let text = draw_settings(&mut drawn);
        assert_eq!(drawn, off);
        assert!(text.iter().any(|t| t == "Inside"), "{text:?}");
    }
}
