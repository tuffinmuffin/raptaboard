//! NT4 topic type strings and their binary type ids.

use std::fmt;

/// The data type a topic announces.
///
/// NT4 sends the type as a string in `announce` and `publish` messages, and
/// as a small integer id in each binary value update.
///
/// ```
/// use raptaboard_nt4::NtType;
/// let t: NtType = "struct:Pose2d[]".parse().unwrap();
/// assert_eq!(t, NtType::Struct { name: "Pose2d".into(), array: true });
/// assert_eq!(t.to_string(), "struct:Pose2d[]");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NtType {
    Boolean,
    Double,
    Int,
    Float,
    String,
    Json,
    Raw,
    Rpc,
    MsgPack,
    BooleanArray,
    DoubleArray,
    IntArray,
    FloatArray,
    StringArray,
    /// A WPILib struct, decoded with the schema at `/.schema/struct:<name>`.
    Struct {
        name: String,
        array: bool,
    },
    /// A protobuf message type.
    Proto {
        name: String,
    },
    /// The schema text of a struct (`structschema`).
    StructSchema,
    /// Anything else; kept so unknown topics still show in the tree.
    Other(String),
}

impl NtType {
    /// Binary type id used in value updates.
    pub fn type_id(&self) -> u8 {
        match self {
            NtType::Boolean => 0,
            NtType::Double => 1,
            NtType::Int => 2,
            NtType::Float => 3,
            NtType::String | NtType::Json => 4,
            NtType::BooleanArray => 16,
            NtType::DoubleArray => 17,
            NtType::IntArray => 18,
            NtType::FloatArray => 19,
            NtType::StringArray => 20,
            _ => 5,
        }
    }

    /// True for plain numbers (int, float, double).
    pub fn is_number(&self) -> bool {
        matches!(self, NtType::Double | NtType::Int | NtType::Float)
    }

    /// True for types a text widget can show directly.
    pub fn is_viewable(&self) -> bool {
        matches!(
            self,
            NtType::Boolean
                | NtType::Double
                | NtType::Int
                | NtType::Float
                | NtType::String
                | NtType::BooleanArray
                | NtType::DoubleArray
                | NtType::IntArray
                | NtType::FloatArray
                | NtType::StringArray
        )
    }
}

impl std::str::FromStr for NtType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "boolean" => NtType::Boolean,
            "double" => NtType::Double,
            "int" => NtType::Int,
            "float" => NtType::Float,
            "string" => NtType::String,
            "json" => NtType::Json,
            "raw" => NtType::Raw,
            "rpc" => NtType::Rpc,
            "msgpack" => NtType::MsgPack,
            "boolean[]" => NtType::BooleanArray,
            "double[]" => NtType::DoubleArray,
            "int[]" => NtType::IntArray,
            "float[]" => NtType::FloatArray,
            "string[]" => NtType::StringArray,
            "structschema" => NtType::StructSchema,
            other => {
                if let Some(rest) = other.strip_prefix("struct:") {
                    match rest.strip_suffix("[]") {
                        Some(name) => NtType::Struct {
                            name: name.into(),
                            array: true,
                        },
                        None => NtType::Struct {
                            name: rest.into(),
                            array: false,
                        },
                    }
                } else if let Some(name) = other.strip_prefix("proto:") {
                    NtType::Proto { name: name.into() }
                } else {
                    NtType::Other(other.into())
                }
            }
        })
    }
}

impl fmt::Display for NtType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NtType::Boolean => "boolean",
            NtType::Double => "double",
            NtType::Int => "int",
            NtType::Float => "float",
            NtType::String => "string",
            NtType::Json => "json",
            NtType::Raw => "raw",
            NtType::Rpc => "rpc",
            NtType::MsgPack => "msgpack",
            NtType::BooleanArray => "boolean[]",
            NtType::DoubleArray => "double[]",
            NtType::IntArray => "int[]",
            NtType::FloatArray => "float[]",
            NtType::StringArray => "string[]",
            NtType::StructSchema => "structschema",
            NtType::Struct { name, array } => {
                return write!(f, "struct:{name}{}", if *array { "[]" } else { "" });
            }
            NtType::Proto { name } => return write!(f, "proto:{name}"),
            NtType::Other(s) => s,
        };
        f.write_str(s)
    }
}

impl serde::Serialize for NtType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for NtType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(s.parse().unwrap_or_else(|never| match never {}))
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn every_named_type_round_trips() {
        for s in [
            "boolean",
            "double",
            "int",
            "float",
            "string",
            "json",
            "raw",
            "rpc",
            "msgpack",
            "boolean[]",
            "double[]",
            "int[]",
            "float[]",
            "string[]",
            "structschema",
            "struct:Pose2d",
            "struct:SwerveModuleState[]",
            "proto:Pose3d",
            "weird",
        ] {
            let t: NtType = s.parse().unwrap();
            assert_eq!(t.to_string(), s);
        }
    }

    #[test]
    fn type_ids_match_the_spec() {
        assert_eq!(NtType::Boolean.type_id(), 0);
        assert_eq!(NtType::Json.type_id(), 4);
        assert_eq!(
            NtType::Struct {
                name: "Pose2d".into(),
                array: false
            }
            .type_id(),
            5
        );
        assert_eq!(NtType::StringArray.type_id(), 20);
    }
}
