//! A field and where things are on it: its size, image and AprilTags (from
//! a WPILib `fields` JSON), and robot poses read from the server.
//!
//! Poses here are in WPILib 2027's frame: origin at the center of the field
//! on the floor, +X toward the blue alliance wall, +Y left facing +X,
//! heading 0 facing +X and counterclockwise positive, in meters and
//! radians. [`Origin::BlueCorner`] reads the frame used before 2027.

use nt4::Value;
use nt4::proto::structs::StructDatabase;
use serde::Deserialize;

use crate::types::structs;

/// A field, as its WPILib `fields` JSON describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    /// The full name: `2026 FRC Rebuilt Welded`.
    pub name: String,
    /// The season: `2026`.
    pub season: String,
    /// The game: `Rebuilt`.
    pub game: String,
    /// Alliance wall to alliance wall, in meters.
    pub length_m: f64,
    /// Guardrail to guardrail, in meters.
    pub width_m: f64,
    /// The image, if the field has one.
    pub image: Option<ImageBounds>,
    /// The official AprilTag layout.
    pub tags: Vec<AprilTag>,
}

/// A field image and where the field's edges are in it, in pixels (x to
/// the right, y down; the blue alliance wall is on the right).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageBounds {
    /// The image file, relative to the JSON.
    pub path: String,
    /// The field's top edge.
    pub top: f32,
    /// The field's left edge (the red alliance wall).
    pub left: f32,
    /// The field's bottom edge.
    pub bottom: f32,
    /// The field's right edge (the blue alliance wall).
    pub right: f32,
}

/// One AprilTag on the field.
#[derive(Debug, Clone, PartialEq)]
pub struct AprilTag {
    /// Its ID.
    pub id: u32,
    /// Where its center is, in meters (2027 frame).
    pub x: f64,
    /// See [`AprilTag::x`].
    pub y: f64,
    /// Height of its center above the floor, in meters.
    pub z: f64,
    /// Which way it faces, in radians (0 faces +X).
    pub yaw: f64,
}

impl FieldInfo {
    /// Read a field from its JSON, or say what's wrong with it.
    ///
    /// ```
    /// use raptaboard::types::field::FieldInfo;
    /// let json = r#"{"name": "Test", "season": "2026", "game": "Test",
    ///     "field-dimensions": {"length": 16.5, "width": 8.0}}"#;
    /// let f = FieldInfo::parse(json).unwrap();
    /// assert_eq!((f.length_m, f.width_m), (16.5, 8.0));
    /// assert!(f.image.is_none() && f.tags.is_empty());
    /// assert!(FieldInfo::parse("{}").is_err());
    /// ```
    pub fn parse(json: &str) -> Result<FieldInfo, String> {
        let raw: RawField = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let size = raw.field_dimensions;
        if !(size.length > 0.0 && size.width > 0.0) {
            return Err("the field's length and width must be more than 0".to_owned());
        }
        Ok(FieldInfo {
            name: raw.name,
            season: raw.season,
            game: raw.game,
            length_m: size.length,
            width_m: size.width,
            image: raw.field_image.map(|i| ImageBounds {
                path: i.path,
                top: i.top,
                left: i.left,
                bottom: i.bottom,
                right: i.right,
            }),
            tags: raw
                .field_tags
                .into_iter()
                .map(|t| {
                    let q = t.pose.rotation.quaternion;
                    AprilTag {
                        id: t.id,
                        x: t.pose.translation.x,
                        y: t.pose.translation.y,
                        z: t.pose.translation.z,
                        yaw: yaw_of(q.w, q.x, q.y, q.z),
                    }
                })
                .collect(),
        })
    }
}

