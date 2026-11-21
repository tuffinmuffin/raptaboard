//! How a robot is moving: its velocity and spin, from what robot code
//! publishes. A WPILib `ChassisSpeeds` (2027 renames it
//! `ChassisVelocities`): `vx` forward and `vy` left in meters per second,
//! `omega` counterclockwise in radians per second, relative to the robot
//! (robot code turns field-relative commands into robot-relative ones). A
//! `Translation2d` can stand for a velocity with no spin.

use nt4::Value;
use nt4::proto::structs::StructDatabase;

use super::structs;

/// A robot's motion, relative to the robot.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Motion {
    /// Speed forward, meters per second.
    pub vx: f64,
    /// Speed to the left, meters per second.
    pub vy: f64,
    /// Spin, radians per second counterclockwise; `None` for a velocity
    /// with no spin (a `Translation2d`).
    pub omega: Option<f64>,
}

impl Motion {
    /// How fast the robot is moving, meters per second, whichever way.
    ///
    /// ```
    /// use raptaboard::types::motion::Motion;
    /// assert_eq!(Motion { vx: 3.0, vy: 4.0, omega: None }.speed(), 5.0);
    /// ```
    pub fn speed(&self) -> f64 {
        self.vx.hypot(self.vy)
    }
}

/// Whether a topic of NT type `ty` holds a motion [`motion_of`] reads.
///
/// ```
/// use raptaboard::types::motion::is_motion_type;
/// assert!(is_motion_type("struct:ChassisSpeeds"));
/// assert!(is_motion_type("struct:ChassisVelocities"));
/// assert!(is_motion_type("struct:Translation2d"));
/// assert!(!is_motion_type("struct:ChassisSpeeds[]"));
/// ```
pub fn is_motion_type(ty: &str) -> bool {
    matches!(
        structs::struct_type(ty),
        Some((
            "ChassisSpeeds" | "ChassisVelocities" | "Translation2d",
            false
        ))
    )
}

/// The motion in `value`, a topic of NT type `nt_type` (see
/// [`is_motion_type`]). `None` if it's none of those, or the server's
/// schemas (`structs`) haven't arrived.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::types::motion::motion_of;
/// let mut db = StructDatabase::new();
/// db.add("ChassisSpeeds", "double vx;double vy;double omega").unwrap();
/// let bytes: Vec<u8> = [1.0f64, -0.5, 0.25].iter().flat_map(|d| d.to_le_bytes()).collect();
/// let m = motion_of(&Value::Raw(bytes), "struct:ChassisSpeeds", Some(&db)).unwrap();
/// assert_eq!((m.vx, m.vy, m.omega), (1.0, -0.5, Some(0.25)));
/// ```
pub fn motion_of(value: &Value, nt_type: &str, structs: Option<&StructDatabase>) -> Option<Motion> {
    let (Value::Raw(bytes), Some(db)) = (value, structs) else {
        return None;
    };
    if !is_motion_type(nt_type) {
        return None;
    }
    let (schema, _) = structs::struct_type(nt_type)?;
    let number = |field: &str| -> Option<f64> {
        match structs::field_value(db, schema, bytes, &[field.to_owned()])? {
            Value::Double(d) => Some(d),
            Value::Float(f) => Some(f64::from(f)),
            _ => None,
        }
    };
    if schema == "Translation2d" {
        return Some(Motion {
            vx: number("x")?,
            vy: number("y")?,
            omega: None,
        });
    }
    Some(Motion {
        vx: number("vx")?,
        vy: number("vy")?,
        omega: Some(number("omega")?),
    })
}

/// The motion in `value` when its topic's type isn't known (a widget's
/// second topic): a `ChassisSpeeds` (or `ChassisVelocities`) is 24 bytes,
/// a `Translation2d` 16.
pub fn untyped_motion(value: &Value, structs: Option<&StructDatabase>) -> Option<Motion> {
    let Value::Raw(bytes) = value else {
        return None;
    };
    let types: &[&str] = match bytes.len() {
        24 => &["struct:ChassisSpeeds", "struct:ChassisVelocities"],
        16 => &["struct:Translation2d"],
        _ => return None,
    };
    types.iter().find_map(|ty| motion_of(value, ty, structs))
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn raw(values: &[f64]) -> Value {
        Value::Raw(values.iter().flat_map(|d| d.to_le_bytes()).collect())
    }

    #[test]
    fn the_2027_name_a_translation_and_untyped_values_read() {
        let mut db = StructDatabase::new();
        db.add("ChassisVelocities", "double vx;double vy;double omega")
            .unwrap();
        db.add("Translation2d", "double x;double y").unwrap();
        let speeds = raw(&[2.0, 0.0, -1.0]);
        let m = motion_of(&speeds, "struct:ChassisVelocities", Some(&db)).unwrap();
        assert_eq!(m.omega, Some(-1.0));
        // Untyped: by size, and whichever name the server has a schema for.
        assert_eq!(untyped_motion(&speeds, Some(&db)), Some(m));
        let v = untyped_motion(&raw(&[0.5, 1.5]), Some(&db)).unwrap();
        assert_eq!((v.vx, v.vy, v.omega), (0.5, 1.5, None));
        assert_eq!(untyped_motion(&raw(&[1.0]), Some(&db)), None);
        assert_eq!(motion_of(&speeds, "struct:Pose2d", Some(&db)), None);
        assert_eq!(motion_of(&speeds, "struct:ChassisVelocities", None), None);
    }
}
