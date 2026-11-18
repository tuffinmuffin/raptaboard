//! Turn WPILib struct values (`struct:Pose2d`, `struct:SwerveModuleState[]`)
//! into labelled fields the tree can show.
//!
//! A struct topic's value is packed bytes. The layout comes from a schema
//! a publisher sends on `/.schema/struct:<Name>`; [`StructDatabase`] holds
//! the schemas and [`DynamicStruct`] reads a value with one. This module only
//! decides how the result is labelled, so it can be tested without a server.

use nt4::Value;
use nt4::proto::structs::{
    Declaration, DynamicStruct, FieldDescriptor, FieldValue, StructDatabase, parse_schema,
};

use crate::tree::format_number;
use crate::widgets::IntStyle;

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

/// One field of a struct value, as a plain NT value: `path` names the field
/// and any structs it's inside, such as `["translation", "x"]` in a
/// `Pose2d`. `schema` is the struct's name (`Pose2d`). `None` if the schema
/// hasn't arrived, the path doesn't name a field, or the field is a whole
/// struct.
///
/// This is how a widget bound to a struct field (Elastic's `struct_meta`)
/// gets its value.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::struct_view::field_value;
/// let mut db = StructDatabase::new();
/// db.add("Translation2d", "double x;double y").unwrap();
/// let bytes: Vec<u8> = [1.5f64, -2.0].iter().flat_map(|d| d.to_le_bytes()).collect();
/// let x = field_value(&db, "Translation2d", &bytes, &["x".to_string()]);
/// assert_eq!(x, Some(Value::Double(1.5)));
/// ```
pub fn field_value(
    db: &StructDatabase,
    schema: &str,
    bytes: &[u8],
    path: &[String],
) -> Option<Value> {
    let (s, field) = find_field(db, schema, bytes, path)?;
    as_nt_value(s.value(field)?)
}

/// How a widget shows an enum field's value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnumDisplay {
    /// The name from the schema: `kIntake`.
    #[default]
    Name,
    /// The number: `3`.
    Number,
    /// Both: `kIntake (3)`.
    Both,
}

impl EnumDisplay {
    /// All choices, in the order a menu lists them.
    pub const ALL: [EnumDisplay; 3] = [EnumDisplay::Name, EnumDisplay::Number, EnumDisplay::Both];

    /// The word saved in a layout's `enum_display` setting.
    pub fn key(self) -> &'static str {
        match self {
            EnumDisplay::Name => "name",
            EnumDisplay::Number => "number",
            EnumDisplay::Both => "both",
        }
    }

    /// The choice saved as `key`, if it's one of [`EnumDisplay::key`]'s words.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }

    /// Words for a menu.
    pub fn label(self) -> &'static str {
        match self {
            EnumDisplay::Name => "Name (kIntake)",
            EnumDisplay::Number => "Number (3)",
            EnumDisplay::Both => "Both (kIntake (3))",
        }
    }
}

/// Like [`field_value`], but an enum field is shown as `display` says: by
/// its name from the schema (`kIntake`, as a string), its number, or both
/// (`kIntake (3)`, with the number written as `ints` says). Numbers the
/// enum doesn't name stay numbers.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::struct_view::{field_value, field_value_shown, EnumDisplay};
/// use raptaboard::widgets::{IntFormat, IntStyle};
/// let mut db = StructDatabase::new();
/// db.add("Arm", "enum {kStow=0, kIntake=3} uint8 mode").unwrap();
/// let mode = vec!["mode".to_string()];
/// let shown = |bytes: &[u8], how| {
///     field_value_shown(&db, "Arm", bytes, &mode, how, IntStyle::default())
/// };
/// assert_eq!(field_value(&db, "Arm", &[3], &mode), Some(Value::Int(3)));
/// assert_eq!(shown(&[3], EnumDisplay::Name), Some(Value::String("kIntake".into())));
/// assert_eq!(shown(&[3], EnumDisplay::Both), Some(Value::String("kIntake (3)".into())));
/// assert_eq!(shown(&[3], EnumDisplay::Number), Some(Value::Int(3)));
/// assert_eq!(shown(&[9], EnumDisplay::Name), Some(Value::Int(9)));
/// let hex = IntStyle { format: IntFormat::Hex, digits: 2 };
/// let both = field_value_shown(&db, "Arm", &[3], &mode, EnumDisplay::Both, hex);
/// assert_eq!(both, Some(Value::String("kIntake (0x03)".into())));
/// ```
pub fn field_value_shown(
    db: &StructDatabase,
    schema: &str,
    bytes: &[u8],
    path: &[String],
    display: EnumDisplay,
    ints: IntStyle,
) -> Option<Value> {
    let (s, field) = find_field(db, schema, bytes, path)?;
    let value = as_nt_value(s.value(field)?)?;
    if field.enum_values().is_empty() || display == EnumDisplay::Number {
        return Some(value);
    }
    Some(match value {
        Value::Int(n) => match field.enum_name(n) {
            Some(name) if display == EnumDisplay::Both => {
                Value::String(format!("{name} ({})", ints.write(n)))
            }
            Some(name) => Value::String(name.to_owned()),
            None => Value::Int(n),
        },
        other => other,
    })
}

