//! Drawing the tree: folders, topic rows, struct fields and their hover
//! text, and dragging rows out.

use super::format::shorten;
use super::*;

/// A tree row that can be selected, dragged onto the grid (when
/// `draggable`) and added from its right-click menu. `contents` draws the
/// row; `hover` is its tooltip. Returns the row's response.
fn selectable_row(
    ui: &mut egui::Ui,
    sel: &mut TreeSelection,
    row: DraggedTopic,
    draggable: bool,
    hover: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    // A slot under the row for its selection shading, filled in once the
    // row's size is known.
    let shading = ui.painter().add(egui::Shape::Noop);
    let drawn = ui.scope(|ui| {
        // Plain labels, so the pointer shows the grab hand rather than the
        // text cursor.
        ui.style_mut().interaction.selectable_labels = false;
        contents(ui);
    });
    let rect = drawn.response.rect;
    let field = row.field.as_ref().map(|f| f.path.clone());
    let id = egui::Id::unique(("tree_row", &row.name, field));
    let sense = if draggable {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    let response = ui.interact(rect, id, sense);
    if sel.reveal_hit(&row) {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if sel.is_selected(&row) {
        ui.painter().set(
            shading,
            egui::Shape::rect_filled(
                rect.expand(1.0),
                egui::CornerRadius::same(2),
                ui.visuals().selection.bg_fill,
            ),
        );
    }
    if response.clicked() {
        let held = ui.input(|i| i.modifiers);
        sel.click(&row, held.command, held.shift);
        sel.clicked = true;
    }
    // Right-clicking a row that isn't selected selects just it first.
    if response.secondary_clicked() && !sel.is_selected(&row) {
        sel.click(&row, false, false);
        sel.clicked = true;
    }
    if draggable {
        if response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), DraggedTopics(sel.group(&row)));
        }
        // Until let go, or called off with Escape.
        if response.dragged() && egui::DragAndDrop::has_payload_of_type::<DraggedTopics>(ui.ctx()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            drag_preview(ui.ctx(), id, &sel.group(&row));
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        response.context_menu(|ui| {
            let group = sel.group(&row);
            let label = match group.len() {
                1 => "Add to grid".to_owned(),
                n => format!("Add {n} to grid"),
            };
            if ui.button(label).clicked() {
                sel.add = group.clone();
            }
            // Any widget type that can show all of them (a Pose2d as a
            // Field or a 3D Pose, a number as a Graph), the default first.
            let choices: Vec<&'static str> = group.first().map_or_else(Vec::new, |first| {
                widgets::types_showing(&first.ty)
                    .into_iter()
                    .filter(|name| {
                        group
                            .iter()
                            .all(|t| widgets::types_showing(&t.ty).contains(name))
                    })
                    .collect()
            });
            if choices.len() > 1 {
                ui.menu_button("Add as", |ui| {
                    for name in choices {
                        if ui.button(name).clicked() {
                            sel.add = group.clone();
                            sel.add_as = Some(name);
                        }
                    }
                });
            }
            let lists = sel.lists.clone();
            ui.menu_button("Add to list", |ui| {
                if ui
                    .button("New list")
                    .on_hover_text("A new List Layout holding these")
                    .clicked()
                {
                    sel.add = group.clone();
                    sel.add_to = AddTo::NewList;
                }
                if !lists.is_empty() {
                    ui.separator();
                }
                for (i, name) in &lists {
                    if ui.button(name).clicked() {
                        sel.add = group.clone();
                        sel.add_to = AddTo::List(*i);
                    }
                }
            });
            let groups = sel.groups.clone();
            ui.menu_button("Add to group", |ui| {
                if ui
                    .button("New group")
                    .on_hover_text("A new Group holding these")
                    .clicked()
                {
                    sel.add = group.clone();
                    sel.add_to = AddTo::NewGroup;
                }
                if !groups.is_empty() {
                    ui.separator();
                }
                for (i, name) in &groups {
                    if ui.button(name).clicked() {
                        sel.add = group.clone();
                        sel.add_to = AddTo::Group(*i);
                    }
                }
            });
        });
    }
    let response = response.on_hover_text(hover);
    sel.drew(&row);
    response
}

