//! Headings: which way something faces, read from what robot code
//! publishes. A WPILib `Rotation2d` (radians, counterclockwise positive), a
//! `Rotation3d`'s yaw (its turn about +Z), a plain number of degrees (a
//! gyro's angle, which may count either way), or a WPILib `Gyro` table
//! (`.type` "Gyro", its angle in degrees in `Value`).

use nt4::Value;
use nt4::proto::structs::StructDatabase;

use super::structs;

/// A WPILib `Gyro` table's `.type` (a Sendable gyro, as Elastic reads it).
pub const GYRO_TYPE: &str = "Gyro";
/// The topic in a `Gyro` table holding its angle, in degrees.
pub const GYRO_VALUE: &str = "Value";

/// A heading as it was sent: `degrees`, and whether a bigger number turns
/// counterclockwise (`ccw`, WPILib's rotations) or clockwise (many gyros'
/// angle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heading {
    /// The angle as sent, in degrees (not wrapped).
    pub degrees: f64,
    /// Whether a bigger angle turns counterclockwise, seen from above.
    pub ccw: bool,
}

impl Heading {
    /// The angle wrapped to 0 up to (not including) 360 degrees, as a gyro
    /// shows it.
    ///
    /// ```
    /// use raptaboard::types::angle::Heading;
    /// assert_eq!(Heading { degrees: -90.0, ccw: true }.wrapped(), 270.0);
    /// assert_eq!(Heading { degrees: 725.0, ccw: false }.wrapped(), 5.0);
    /// ```
    pub fn wrapped(self) -> f64 {
        let w = self.degrees.rem_euclid(360.0);
        // -1e-15 wraps to 360 in floating point; that's 0.
        if w >= 360.0 { 0.0 } else { w }
    }

    /// The angle clockwise from straight up on screen, in degrees: how far
    /// a dial's needle turns (counterclockwise angles turn it left).
    ///
    /// ```
    /// use raptaboard::types::angle::Heading;
    /// assert_eq!(Heading { degrees: 90.0, ccw: true }.clockwise(), -90.0);
    /// assert_eq!(Heading { degrees: 90.0, ccw: false }.clockwise(), 90.0);
    /// ```
    pub fn clockwise(self) -> f64 {
        if self.ccw {
            -self.degrees
        } else {
            self.degrees
        }
    }
}

/// The heading in `value`, a topic of NT type `nt_type`: a
/// `struct:Rotation2d` or `struct:Rotation3d` (counterclockwise, as WPILib
/// defines them), or a number of degrees, counterclockwise if
/// `numbers_ccw`. `structs` decodes structs; `None` until the server's
/// schemas arrive, or if it's none of those.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::types::angle::heading_of;
/// let mut db = StructDatabase::new();
/// db.add("Rotation2d", "double value").unwrap();
/// let quarter = Value::Raw(std::f64::consts::FRAC_PI_2.to_le_bytes().to_vec());
/// let h = heading_of(&quarter, "struct:Rotation2d", Some(&db), false).unwrap();
/// assert!((h.degrees - 90.0).abs() < 1e-9 && h.ccw);
/// let gyro = heading_of(&Value::Double(45.0), "double", None, false).unwrap();
/// assert_eq!((gyro.degrees, gyro.ccw), (45.0, false));
/// ```
pub fn heading_of(
    value: &Value,
    nt_type: &str,
    structs: Option<&StructDatabase>,
    numbers_ccw: bool,
) -> Option<Heading> {
    let degrees = match value {
        Value::Double(d) => *d,
        Value::Float(f) => f64::from(*f),
        Value::Int(i) => *i as f64,
        Value::Raw(bytes) => {
            let db = structs?;
            let (schema, false) = structs::struct_type(nt_type)? else {
                return None;
            };
            let number = |path: &[&str]| -> Option<f64> {
                let path: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
                match structs::field_value(db, schema, bytes, &path)? {
                    Value::Double(d) => Some(d),
                    Value::Float(f) => Some(f64::from(f)),
                    _ => None,
                }
            };
            let radians = match schema {
                "Rotation2d" => number(&["value"])?,
                "Rotation3d" => super::field::yaw_of(
                    number(&["q", "w"])?,
                    number(&["q", "x"])?,
                    number(&["q", "y"])?,
                    number(&["q", "z"])?,
                ),
                _ => return None,
            };
            return Some(Heading {
                degrees: radians.to_degrees(),
                ccw: true,
            });
        }
        _ => return None,
    };
    degrees.is_finite().then_some(Heading {
        degrees,
        ccw: numbers_ccw,
    })
}

/// Where a `Gyro` table's angle is: `<table>/Value`.
///
/// ```
/// use raptaboard::types::angle::gyro_value_topic;
/// assert_eq!(gyro_value_topic("/SmartDashboard/Gyro/"), "/SmartDashboard/Gyro/Value");
/// ```
pub fn gyro_value_topic(table: &str) -> String {
    format!("{}/{GYRO_VALUE}", table.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn a_rotation3ds_yaw_is_its_heading() {
        let mut db = StructDatabase::new();
        db.add("Quaternion", "double w;double x;double y;double z")
            .unwrap();
        db.add("Rotation3d", "Quaternion q").unwrap();
        // A 30° turn about +Z: w = cos 15°, z = sin 15°.
        let half = 15f64.to_radians();
        let bytes: Vec<u8> = [half.cos(), 0.0, 0.0, half.sin()]
            .iter()
            .flat_map(|d| d.to_le_bytes())
            .collect();
        let h = heading_of(&Value::Raw(bytes), "struct:Rotation3d", Some(&db), false).unwrap();
        assert!((h.degrees - 30.0).abs() < 1e-9, "{h:?}");
        assert!(
            h.ccw,
            "rotations count counterclockwise, whatever numbers do"
        );
    }

    #[test]
    fn other_values_have_no_heading() {
        let mut db = StructDatabase::new();
        db.add("Rotation2d", "double value").unwrap();
        let bytes = Value::Raw(1f64.to_le_bytes().to_vec());
        assert!(heading_of(&bytes, "struct:Rotation2d[]", Some(&db), true).is_none());
        assert!(heading_of(&bytes, "struct:Pose2d", Some(&db), true).is_none());
        assert!(heading_of(&bytes, "struct:Rotation2d", None, true).is_none());
        assert!(heading_of(&Value::Double(f64::NAN), "double", None, true).is_none());
        assert!(heading_of(&Value::String("90".into()), "string", None, true).is_none());
        let ints = heading_of(&Value::Int(-30), "int", None, true).unwrap();
        assert_eq!(ints.wrapped(), 330.0);
    }
}
