//! Swerve modules: each wheel's speed and which way it points, read from
//! what robot code publishes. A WPILib `SwerveModuleState[]` (RobotPy and
//! WPILib 2026; 2027 renames it `SwerveModuleVelocity[]`, its `speed`
//! becoming `velocity`), or Elastic's `SwerveDrive` table, which a team
//! builds by hand: `Front Left Angle`, `Front Left Velocity` and the same
//! for the other three, and `Robot Angle`.
//!
//! Modules are in WPILib's usual order: front left, front right, back
//! left, back right.

use std::collections::HashMap;

use nt4::Value;
use nt4::proto::structs::StructDatabase;

use super::structs;

/// Elastic's swerve table's `.type`.
pub const SWERVE_TYPE: &str = "SwerveDrive";
/// The modules' names in Elastic's table, in order.
pub const CORNER_NAMES: [&str; 4] = ["Front Left", "Front Right", "Back Left", "Back Right"];
/// Short names for the modules, in order.
pub const CORNER_SHORT: [&str; 4] = ["FL", "FR", "BL", "BR"];
/// The robot's heading in Elastic's table.
pub const ROBOT_ANGLE: &str = "Robot Angle";

/// One swerve module: how fast its wheel drives and which way it points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Module {
    /// The wheel's speed, meters per second (negative drives backwards).
    pub speed: f64,
    /// Which way the wheel points, radians counterclockwise from the
    /// robot's front.
    pub angle: f64,
}

/// The unit Elastic's swerve table's angles are in (its `rotation_unit`
/// setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationUnit {
    /// Radians.
    Radians,
    /// Degrees (what Elastic reads when a file doesn't say).
    Degrees,
    /// Whole turns.
    Rotations,
}

impl RotationUnit {
    /// Every unit, in the order a settings list shows them.
    pub const ALL: [RotationUnit; 3] = [Self::Radians, Self::Degrees, Self::Rotations];

    /// The unit's name as Elastic saves it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Radians => "Radians",
            Self::Degrees => "Degrees",
            Self::Rotations => "Rotations",
        }
    }

    /// The unit saved as `key` (degrees for anything else, as Elastic reads
    /// a file without one).
    pub fn from_key(key: &str) -> RotationUnit {
        Self::ALL
            .into_iter()
            .find(|u| u.key() == key)
            .unwrap_or(Self::Degrees)
    }

    /// `angle` in this unit, as radians.
    ///
    /// ```
    /// use raptaboard::types::swerve::RotationUnit;
    /// assert_eq!(RotationUnit::Degrees.radians(180.0), std::f64::consts::PI);
    /// assert_eq!(RotationUnit::Rotations.radians(0.5), std::f64::consts::PI);
    /// ```
    pub fn radians(self, angle: f64) -> f64 {
        match self {
            Self::Radians => angle,
            Self::Degrees => angle.to_radians(),
            Self::Rotations => angle * std::f64::consts::TAU,
        }
    }
}

/// Whether a topic of NT type `nt_type` holds module states.
///
/// ```
/// use raptaboard::types::swerve::is_module_array;
/// assert!(is_module_array("struct:SwerveModuleState[]"));
/// assert!(is_module_array("struct:SwerveModuleVelocity[]"));
/// assert!(!is_module_array("struct:SwerveModuleState"));
/// ```
pub fn is_module_array(nt_type: &str) -> bool {
    matches!(
        structs::struct_type(nt_type),
        Some(("SwerveModuleState" | "SwerveModuleVelocity", true))
    )
}

/// The modules in `value`, a `SwerveModuleState[]` or
/// `SwerveModuleVelocity[]` topic of NT type `nt_type`. Empty if it's
/// neither, or the server's schemas (`structs`) haven't arrived.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::types::swerve::modules_of;
/// let mut db = StructDatabase::new();
/// db.add("Rotation2d", "double value").unwrap();
/// db.add("SwerveModuleState", "double speed;Rotation2d angle").unwrap();
/// let bytes: Vec<u8> = [2.0f64, 0.5, -1.0, 0.0].iter().flat_map(|d| d.to_le_bytes()).collect();
/// let m = modules_of(&Value::Raw(bytes), "struct:SwerveModuleState[]", Some(&db));
/// assert_eq!(m.len(), 2);
/// assert_eq!((m[0].speed, m[0].angle), (2.0, 0.5));
/// assert_eq!(m[1].speed, -1.0);
/// ```
pub fn modules_of(value: &Value, nt_type: &str, structs: Option<&StructDatabase>) -> Vec<Module> {
    let (Value::Raw(bytes), Some(db)) = (value, structs) else {
        return Vec::new();
    };
    if !is_module_array(nt_type) {
        return Vec::new();
    }
    let schema = structs::struct_type(nt_type).map_or("", |(name, _)| name);
    let number = |i: usize, path: &[&str]| -> Option<f64> {
        let path: Vec<String> = std::iter::once(format!("[{i}]"))
            .chain(path.iter().map(|s| (*s).to_owned()))
            .collect();
        match structs::field_value(db, schema, bytes, &path)? {
            Value::Double(d) => Some(d),
            Value::Float(f) => Some(f64::from(f)),
            _ => None,
        }
    };
    (0..)
        .map_while(|i| {
            // 2026 calls the speed `speed`; 2027 `velocity`.
            let speed = number(i, &["speed"]).or_else(|| number(i, &["velocity"]))?;
            let angle = number(i, &["angle", "value"])?;
            Some(Module { speed, angle })
        })
        .collect()
}