/// A small label following the pointer while rows are dragged: the topic's
/// name, or how many there are.
fn drag_preview(ctx: &egui::Context, id: egui::Id, rows: &[DraggedTopic]) {
    let Some(pointer) = ctx.pointer_interact_pos() else {
        return;
    };
    let text = match rows {
        [one] => match &one.field {
            Some(field) => format!("{} → {}", one.name, field.path.join(".")),
            None => one.name.clone(),
        },
        many => format!("{} topics", many.len()),
    };
    egui::Area::new(id.with("drag_preview"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(12.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
        });
}

impl Folder {
    /// Draw this folder's contents. `path` is the folder's own path (`""` for
    /// the root); `expand_all` opens every folder, used while filtering.
    /// `structs` holds the struct schemas the server has sent, used to open struct topics
    /// such as `Pose2d` into their fields.
    ///
    /// Rows for topics (and struct fields) that have a widget type can be
    /// selected in `sel`; when `draggable` is set (the layout isn't
    /// locked) they can also be dragged onto the grid or added from their
    /// right-click menu. See [`DraggedTopic`] and
    /// [`widgets::type_for_topic`].
    ///
    /// Returns the full names of the topics that were drawn, that is, those
    /// in open folders, so their values can be requested from the server.
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        path: &str,
        expand_all: bool,
        structs: &StructDatabase,
        draggable: bool,
        sel: &mut TreeSelection,
    ) -> Vec<String> {
        let mut visible = Vec::new();
        for (name, folder) in &self.folders {
            let folder_path = format!("{path}/{name}");
            // A table with a widget of its own (a chooser) is one row that
            // drags as a whole, with an arrow of its own that opens it to
            // show its parts (options, active, ...; with metadata shown,
            // `.type` and the rest too).
            if let Some(ty) = &folder.table_type
                && widgets::type_for_table(ty).is_some()
            {
                let row = DraggedTopic {
                    name: folder_path.clone(),
                    ty: ty.clone(),
                    field: None,
                };
                let hover = if draggable {
                    format!("{folder_path} ({ty})\n{DRAG_HINT}")
                } else {
                    format!("{folder_path} ({ty})")
                };
                let active = folder.topics.get("active").and_then(|t| t.value.as_ref());
                // Only the arrow opens it, so the row itself can be dragged.
                let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id(("table", &folder_path)),
                    expand_all,
                );
                state
                    .show_header(ui, |ui| {
                        selectable_row(ui, sel, row, draggable, &hover, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(name);
                                if let Some(active) = active {
                                    ui.monospace(format_value(active));
                                }
                                ui.weak(ty);
                            });
                        })
                    })
                    .body(|ui| {
                        for (key, topic) in &folder.topics {
                            plain_row(ui, key, topic);
                        }
                    });
                // Its values, for the row (and the widget made from it).
                visible.extend(folder.topics.values().map(|t| t.name.clone()));
                continue;
            }
            let label = format!("{name}  ({})", folder.topic_count());
            let open = egui::CollapsingHeader::new(label)
                .id_salt(&folder_path)
                .default_open(false)
                .open((expand_all || sel.reveals_in_folder(&folder_path)).then_some(true))
                .show(ui, |ui| {
                    folder.show(ui, &folder_path, expand_all, structs, draggable, sel)
                });
            visible.extend(open.body_returned.unwrap_or_default());
        }
        for (key, topic) in &self.topics {
            let has_widget = widgets::type_for_topic(&topic.ty).is_some();
            // A struct with a widget of its own (a Pose2d's Field) still
            // opens into its fields; `show_topic` makes its row draggable.
            if has_widget && structs::struct_type(&topic.ty).is_none() {
                let row = DraggedTopic {
                    name: topic.name.clone(),
                    ty: topic.ty.clone(),
                    field: None,
                };
                let hover = if draggable {
                    format!("{}\n{DRAG_HINT}", topic.name)
                } else {
                    topic.name.clone()
                };
                selectable_row(ui, sel, row, draggable, &hover, |ui| {
                    plain_row(ui, key, topic);
                });
            } else {
                // Rows that open (structs, schemas) must not be drag sources:
                // egui gives a drag area on top of a header the clicks.
                let note = if has_widget || structs::struct_type(&topic.ty).is_some() {
                    String::new()
                } else {
                    format!("No widget for {} yet", topic.ty)
                };
                show_topic(ui, key, topic, structs, &note, draggable, sel);
            }
            visible.push(topic.name.clone());
        }
        visible
    }
}