/// The yaw (turn about +Z, radians) of the rotation in a quaternion.
fn yaw_of(w: f64, x: f64, y: f64, z: f64) -> f64 {
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawField {
    name: String,
    #[serde(default)]
    season: String,
    #[serde(default)]
    game: String,
    field_dimensions: RawSize,
    field_image: Option<RawImage>,
    #[serde(default)]
    field_tags: Vec<RawTag>,
}

#[derive(Deserialize)]
struct RawSize {
    length: f64,
    width: f64,
}

#[derive(Deserialize)]
struct RawImage {
    path: String,
    top: f32,
    left: f32,
    bottom: f32,
    right: f32,
}

#[derive(Deserialize)]
struct RawTag {
    #[serde(rename = "ID")]
    id: u32,
    pose: RawPose,
}

#[derive(Deserialize)]
struct RawPose {
    translation: RawTranslation,
    rotation: RawRotation,
}

#[derive(Deserialize)]
struct RawTranslation {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Deserialize)]
struct RawRotation {
    quaternion: RawQuaternion,
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct RawQuaternion {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

/// Where something is on the field, and which way it faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldPose {
    /// Meters along the field, toward the blue alliance wall (2027 frame).
    pub x: f64,
    /// Meters across the field, left facing +X.
    pub y: f64,
    /// Which way it faces, in radians (0 faces +X, counterclockwise positive).
    pub heading: f64,
}

/// Where the origin of the poses robot code sends is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Origin {
    /// WPILib 2027: the center of the field, +X toward the blue alliance wall.
    #[default]
    Center,
    /// Before 2027: the blue alliance wall's right corner, +X toward red.
    BlueCorner,
}

impl Origin {
    /// Both, in the order a menu lists them.
    pub const ALL: [Origin; 2] = [Origin::Center, Origin::BlueCorner];

    /// How it's saved: `center` or `blue_corner`.
    pub fn key(self) -> &'static str {
        match self {
            Origin::Center => "center",
            Origin::BlueCorner => "blue_corner",
        }
    }

    /// The origin saved as `key` ([`Origin::Center`] for anything else).
    pub fn from_key(key: &str) -> Origin {
        Origin::ALL
            .into_iter()
            .find(|o| o.key() == key)
            .unwrap_or_default()
    }

    /// `pose`, given in this origin's frame, in the 2027 frame on a field
    /// `length` by `width` meters.
    ///
    /// ```
    /// use raptaboard::types::field::{FieldPose, Origin};
    /// // The old frame's origin is the blue alliance corner.
    /// let corner = FieldPose { x: 0.0, y: 0.0, heading: 0.0 };
    /// let p = Origin::BlueCorner.to_center(corner, 16.0, 8.0);
    /// assert_eq!((p.x, p.y), (8.0, 4.0));
    /// assert!((p.heading - std::f64::consts::PI).abs() < 1e-9);
    /// assert_eq!(Origin::Center.to_center(corner, 16.0, 8.0), corner);
    /// ```
    pub fn to_center(self, pose: FieldPose, length: f64, width: f64) -> FieldPose {
        match self {
            Origin::Center => pose,
            Origin::BlueCorner => FieldPose {
                x: length / 2.0 - pose.x,
                y: width / 2.0 - pose.y,
                heading: wrap(pose.heading + std::f64::consts::PI),
            },
        }
    }
}

/// `angle` (radians) between -π and π.
fn wrap(angle: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let a = angle.rem_euclid(tau);
    if a > std::f64::consts::PI { a - tau } else { a }
}