/// Whether the field `path` names in the struct `schema` is an enum. Needs
/// only the schemas, not a value.
///
/// ```
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::struct_view::is_enum_field;
/// let mut db = StructDatabase::new();
/// db.add("Arm", "enum {kStow=0, kIntake=3} uint8 mode; double angle").unwrap();
/// assert!(is_enum_field(&db, "Arm", &["mode".to_string()]));
/// assert!(!is_enum_field(&db, "Arm", &["angle".to_string()]));
/// ```
pub fn is_enum_field(db: &StructDatabase, schema: &str, path: &[String]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let mut s = db.find(schema);
    for name in parents {
        s = s.and_then(|s| s.field(name)).and_then(|f| db.nested(f));
    }
    s.and_then(|s| s.field(last))
        .is_some_and(|f| !f.enum_values().is_empty())
}

/// The struct holding the field `path` names, and that field.
fn find_field<'a>(
    db: &'a StructDatabase,
    schema: &str,
    bytes: &'a [u8],
    path: &[String],
) -> Option<(DynamicStruct<'a>, &'a FieldDescriptor)> {
    let (last, parents) = path.split_last()?;
    let mut s = DynamicStruct::new(db, schema, bytes)?;
    for name in parents {
        match s.value(s.field(name)?)? {
            FieldValue::Struct(inner) => s = inner,
            _ => return None,
        }
    }
    let field = s.field(last)?;
    Some((s, field))
}

/// A struct field's value as an NT value. Whole structs and mixed arrays
/// have no plain NT form.
fn as_nt_value(v: FieldValue<'_>) -> Option<Value> {
    Some(match v {
        FieldValue::Bool(b) => Value::Boolean(b),
        FieldValue::Int(n) => Value::Int(n),
        FieldValue::Uint(n) => Value::Int(i64::try_from(n).ok()?),
        FieldValue::Float(f) => Value::Float(f),
        FieldValue::Double(d) => Value::Double(d),
        FieldValue::String(s) => Value::String(s.into_owned()),
        FieldValue::Struct(_) => return None,
        FieldValue::Array(items) => {
            let items: Vec<Value> = items.into_iter().map(as_nt_value).collect::<Option<_>>()?;
            let all = |f: fn(&Value) -> bool| items.iter().all(f);
            if all(|v| matches!(v, Value::Boolean(_))) {
                Value::BooleanArray(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Boolean(b) => Some(*b),
                            _ => None,
                        })
                        .collect(),
                )
            } else if all(|v| matches!(v, Value::Int(_))) {
                Value::IntArray(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Int(n) => Some(*n),
                            _ => None,
                        })
                        .collect(),
                )
            } else if all(|v| matches!(v, Value::Float(_))) {
                Value::FloatArray(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Float(f) => Some(*f),
                            _ => None,
                        })
                        .collect(),
                )
            } else if all(|v| matches!(v, Value::Double(_))) {
                Value::DoubleArray(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Double(d) => Some(*d),
                            _ => None,
                        })
                        .collect(),
                )
            } else {
                return None;
            }
        }
    })
}