/// The hover hint on rows that can become widgets.
const DRAG_HINT: &str = "Drag onto the grid, or right-click to add. Ctrl+click or Shift+click \
                         to select several.";

/// A topic's name, value and type on one line.
fn plain_row(ui: &mut egui::Ui, key: &str, topic: &TopicEntry) {
    let value = match &topic.value {
        Some(v) => format_value(v),
        None => "…".into(),
    };
    ui.horizontal(|ui| {
        ui.label(key);
        ui.monospace(value);
        ui.weak(&topic.ty);
    });
}

/// One topic row that isn't a widget source itself: struct values that can
/// be decoded open into their fields (which can be selected, and dragged
/// onto the grid when `draggable` is set), schemas open into their parts,
/// and anything else is a plain row. `note`, if not empty, is added to the
/// hover text.
fn show_topic(
    ui: &mut egui::Ui,
    key: &str,
    topic: &TopicEntry,
    structs: &StructDatabase,
    note: &str,
    draggable: bool,
    sel: &mut TreeSelection,
) {
    let bytes = match &topic.value {
        Some(Value::Raw(bytes)) => Some(bytes.as_slice()),
        _ => None,
    };
    let schema = structs::struct_type(&topic.ty)
        .and_then(|(name, _)| structs.find(name))
        .map(|d| d.schema().to_owned());
    let mut hover = match &schema {
        Some(schema) => format!("{}\nschema: {schema}", topic.name),
        None => topic.name.clone(),
    };
    if !note.is_empty() {
        hover = format!("{hover}\n{note}");
    }
    if let Some(Shown::Node { summary, children }) =
        bytes.and_then(|b| structs::decode(structs, &topic.ty, b))
    {
        // Fields can become widgets, including fields of an item in a
        // struct array (raptaboard only; Elastic can't bind those).
        let source = match (structs::struct_type(&topic.ty), bytes) {
            (Some((schema, _)), Some(bytes)) => Some(FieldSource {
                topic: &topic.name,
                schema,
                bytes,
                structs,
            }),
            _ => None,
        };
        let fields = structs::struct_type(&topic.ty).map(|(schema, _)| Fields {
            topic: &topic.name,
            schema,
            structs,
        });
        if widgets::type_for_topic(&topic.ty).is_some() {
            // The whole struct is a widget source (a Pose2d makes a Field):
            // a row that drags as a whole, with an arrow to open it. A
            // click on the row opens it too, as a header's would.
            let id = ui.make_persistent_id(("struct", &topic.name));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                false,
            );
            if sel.reveals_in_struct(&topic.name, &[]) {
                state.set_open(true);
            }
            let row = DraggedTopic {
                name: topic.name.clone(),
                ty: topic.ty.clone(),
                field: None,
            };
            let hover = if draggable {
                format!("{hover}\n{DRAG_HINT}")
            } else {
                hover
            };
            let mut clicked = false;
            state
                .show_header(ui, |ui| {
                    clicked = selectable_row(ui, sel, row, draggable, &hover, |ui| {
                        ui.label(format!("{key}  {summary}"));
                    })
                    .clicked();
                })
                .body(|ui| {
                    show_children(
                        ui,
                        &topic.name,
                        &children,
                        fields.as_ref(),
                        source.as_ref(),
                        &[],
                        Rows { sel, draggable },
                    )
                });
            if clicked
                && let Some(mut state) =
                    egui::collapsing_header::CollapsingState::load(ui.ctx(), id)
            {
                state.toggle(ui);
                state.store(ui.ctx());
            }
            return;
        }
        egui::CollapsingHeader::new(format!("{key}  {summary}"))
            .id_salt(&topic.name)
            .default_open(false)
            .open(sel.reveals_in_struct(&topic.name, &[]).then_some(true))
            .show(ui, |ui| {
                show_children(
                    ui,
                    &topic.name,
                    &children,
                    fields.as_ref(),
                    source.as_ref(),
                    &[],
                    Rows { sel, draggable },
                )
            })
            .header_response
            .on_hover_text(hover);
        return;
    }
    // A schema topic opens into its fields, enum lists and nested structs.
    if topic.ty == "structschema"
        && let Some(text) = bytes.map(String::from_utf8_lossy)
        && let Ok(Shown::Node { summary, children }) = structs::describe_schema(structs, &text)
    {
        egui::CollapsingHeader::new(format!("{key}  schema, {summary}"))
            .id_salt(&topic.name)
            .default_open(false)
            .show(ui, |ui| {
                show_children(
                    ui,
                    &topic.name,
                    &children,
                    None,
                    None,
                    &[],
                    Rows { sel, draggable },
                )
            })
            .header_response
            .on_hover_text(format!("{}\n{text}", topic.name));
        return;
    }
    let value = match (&topic.value, bytes) {
        // Schema text that didn't parse is shown as it is, such as "double x;double y".
        (_, Some(b)) if topic.ty == "structschema" => {
            format!("\"{}\"", shorten(&String::from_utf8_lossy(b)))
        }
        (Some(v), _) => format_value(v),
        (None, _) => "…".into(),
    };
    ui.horizontal(|ui| {
        ui.label(key);
        ui.monospace(value);
        ui.weak(&topic.ty);
    })
    .response
    .on_hover_text(hover);
}

