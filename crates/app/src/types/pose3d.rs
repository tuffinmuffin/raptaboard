//! Poses and rotations in 3D, read from what robot code publishes: a WPILib
//! `Pose3d` or `Transform3d` (a translation and a rotation), a `Rotation3d`
//! or a `Quaternion`. WPILib's frame: +X forward, +Y left, +Z up; a
//! rotation is a unit quaternion.

use nt4::Value;
use nt4::proto::structs::StructDatabase;

use super::structs;

/// A rotation in 3D as a unit quaternion (`w` the real part), as WPILib's
/// `Rotation3d` holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    /// The real part.
    pub w: f64,
    /// The X part.
    pub x: f64,
    /// The Y part.
    pub y: f64,
    /// The Z part.
    pub z: f64,
}

impl Default for Quaternion {
    /// No rotation.
    fn default() -> Self {
        Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

impl Quaternion {
    /// A turn of `radians` counterclockwise about +Z (a yaw).
    ///
    /// ```
    /// use raptaboard::types::pose3d::Quaternion;
    /// let q = Quaternion::from_yaw(std::f64::consts::FRAC_PI_2);
    /// let v = q.rotate([1.0, 0.0, 0.0]);
    /// assert!(v[0].abs() < 1e-12 && (v[1] - 1.0).abs() < 1e-12, "+X turns to +Y");
    /// ```
    pub fn from_yaw(radians: f64) -> Quaternion {
        let (s, c) = (radians / 2.0).sin_cos();
        Quaternion {
            w: c,
            x: 0.0,
            y: 0.0,
            z: s,
        }
    }

    /// This quaternion scaled to length 1 (no rotation if it's all zeros).
    pub fn normalized(self) -> Quaternion {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if n < 1e-12 || !n.is_finite() {
            return Quaternion::default();
        }
        Quaternion {
            w: self.w / n,
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
        }
    }

    /// `v` (x, y, z) turned by this rotation.
    pub fn rotate(self, v: [f64; 3]) -> [f64; 3] {
        let q = self.normalized();
        let u = [q.x, q.y, q.z];
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        // v + 2w(u × v) + 2u × (u × v)
        let t = cross(u, v).map(|c| 2.0 * c);
        let ut = cross(u, t);
        [
            v[0] + q.w * t[0] + ut[0],
            v[1] + q.w * t[1] + ut[1],
            v[2] + q.w * t[2] + ut[2],
        ]
    }

    /// Roll, pitch and yaw, radians: the turns about +X, +Y and +Z, as
    /// WPILib's `Rotation3d` `getX`, `getY` and `getZ` give them.
    ///
    /// ```
    /// use raptaboard::types::pose3d::Quaternion;
    /// let (roll, pitch, yaw) = Quaternion::from_yaw(0.5).roll_pitch_yaw();
    /// assert!(roll.abs() < 1e-12 && pitch.abs() < 1e-12 && (yaw - 0.5).abs() < 1e-12);
    /// ```
    pub fn roll_pitch_yaw(self) -> (f64, f64, f64) {
        let Quaternion { w, x, y, z } = self.normalized();
        let roll = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
        let pitch = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
        let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
        (roll, pitch, yaw)
    }
}

/// A pose in 3D: where (meters, if it has a place; a rotation alone
/// doesn't) and which way it's turned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose3 {
    /// x, y, z in meters; `None` for a rotation alone.
    pub translation: Option<[f64; 3]>,
    /// Which way it's turned.
    pub rotation: Quaternion,
}

/// The struct names [`pose3_of`] reads.
const POSE3_STRUCTS: [&str; 4] = ["Pose3d", "Transform3d", "Rotation3d", "Quaternion"];

/// Whether a topic of NT type `ty` holds a 3D pose or rotation
/// [`pose3_of`] reads (it reads a `Pose2d` too, flat on the floor).
///
/// ```
/// use raptaboard::types::pose3d::is_pose3_type;
/// assert!(is_pose3_type("struct:Pose3d") && is_pose3_type("struct:Rotation3d"));
/// assert!(!is_pose3_type("struct:Pose3d[]") && !is_pose3_type("struct:Pose2d"));
/// ```
pub fn is_pose3_type(ty: &str) -> bool {
    matches!(structs::struct_type(ty), Some((name, false)) if POSE3_STRUCTS.contains(&name))
}

/// The pose in `value`, a topic of NT type `nt_type` (see
/// [`is_pose3_type`], or a `Pose2d`, on the floor turned about +Z). `None` if it's none of those, or the server's schemas
/// (`structs`) haven't arrived.
///
/// ```
/// use nt4::Value;
/// use nt4::proto::structs::StructDatabase;
/// use raptaboard::types::pose3d::pose3_of;
/// let mut db = StructDatabase::new();
/// db.add("Quaternion", "double w;double x;double y;double z").unwrap();
/// db.add("Rotation3d", "Quaternion q").unwrap();
/// let bytes: Vec<u8> = [1.0f64, 0.0, 0.0, 0.0].iter().flat_map(|d| d.to_le_bytes()).collect();
/// let p = pose3_of(&Value::Raw(bytes), "struct:Rotation3d", Some(&db)).unwrap();
/// assert_eq!(p.translation, None);
/// assert_eq!(p.rotation.w, 1.0);
/// ```
pub fn pose3_of(value: &Value, nt_type: &str, structs: Option<&StructDatabase>) -> Option<Pose3> {
    let (Value::Raw(bytes), Some(db)) = (value, structs) else {
        return None;
    };
    if !is_pose3_type(nt_type) && nt_type != "struct:Pose2d" {
        return None;
    }
    let (schema, _) = structs::struct_type(nt_type)?;
    if schema == "Pose2d" {
        // Flat: on the floor, turned about +Z.
        let p = super::field::pose_of(value, nt_type, Some(db))?;
        return Some(Pose3 {
            translation: Some([p.x, p.y, 0.0]),
            rotation: Quaternion::from_yaw(p.heading),
        });
    }
    let number = |path: &[&str]| -> Option<f64> {
        let path: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
        match structs::field_value(db, schema, bytes, &path)? {
            Value::Double(d) => Some(d),
            Value::Float(f) => Some(f64::from(f)),
            _ => None,
        }
    };
    // Where the quaternion is inside the struct.
    let q_at: &[&str] = match schema {
        "Pose3d" | "Transform3d" => &["rotation", "q"],
        "Rotation3d" => &["q"],
        _ => &[],
    };
    let part = |name: &str| -> Option<f64> {
        let path: Vec<&str> = q_at.iter().copied().chain([name]).collect();
        number(&path)
    };
    let rotation = Quaternion {
        w: part("w")?,
        x: part("x")?,
        y: part("y")?,
        z: part("z")?,
    }
    .normalized();
    let translation = match schema {
        "Pose3d" | "Transform3d" => Some([
            number(&["translation", "x"])?,
            number(&["translation", "y"])?,
            number(&["translation", "z"])?,
        ]),
        _ => None,
    };
    Some(Pose3 {
        translation,
        rotation,
    })
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn rotations_turn_vectors_and_read_back_as_roll_pitch_yaw() {
        // A quarter roll about +X takes +Y to +Z.
        let h = std::f64::consts::FRAC_PI_4;
        let roll = Quaternion {
            w: h.cos(),
            x: h.sin(),
            y: 0.0,
            z: 0.0,
        };
        assert!(close(roll.rotate([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]));
        let (r, p, y) = roll.roll_pitch_yaw();
        assert!((r - 2.0 * h).abs() < 1e-9 && p.abs() < 1e-9 && y.abs() < 1e-9);
        // A pitch of 30° (nose down, as WPILib's +Y turn) reads back.
        let half = 15f64.to_radians();
        let pitch = Quaternion {
            w: half.cos(),
            x: 0.0,
            y: half.sin(),
            z: 0.0,
        };
        let (_, p, _) = pitch.roll_pitch_yaw();
        assert!((p - 30f64.to_radians()).abs() < 1e-9);
        // An unnormalized or zero quaternion still works.
        let big = Quaternion {
            w: 2.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        assert!(close(big.rotate([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]));
        assert_eq!(
            Quaternion {
                w: 0.0,
                x: 0.0,
                y: 0.0,
                z: 0.0
            }
            .normalized(),
            Quaternion::default()
        );
    }

    #[test]
    fn a_pose3d_and_a_transform3d_read_their_place_and_turn() {
        let mut db = StructDatabase::new();
        db.add("Translation3d", "double x;double y;double z")
            .unwrap();
        db.add("Quaternion", "double w;double x;double y;double z")
            .unwrap();
        db.add("Rotation3d", "Quaternion q").unwrap();
        db.add("Pose3d", "Translation3d translation;Rotation3d rotation")
            .unwrap();
        db.add(
            "Transform3d",
            "Translation3d translation;Rotation3d rotation",
        )
        .unwrap();
        let q = Quaternion::from_yaw(1.0);
        let bytes: Vec<u8> = [1.0, 2.0, 3.0, q.w, q.x, q.y, q.z]
            .iter()
            .flat_map(|d| d.to_le_bytes())
            .collect();
        for ty in ["struct:Pose3d", "struct:Transform3d"] {
            let p = pose3_of(&Value::Raw(bytes.clone()), ty, Some(&db)).unwrap();
            assert_eq!(p.translation, Some([1.0, 2.0, 3.0]), "{ty}");
            assert!((p.rotation.roll_pitch_yaw().2 - 1.0).abs() < 1e-9, "{ty}");
        }
        assert!(pose3_of(&Value::Raw(bytes), "struct:Pose2d[]", Some(&db)).is_none());
    }
}
