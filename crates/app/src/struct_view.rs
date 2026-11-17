//! Turn WPILib struct values (`struct:Pose2d`, `struct:SwerveModuleState[]`)
//! into labelled fields the tree can show.
//!
//! A struct topic's value is packed bytes. The layout comes from a schema
//! the robot publishes on `/.schema/struct:<Name>`; [`StructDatabase`] holds
//! the schemas and [`DynamicStruct`] reads a value with one. This module only
//! decides how the result is labelled, so it can be tested without a robot.

use nt4::proto::structs::{DynamicStruct, FieldValue, StructDatabase};

use crate::tree::format_number;

/// A decoded value, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// A single value as text, such as `1.5`, `true` or `kForward (1)`.
    Leaf(String),
    /// A struct or array: a short label and its named parts.
    Node {
        /// What it is, such as `Pose2d` or `SwerveModuleState[4]`.
        summary: String,
        /// Field names (or `[0]`, `[1]`, ... for arrays) and their values.
        children: Vec<(String, Shown)>,
    },
}

/// The struct name in a topic type: `struct:Pose2d[]` gives
/// `Some(("Pose2d", true))`, `double` gives `None`.
///
/// ```
/// use raptaboard::struct_view::struct_type;
/// assert_eq!(struct_type("struct:Pose2d"), Some(("Pose2d", false)));
/// assert_eq!(struct_type("struct:SwerveModuleState[]"), Some(("SwerveModuleState", true)));
/// assert_eq!(struct_type("double"), None);
/// ```
pub fn struct_type(ty: &str) -> Option<(&str, bool)> {
    let rest = ty.strip_prefix("struct:")?;
    Some(match rest.strip_suffix("[]") {
        Some(name) => (name, true),
        None => (rest, false),
    })
}

/// Decode the bytes of a topic of type `ty`. `None` if it isn't a struct
/// type, its schema hasn't arrived yet, or the bytes don't fit the schema.
///
/// ```
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::struct_view::{decode, Shown};
/// let mut db = StructDatabase::new();
/// db.add("Rotation2d", "double value").unwrap();
/// let bytes = 0.5f64.to_le_bytes();
/// let Some(Shown::Node { summary, children }) = decode(&db, "struct:Rotation2d", &bytes) else {
///     panic!()
/// };
/// assert_eq!(summary, "Rotation2d");
/// assert_eq!(children, vec![("value".to_string(), Shown::Leaf("0.5".into()))]);
/// ```
pub fn decode(db: &StructDatabase, ty: &str, bytes: &[u8]) -> Option<Shown> {
    let (name, array) = struct_type(ty)?;
    if array {
        let items = DynamicStruct::array(db, name, bytes)?;
        Some(Shown::Node {
            summary: format!("{name}[{}]", items.len()),
            children: items
                .iter()
                .enumerate()
                .map(|(i, s)| (format!("[{i}]"), show_struct(s)))
                .collect(),
        })
    } else {
        Some(show_struct(&DynamicStruct::new(db, name, bytes)?))
    }
}

fn show_struct(s: &DynamicStruct<'_>) -> Shown {
    let desc = s.descriptor();
    let children =
        desc.fields()
            .iter()
            .filter_map(|field| {
                let value = s.value(field)?;
                let shown = match value {
                    // Enum fields show the name the schema gives the number.
                    FieldValue::Int(n) if !field.enum_values().is_empty() => {
                        Shown::Leaf(enum_label(field.enum_name(n), n))
                    }
                    FieldValue::Uint(n) if !field.enum_values().is_empty() => Shown::Leaf(
                        enum_label(i64::try_from(n).ok().and_then(|n| field.enum_name(n)), n),
                    ),
                    other => show_field(other),
                };
                Some((field.name().to_owned(), shown))
            })
            .collect();
    Shown::Node {
        summary: desc.name().to_owned(),
        children,
    }
}

fn enum_label(name: Option<&str>, n: impl std::fmt::Display) -> String {
    match name {
        Some(name) => format!("{name} ({n})"),
        None => n.to_string(),
    }
}