/// What field rows need for selecting and dragging.
struct Rows<'s> {
    sel: &'s mut TreeSelection,
    draggable: bool,
}

/// A struct topic whose field rows can become widgets.
struct FieldSource<'a> {
    topic: &'a str,
    /// The struct's schema name, such as `Pose2d`.
    schema: &'a str,
    bytes: &'a [u8],
    structs: &'a StructDatabase,
}

/// A decoded struct topic, for looking up its fields' types.
struct Fields<'a> {
    topic: &'a str,
    /// The struct's schema name, such as `Pose2d`.
    schema: &'a str,
    structs: &'a StructDatabase,
}

impl Fields<'_> {
    /// The field at `path`, if the schema has it.
    fn info(&self, path: &[String]) -> Option<FieldInfo> {
        structs::field_info(self.structs, self.schema, path)
    }

    /// Hover text for the field at `path`: where it is, its type, and its
    /// enum names, such as
    /// `/Arm/state → mode` / `enum uint8 field of Arm` / `kStow = 0, kIntake = 3`.
    fn hover(&self, path: &[String], info: Option<&FieldInfo>) -> String {
        let mut text = format!("{} → {}", self.topic, structs::path_text(path));
        if let Some(info) = info {
            text.push_str(&format!("\n{} field of {}", info.label(), self.schema));
            if info.is_enum() {
                text.push_str(&format!("\n{}", info.enum_list()));
            }
        }
        text
    }
}