/// The pose in `value`, a topic of NT type `nt_type`: a `struct:Pose2d`,
/// a `struct:Pose3d` (drawn flat: its x, y and yaw), or a `double[]` of x,
/// y and degrees (a `Field2d` object's first pose). `structs` decodes
/// structs; `None` until the server's schemas arrive.
///
/// ```
/// use nt4::Value;
/// use raptaboard::types::field::pose_of;
/// let robot = Value::DoubleArray(vec![1.0, 2.0, 90.0]);
/// let p = pose_of(&robot, "double[]", None).unwrap();
/// assert_eq!((p.x, p.y), (1.0, 2.0));
/// assert!((p.heading - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
/// ```
pub fn pose_of(
    value: &Value,
    nt_type: &str,
    structs: Option<&StructDatabase>,
) -> Option<FieldPose> {
    match value {
        Value::DoubleArray(v) if v.len() >= 3 => Some(FieldPose {
            x: v[0],
            y: v[1],
            heading: v[2].to_radians(),
        }),
        Value::Raw(bytes) => {
            let db = structs?;
            let number = |schema: &str, path: &[&str]| -> Option<f64> {
                let path: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
                match structs::field_value(db, schema, bytes, &path)? {
                    Value::Double(d) => Some(d),
                    Value::Float(f) => Some(f64::from(f)),
                    _ => None,
                }
            };
            match structs::struct_type(nt_type) {
                Some(("Pose3d", false)) => Some(FieldPose {
                    x: number("Pose3d", &["translation", "x"])?,
                    y: number("Pose3d", &["translation", "y"])?,
                    heading: yaw_of(
                        number("Pose3d", &["rotation", "q", "w"])?,
                        number("Pose3d", &["rotation", "q", "x"])?,
                        number("Pose3d", &["rotation", "q", "y"])?,
                        number("Pose3d", &["rotation", "q", "z"])?,
                    ),
                }),
                Some(("Pose2d", false)) => Some(FieldPose {
                    x: number("Pose2d", &["translation", "x"])?,
                    y: number("Pose2d", &["translation", "y"])?,
                    heading: number("Pose2d", &["rotation", "value"])?,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn the_built_in_field_reads_with_its_tags() {
        let json = include_str!("../../../../fields/frc/2026-rebuilt-welded.json");
        let f = FieldInfo::parse(json).unwrap();
        assert_eq!(f.game, "Rebuilt");
        assert!((f.length_m - 16.541).abs() < 1e-9);
        let image = f.image.unwrap();
        assert_eq!(image.path, "2026-rebuilt.png");
        assert!(image.right > image.left && image.bottom > image.top);
        assert_eq!(f.tags.len(), 32);
        // Every tag is on the field, in the centered frame.
        for t in &f.tags {
            assert!(
                t.x.abs() <= f.length_m / 2.0 + 0.1,
                "tag {} x {}",
                t.id,
                t.x
            );
            assert!(t.y.abs() <= f.width_m / 2.0 + 0.1, "tag {} y {}", t.id, t.y);
        }
        // Tag 1 faces +X (an identity quaternion).
        assert_eq!(f.tags[0].id, 1);
        assert!(f.tags[0].yaw.abs() < 1e-9);
    }

    #[test]
    fn poses_decode_from_structs_in_either_frame() {
        let mut db = StructDatabase::new();
        db.add("Translation2d", "double x;double y").unwrap();
        db.add("Rotation2d", "double value").unwrap();
        db.add("Pose2d", "Translation2d translation;Rotation2d rotation")
            .unwrap();
        db.add("Translation3d", "double x;double y;double z")
            .unwrap();
        db.add("Quaternion", "double w;double x;double y;double z")
            .unwrap();
        db.add("Rotation3d", "Quaternion q").unwrap();
        db.add("Pose3d", "Translation3d translation;Rotation3d rotation")
            .unwrap();
        let bytes = |v: &[f64]| Value::Raw(v.iter().flat_map(|d| d.to_le_bytes()).collect());
        let p = pose_of(&bytes(&[1.5, -2.0, 0.5]), "struct:Pose2d", Some(&db)).unwrap();
        assert_eq!(
            p,
            FieldPose {
                x: 1.5,
                y: -2.0,
                heading: 0.5
            }
        );
        // A quarter turn about Z: w = z = sqrt(1/2).
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let p3 = pose_of(
            &bytes(&[3.0, 4.0, 1.0, h, 0.0, 0.0, h]),
            "struct:Pose3d",
            Some(&db),
        )
        .unwrap();
        assert_eq!((p3.x, p3.y), (3.0, 4.0));
        assert!((p3.heading - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Without schemas yet, or an array type, nothing.
        assert_eq!(pose_of(&bytes(&[0.0; 3]), "struct:Pose2d", None), None);
        assert_eq!(
            pose_of(&bytes(&[0.0; 3]), "struct:Pose2d[]", Some(&db)),
            None
        );
        // The old frame turns half round about the field's center.
        let old = FieldPose {
            x: 16.0,
            y: 8.0,
            heading: -std::f64::consts::FRAC_PI_2,
        };
        let new = Origin::BlueCorner.to_center(old, 16.0, 8.0);
        assert_eq!((new.x, new.y), (-8.0, -4.0));
        assert!((new.heading - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(Origin::from_key("blue_corner"), Origin::BlueCorner);
        assert_eq!(Origin::from_key("?"), Origin::Center);
    }
}