fn show_field(value: FieldValue<'_>) -> Shown {
    match value {
        FieldValue::Bool(b) => Shown::Leaf(b.to_string()),
        FieldValue::Int(n) => Shown::Leaf(n.to_string()),
        FieldValue::Uint(n) => Shown::Leaf(n.to_string()),
        FieldValue::Float(f) => Shown::Leaf(format_number(f64::from(f))),
        FieldValue::Double(d) => Shown::Leaf(format_number(d)),
        FieldValue::String(s) => Shown::Leaf(format!("\"{s}\"")),
        FieldValue::Struct(s) => show_struct(&s),
        FieldValue::Array(items) => Shown::Node {
            summary: format!("[{}]", items.len()),
            children: items
                .into_iter()
                .enumerate()
                .map(|(i, v)| (format!("[{i}]"), show_field(v)))
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn doubles(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn wpilib_db() -> StructDatabase {
        let mut db = StructDatabase::new();
        // The schemas robot code publishes, in the order the sim sent them.
        db.add("SwerveModuleState", "double speed;Rotation2d angle")
            .unwrap();
        db.add("Rotation2d", "double value").unwrap();
        db.add("Pose2d", "Translation2d translation;Rotation2d rotation")
            .unwrap();
        db.add("Translation2d", "double x;double y").unwrap();
        db
    }

    fn leaf(s: &str) -> Shown {
        Shown::Leaf(s.into())
    }

    fn node(summary: &str, children: Vec<(&str, Shown)>) -> Shown {
        Shown::Node {
            summary: summary.into(),
            children: children
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn pose2d_shows_nested_fields() {
        let db = wpilib_db();
        let shown = decode(&db, "struct:Pose2d", &doubles(&[1.5, -2.0, 0.25])).unwrap();
        assert_eq!(
            shown,
            node(
                "Pose2d",
                vec![
                    (
                        "translation",
                        node("Translation2d", vec![("x", leaf("1.5")), ("y", leaf("-2"))])
                    ),
                    (
                        "rotation",
                        node("Rotation2d", vec![("value", leaf("0.25"))])
                    ),
                ]
            )
        );
    }

    #[test]
    fn struct_arrays_number_their_items() {
        let db = wpilib_db();
        let bytes = doubles(&[1.0, 0.1, 2.0, 0.2]);
        let Some(Shown::Node { summary, children }) =
            decode(&db, "struct:SwerveModuleState[]", &bytes)
        else {
            panic!("expected a node")
        };
        assert_eq!(summary, "SwerveModuleState[2]");
        assert_eq!(children.len(), 2);
        assert_eq!(children[1].0, "[1]");
        assert_eq!(
            children[1].1,
            node(
                "SwerveModuleState",
                vec![
                    ("speed", leaf("2")),
                    ("angle", node("Rotation2d", vec![("value", leaf("0.2"))])),
                ]
            )
        );
    }

    #[test]
    fn enums_arrays_and_text() {
        let mut db = StructDatabase::new();
        db.add(
            "Mixed",
            "enum {kOff=0, kOn=1} int8 mode;int32 counts[3];char label[4];bool ok",
        )
        .unwrap();
        let mut bytes = vec![1u8];
        for n in [7i32, 8, 9] {
            bytes.extend(n.to_le_bytes());
        }
        bytes.extend(b"arm\0");
        bytes.push(1);
        assert_eq!(
            decode(&db, "struct:Mixed", &bytes).unwrap(),
            node(
                "Mixed",
                vec![
                    ("mode", leaf("kOn (1)")),
                    (
                        "counts",
                        node(
                            "[3]",
                            vec![("[0]", leaf("7")), ("[1]", leaf("8")), ("[2]", leaf("9"))]
                        )
                    ),
                    ("label", leaf("\"arm\"")),
                    ("ok", leaf("true")),
                ]
            )
        );
    }

    #[test]
    fn missing_schema_or_short_data_is_none() {
        let db = wpilib_db();
        assert_eq!(decode(&db, "struct:Unknown", &[0; 8]), None);
        assert_eq!(decode(&db, "struct:Pose2d", &[0; 8]), None);
        assert_eq!(decode(&db, "struct:SwerveModuleState[]", &[0; 17]), None);
        assert_eq!(decode(&db, "double", &[0; 8]), None);
    }
}