/// The topics in Elastic's swerve table at `table`: each module's angle and
/// velocity, then the robot's angle.
///
/// ```
/// use raptaboard::types::swerve::table_topics;
/// let t = table_topics("/SmartDashboard/Swerve");
/// assert_eq!(t[0], "/SmartDashboard/Swerve/Front Left Angle");
/// assert_eq!(t.last().unwrap(), "/SmartDashboard/Swerve/Robot Angle");
/// ```
pub fn table_topics(table: &str) -> Vec<String> {
    let table = table.trim_end_matches('/');
    CORNER_NAMES
        .iter()
        .flat_map(|c| {
            [
                format!("{table}/{c} Angle"),
                format!("{table}/{c} Velocity"),
            ]
        })
        .chain([format!("{table}/{ROBOT_ANGLE}")])
        .collect()
}

/// A number from `others`, by name.
fn number_in(others: &HashMap<&str, &Value>, name: &str) -> Option<f64> {
    match others.get(name)? {
        Value::Double(d) => Some(*d),
        Value::Float(f) => Some(f64::from(*f)),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// The modules in Elastic's swerve table at `table`, from the values in
/// `others`, angles in `unit`. A module with nothing sent reads as 0, as
/// Elastic reads it; empty if the table has sent nothing at all.
pub fn table_modules(
    table: &str,
    others: &HashMap<&str, &Value>,
    unit: RotationUnit,
) -> Vec<Module> {
    let table = table.trim_end_matches('/');
    let read: Vec<(Option<f64>, Option<f64>)> = CORNER_NAMES
        .iter()
        .map(|c| {
            (
                number_in(others, &format!("{table}/{c} Velocity")),
                number_in(others, &format!("{table}/{c} Angle")),
            )
        })
        .collect();
    if read.iter().all(|(s, a)| s.is_none() && a.is_none()) {
        return Vec::new();
    }
    read.into_iter()
        .map(|(speed, angle)| Module {
            speed: speed.unwrap_or(0.0),
            angle: unit.radians(angle.unwrap_or(0.0)),
        })
        .collect()
}

/// The robot's heading in Elastic's swerve table at `table`, radians
/// counterclockwise, from `others`, in `unit`.
pub fn table_robot_angle(
    table: &str,
    others: &HashMap<&str, &Value>,
    unit: RotationUnit,
) -> Option<f64> {
    let name = format!("{}/{ROBOT_ANGLE}", table.trim_end_matches('/'));
    number_in(others, &name).map(|a| unit.radians(a))
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn the_2027_name_and_field_read_the_same() {
        let mut db = StructDatabase::new();
        db.add("Rotation2d", "double value").unwrap();
        db.add("SwerveModuleVelocity", "double velocity;Rotation2d angle")
            .unwrap();
        let bytes: Vec<u8> = [1.5f64, -0.25]
            .iter()
            .flat_map(|d| d.to_le_bytes())
            .collect();
        let m = modules_of(
            &Value::Raw(bytes),
            "struct:SwerveModuleVelocity[]",
            Some(&db),
        );
        assert_eq!(
            m,
            [Module {
                speed: 1.5,
                angle: -0.25
            }]
        );
    }

    #[test]
    fn elastics_table_reads_in_its_unit_and_missing_values_are_zero() {
        let fl_angle = Value::Double(90.0);
        let fl_speed = Value::Double(2.0);
        let robot = Value::Double(0.25);
        let others: HashMap<&str, &Value> = HashMap::from([
            ("/S/Front Left Angle", &fl_angle),
            ("/S/Front Left Velocity", &fl_speed),
            ("/S/Robot Angle", &robot),
        ]);
        let m = table_modules("/S/", &others, RotationUnit::Degrees);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].speed, 2.0);
        assert!((m[0].angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(m[3], Module::default());
        let turn = table_robot_angle("/S", &others, RotationUnit::Rotations).unwrap();
        assert!((turn - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(table_modules("/Other", &others, RotationUnit::Degrees).is_empty());
        assert_eq!(RotationUnit::from_key("bogus"), RotationUnit::Degrees);
    }
}