/// The NT type string for a value, as servers announce it: `double`,
/// `boolean[]` and so on. Used as a widget's `data_type`.
///
/// ```
/// use nt4::Value;
/// use raptaboard::struct_view::nt_type_name;
/// assert_eq!(nt_type_name(&Value::Double(1.0)), "double");
/// assert_eq!(nt_type_name(&Value::IntArray(vec![])), "int[]");
/// ```
pub fn nt_type_name(v: &Value) -> &'static str {
    match v {
        Value::Boolean(_) => "boolean",
        Value::Double(_) => "double",
        Value::Float(_) => "float",
        Value::Int(_) => "int",
        Value::String(_) => "string",
        Value::Raw(_) => "raw",
        Value::BooleanArray(_) => "boolean[]",
        Value::DoubleArray(_) => "double[]",
        Value::FloatArray(_) => "float[]",
        Value::IntArray(_) => "int[]",
        Value::StringArray(_) => "string[]",
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

/// How deep nested struct types are opened in a schema, to stop a schema
/// that names itself from going on forever.
const MAX_SCHEMA_DEPTH: usize = 8;

/// Describe a struct schema field by field, for reviewing it in the tree.
///
/// Each field shows its type (`double`, `uint8[4]`, `int32:3` for a bit
/// field). Enum fields open into their `name = value` list. Fields whose type
/// is another struct open into that struct's schema when `db` has it.
///
/// ```
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::struct_view::{describe_schema, Shown};
/// let db = StructDatabase::new();
/// let Shown::Node { summary, children } =
///     describe_schema(&db, "enum {kOff=0, kOn=1} uint8 mode;double x").unwrap()
/// else {
///     panic!()
/// };
/// assert_eq!(summary, "2 fields");
/// assert_eq!(children[1], ("x".to_string(), Shown::Leaf("double".into())));
/// ```
pub fn describe_schema(db: &StructDatabase, schema: &str) -> Result<Shown, String> {
    describe_at_depth(db, schema, 0)
}

fn describe_at_depth(db: &StructDatabase, schema: &str, depth: usize) -> Result<Shown, String> {
    let decls = parse_schema(schema)?;
    let summary = match decls.len() {
        1 => "1 field".to_owned(),
        n => format!("{n} fields"),
    };
    let children = decls
        .iter()
        .map(|d| (d.name.clone(), describe_field(db, d, depth)))
        .collect();
    Ok(Shown::Node { summary, children })
}

fn describe_field(db: &StructDatabase, d: &Declaration, depth: usize) -> Shown {
    let mut ty = d.type_name.clone();
    if d.array_size != 1 {
        ty.push_str(&format!("[{}]", d.array_size));
    }
    if d.bit_width != 0 {
        ty.push_str(&format!(":{}", d.bit_width));
    }
    if !d.enum_values.is_empty() {
        return Shown::Node {
            summary: format!("{ty} enum ({} values)", d.enum_values.len()),
            children: d
                .enum_values
                .iter()
                .map(|(name, value)| (name.clone(), Shown::Leaf(value.to_string())))
                .collect(),
        };
    }
    // A nested struct opens into its own schema, if the server has sent it.
    let nested = (depth < MAX_SCHEMA_DEPTH)
        .then(|| db.find(&d.type_name))
        .flatten()
        .and_then(|desc| describe_at_depth(db, desc.schema(), depth + 1).ok());
    match nested {
        Some(Shown::Node { children, .. }) => Shown::Node {
            summary: ty,
            children,
        },
        _ => Shown::Leaf(ty),
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

    fn path(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn struct_fields_read_by_path() {
        let db = wpilib_db();
        // Pose2d: translation (x, y), rotation (value).
        let pose = doubles(&[1.5, -2.0, 0.25]);
        let field = |p: &[&str]| field_value(&db, "Pose2d", &pose, &path(p));
        assert_eq!(field(&["translation", "x"]), Some(Value::Double(1.5)));
        assert_eq!(field(&["translation", "y"]), Some(Value::Double(-2.0)));
        assert_eq!(field(&["rotation", "value"]), Some(Value::Double(0.25)));
        // A whole struct, a missing field, a path through a number, no path.
        assert_eq!(field(&["translation"]), None);
        assert_eq!(field(&["heading"]), None);
        assert_eq!(field(&["translation", "x", "more"]), None);
        assert_eq!(field(&[]), None);
        // An unknown schema or too few bytes.
        assert_eq!(field_value(&db, "Pose3d", &pose, &path(&["x"])), None);
        assert_eq!(
            field_value(&db, "Pose2d", &pose[..8], &path(&["translation", "x"])),
            None
        );
    }

    #[test]
    fn struct_field_types_become_nt_types() {
        let mut db = StructDatabase::new();
        db.add("Mixed", "bool on;int16 n;float f;double d[2];char name[4]")
            .unwrap();
        let mut bytes = vec![1u8];
        bytes.extend((-3i16).to_le_bytes());
        bytes.extend(0.5f32.to_le_bytes());
        bytes.extend(doubles(&[1.0, 2.0]));
        bytes.extend(b"abc\0");
        let field = |name: &str| field_value(&db, "Mixed", &bytes, &path(&[name])).unwrap();
        assert_eq!(field("on"), Value::Boolean(true));
        assert_eq!(field("n"), Value::Int(-3));
        assert_eq!(field("f"), Value::Float(0.5));
        assert_eq!(field("d"), Value::DoubleArray(vec![1.0, 2.0]));
        assert_eq!(field("name"), Value::String("abc".into()));
        assert_eq!(nt_type_name(&field("on")), "boolean");
        assert_eq!(nt_type_name(&field("d")), "double[]");
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
    fn schema_lists_enum_values_and_opens_nested_structs() {
        let db = wpilib_db();
        assert_eq!(
            describe_schema(
                &db,
                "enum {kIdle=0, kFault=-1} int8 health;double t[2];uint8 flags:3;Rotation2d heading",
            )
            .unwrap(),
            node(
                "4 fields",
                vec![
                    (
                        "health",
                        node("int8 enum (2 values)", vec![("kIdle", leaf("0")), ("kFault", leaf("-1"))])
                    ),
                    ("t", leaf("double[2]")),
                    ("flags", leaf("uint8:3")),
                    ("heading", node("Rotation2d", vec![("value", leaf("double"))])),
                ]
            )
        );
        // A struct type whose schema hasn't arrived stays a plain type name.
        assert_eq!(
            describe_schema(&db, "Unknown thing").unwrap(),
            node("1 field", vec![("thing", leaf("Unknown"))])
        );
        assert!(describe_schema(&db, "int32 a[]").is_err());
    }

    #[test]
    fn missing_schema_or_short_data_is_none() {
        let db = wpilib_db();
        assert_eq!(decode(&db, "struct:Unknown", &[0; 8]), None);
        assert_eq!(decode(&db, "struct:Pose2d", &[0; 8]), None);
        assert_eq!(decode(&db, "struct:SwerveModuleState[]", &[0; 17]), None);
        assert_eq!(decode(&db, "double", &[0; 8]), None);
    }

    #[test]
    fn enum_fields_are_found_inside_nested_structs() {
        let mut db = StructDatabase::new();
        db.add("Arm", "enum {kStow=0, kIntake=3} uint8 mode; double angle")
            .unwrap();
        db.add("Robot", "Arm arm; double speed").unwrap();
        let path = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_enum_field(&db, "Robot", &path(&["arm", "mode"])));
        assert!(!is_enum_field(&db, "Robot", &path(&["arm", "angle"])));
        assert!(!is_enum_field(&db, "Robot", &path(&["speed", "mode"])));
        assert!(!is_enum_field(&db, "Robot", &path(&[])));
        assert!(!is_enum_field(&db, "Unknown", &path(&["mode"])));
        // Values two levels down show by name, number or both.
        let bytes = [3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mode = path(&["arm", "mode"]);
        let shown = |how| field_value_shown(&db, "Robot", &bytes, &mode, how, IntStyle::default());
        assert_eq!(
            shown(EnumDisplay::Name),
            Some(Value::String("kIntake".into()))
        );
        assert_eq!(shown(EnumDisplay::Number), Some(Value::Int(3)));
    }

    #[test]
    fn robot_2027_test_enums_are_found() {
        // The schema robot-2027's dashboard_test_topics.py publishes.
        let mut db = StructDatabase::new();
        db.add(
            "DashTestMode",
            "enum {kIdle=0, kIntake=1, kShoot=2, kClimb=3} uint8 mode;\
             enum {kOk=0, kWarn=1, kFault=-1} int8 health;double elapsed_s",
        )
        .unwrap();
        let one = |name: &str| vec![name.to_string()];
        assert!(is_enum_field(&db, "DashTestMode", &one("mode")));
        assert!(is_enum_field(&db, "DashTestMode", &one("health")));
        assert!(!is_enum_field(&db, "DashTestMode", &one("elapsed_s")));
    }
}