/// The fields of a decoded struct; nested structs and arrays open in turn.
/// With `fields`, each field shows its type and has hover details. With
/// `source`, plain fields (not array items) can be selected, and dragged
/// onto the grid or added when `rows.draggable`. `path` is the field names
/// leading here.
fn show_children(
    ui: &mut egui::Ui,
    id: &str,
    children: &[(String, Shown)],
    fields: Option<&Fields<'_>>,
    source: Option<&FieldSource<'_>>,
    path: &[String],
    rows: Rows<'_>,
) {
    let Rows { sel, draggable } = rows;
    // Used for decoded values and for schemas; nested parts start open.
    for (name, shown) in children {
        let field_path: Vec<String> = path.iter().cloned().chain([name.clone()]).collect();
        let info = fields.and_then(|f| f.info(&field_path));
        let hover = fields.map(|f| f.hover(&field_path, info.as_ref()));
        match shown {
            Shown::Leaf(text) => {
                let row = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.monospace(text);
                        if let Some(info) = &info {
                            ui.weak(info.label());
                        }
                    })
                };
                let payload = source.and_then(|src| {
                    let value =
                        structs::field_value(src.structs, src.schema, src.bytes, &field_path)?;
                    let ty = structs::nt_type_name(&value);
                    widgets::type_for_topic(ty)?;
                    Some(DraggedTopic {
                        name: src.topic.to_owned(),
                        ty: ty.to_owned(),
                        field: Some(StructMeta {
                            path: field_path.clone(),
                            schema_name: src.schema.to_owned(),
                        }),
                    })
                });
                match payload {
                    Some(payload) => {
                        let mut hover = hover.unwrap_or_default();
                        if draggable {
                            hover = format!("{hover}\n\n{DRAG_HINT}");
                        }
                        selectable_row(ui, sel, payload, draggable, &hover, |ui| {
                            row(ui);
                        });
                    }
                    None => {
                        let row = row(ui).response;
                        if let Some(hover) = hover {
                            row.on_hover_text(hover);
                        }
                    }
                }
            }
            Shown::Node { summary, children } => {
                let child_id = format!("{id}/{name}");
                // The type after the value, greyed like a leaf row's.
                let mut title = egui::text::LayoutJob::default();
                let style = ui.style();
                egui::RichText::new(format!("{name}  {summary}")).append_to(
                    &mut title,
                    style,
                    egui::FontSelection::Default,
                    egui::Align::Center,
                );
                if let Some(info) = &info {
                    egui::RichText::new(format!("  {}", info.label()))
                        .weak()
                        .append_to(
                            &mut title,
                            style,
                            egui::FontSelection::Default,
                            egui::Align::Center,
                        );
                }
                let open = source.is_some_and(|src| sel.reveals_in_struct(src.topic, &field_path));
                // A whole struct inside (a Pose2d's translation, an item of a
                // struct array) is a widget source too: a row that drags,
                // with an arrow to open it, as a struct topic's row is.
                let payload = source.and_then(|src| {
                    let (ty, _) =
                        structs::nested_struct(src.structs, src.schema, src.bytes, &field_path)?;
                    widgets::type_for_topic(&ty)?;
                    Some(DraggedTopic {
                        name: src.topic.to_owned(),
                        ty,
                        field: Some(StructMeta {
                            path: field_path.clone(),
                            schema_name: src.schema.to_owned(),
                        }),
                    })
                });
                if let Some(payload) = payload {
                    let state_id = ui.make_persistent_id(("struct part", &child_id));
                    let mut state =
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            state_id,
                            true,
                        );
                    if open {
                        state.set_open(true);
                    }
                    let mut hover = hover.unwrap_or_default();
                    if draggable {
                        hover = format!("{hover}\n\n{DRAG_HINT}");
                    }
                    let mut clicked = false;
                    state
                        .show_header(ui, |ui| {
                            clicked = selectable_row(ui, sel, payload, draggable, &hover, |ui| {
                                ui.label(title);
                            })
                            .clicked();
                        })
                        .body(|ui| {
                            show_children(
                                ui,
                                &child_id,
                                children,
                                fields,
                                source,
                                &field_path,
                                Rows {
                                    sel: &mut *sel,
                                    draggable,
                                },
                            )
                        });
                    if clicked
                        && let Some(mut state) =
                            egui::collapsing_header::CollapsingState::load(ui.ctx(), state_id)
                    {
                        state.toggle(ui);
                        state.store(ui.ctx());
                    }
                    continue;
                }
                let header = egui::CollapsingHeader::new(title)
                    .id_salt(&child_id)
                    .default_open(true)
                    .open(open.then_some(true))
                    .show(ui, |ui| {
                        show_children(
                            ui,
                            &child_id,
                            children,
                            fields,
                            source,
                            &field_path,
                            Rows {
                                sel: &mut *sel,
                                draggable,
                            },
                        )
                    })
                    .header_response;
                if let Some(hover) = hover {
                    header.on_hover_text(hover);
                }
            }
        }
    }
}
