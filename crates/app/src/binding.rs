//! What a widget is bound to on the server: the value it shows now, and
//! where a value it sends goes. No drawing here; the grid asks these and
//! draws.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use nt4::proto::structs::StructDatabase;
use nt4::{NtType, Value};
use raptaboard_layout::WidgetContainer;

use crate::nt::WriteError;
use crate::sampling::Sampler;
use crate::types::structs;
use crate::widgets;
use crate::widgets::shared::{format, props};

/// The values widgets show this frame: each widget's topic, taken once per
/// its `period` (see [`Sampler`]), and for a widget bound to one field of a
/// struct, that field (an enum by name or number, as its settings say).
pub struct WidgetValues<'a> {
    sampler: &'a RefCell<Sampler>,
    values: &'a HashMap<&'a str, &'a Value>,
    structs: Option<&'a StructDatabase>,
    now: Instant,
}

impl<'a> WidgetValues<'a> {
    /// Values from the server's newest `values` (by full topic name), with
    /// its struct schemas `structs`, at time `now`.
    pub fn new(
        sampler: &'a RefCell<Sampler>,
        values: &'a HashMap<&'a str, &'a Value>,
        structs: Option<&'a StructDatabase>,
        now: Instant,
    ) -> Self {
        WidgetValues {
            sampler,
            values,
            structs,
            now,
        }
    }

    /// The value `widget` shows now, if there is one.
    pub fn of(&self, widget: &WidgetContainer) -> Option<Value> {
        let topic = widget.topic()?;
        let latest = self.values.get(topic).copied();
        let value =
            self.sampler
                .borrow_mut()
                .sample(topic, props::period_s(widget), latest, self.now)?;
        match (widget.struct_meta(), &value) {
            (None, _) => Some(value),
            (Some(meta), Value::Raw(bytes)) => {
                let (value, name) = structs::field_value_named(
                    self.structs?,
                    &meta.schema_name,
                    bytes,
                    &meta.path,
                )?;
                Some(format::enum_shown(
                    value,
                    name.as_deref(),
                    format::enum_display(&widget.properties),
                    format::int_style(&widget.properties),
                ))
            }
            (Some(_), _) => None,
        }
    }
}

/// Where a value a widget sends goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTarget {
    /// The widget's topic, written in the type the server announced.
    Topic(String),
    /// A topic under the widget's table (a chooser's `selected`), which the
    /// dashboard publishes as this type if nobody has yet.
    Publish(String, NtType),
}

/// Where the values `widget` sends go, or why they can't be sent: it has
/// no topic, or it shows one field of a struct (sending would rewrite the
/// robot's whole struct).
///
/// ```
/// use raptaboard::binding::{write_target, WriteTarget};
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let toggle = for_topic("/Arm/enabled", "boolean", None, at).unwrap();
/// assert_eq!(write_target(&toggle), Ok(WriteTarget::Topic("/Arm/enabled".into())));
/// ```
pub fn write_target(widget: &WidgetContainer) -> Result<WriteTarget, WriteError> {
    let Some(topic) = widget.topic() else {
        return Err(WriteError::NotPublished("(no topic)".to_owned()));
    };
    if widget.struct_meta().is_some() {
        return Err(WriteError::Struct(topic.to_owned()));
    }
    let under = widgets::find(&widget.widget_type).and_then(|ty| ty.sends_to(&widget.properties));
    Ok(match under {
        Some((key, ty)) => {
            WriteTarget::Publish(format!("{}/{key}", topic.trim_end_matches('/')), ty)
        }
        None => WriteTarget::Topic(topic.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use raptaboard_layout::{Rect, StructMeta};

    fn widget(ty: &str, topic: Option<&str>) -> WidgetContainer {
        let mut properties = serde_json::Map::new();
        if let Some(topic) = topic {
            properties.insert("topic".into(), topic.into());
        }
        WidgetContainer {
            title: "w".into(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 128.0,
                height: 128.0,
            },
            widget_type: ty.into(),
            properties,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn sends_go_to_the_topic_or_the_table_below_it() {
        assert_eq!(
            write_target(&widget("Toggle Switch", Some("/Arm/on"))),
            Ok(WriteTarget::Topic("/Arm/on".into()))
        );
        // A chooser's pick goes to `selected` under its table, published
        // as a string if nobody has.
        assert_eq!(
            write_target(&widget("Dropdown Chooser", Some("/SmartDashboard/Auto/"))),
            Ok(WriteTarget::Publish(
                "/SmartDashboard/Auto/selected".into(),
                NtType::String
            ))
        );
        // No topic, or a struct field: can't be sent, and says why.
        assert_eq!(
            write_target(&widget("Toggle Switch", None)),
            Err(WriteError::NotPublished("(no topic)".into()))
        );
        let mut field = widget("Text Display", Some("/Drive/pose"));
        let meta = StructMeta {
            path: vec!["x".into()],
            schema_name: "Pose2d".into(),
        };
        field
            .properties
            .insert("struct_meta".into(), meta.to_json("double"));
        assert_eq!(
            write_target(&field),
            Err(WriteError::Struct("/Drive/pose".into()))
        );
    }

    #[test]
    fn a_struct_field_shows_its_value_by_its_settings() {
        let mut db = StructDatabase::new();
        db.add("Arm", "enum {kStow=0, kIntake=3} uint8 mode")
            .unwrap();
        let mut w = widget("Text Display", Some("/Arm"));
        let meta = StructMeta {
            path: vec!["mode".into()],
            schema_name: "Arm".into(),
        };
        w.properties
            .insert("struct_meta".into(), meta.to_json("uint8"));
        let raw = Value::Raw(vec![3]);
        let values = HashMap::from([("/Arm", &raw)]);
        let sampler = RefCell::new(Sampler::default());
        let now = Instant::now();
        let bound = WidgetValues::new(&sampler, &values, Some(&db), now);
        assert_eq!(bound.of(&w), Some(Value::String("kIntake".into())));
        // Without the schema yet, nothing to show.
        let sampler = RefCell::new(Sampler::default());
        let bound = WidgetValues::new(&sampler, &values, None, now);
        assert_eq!(bound.of(&w), None);
    }
}
