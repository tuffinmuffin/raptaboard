//! WPILib's `Mechanism2d`: a drawing of an arm, elevator or other
//! mechanism as straight lines ("ligaments") on a canvas, as robot code
//! publishes it (`.type` "Mechanism2d"):
//!
//! - `dims`: the canvas's width and height (any unit; robot code's choice,
//!   often meters), and `backgroundColor`, a `#RRGGBB` string.
//! - Each root is a subtable with `x` and `y`: where its ligaments start.
//! - Each ligament is a subtable (`.type` "line") in its root or in another
//!   ligament, with `angle` (degrees counterclockwise, from the ligament
//!   it's on, or from +X for a root's), `length` (canvas units), `color`
//!   (`#RRGGBB`) and `weight` (line width).

use std::collections::{BTreeSet, HashMap};

use nt4::Value;

/// A Mechanism2d table's `.type`.
pub const MECHANISM_TYPE: &str = "Mechanism2d";
/// A ligament's `.type`.
const LIGAMENT_TYPE: &str = "line";

/// One straight piece of a mechanism.
#[derive(Debug, Clone, PartialEq)]
pub struct Ligament {
    /// Its name (the last part of its path).
    pub name: String,
    /// Degrees counterclockwise from the ligament it's on (or +X).
    pub angle: f64,
    /// Length, canvas units.
    pub length: f64,
    /// `#RRGGBB`, as sent (`None` if not sent).
    pub color: Option<String>,
    /// Line width, as sent (WPILib's default is 10).
    pub weight: f64,
    /// The ligaments on its end.
    pub children: Vec<Ligament>,
}

/// Where a mechanism's ligaments start.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    /// Its name.
    pub name: String,
    /// Where it is on the canvas, canvas units from the bottom left.
    pub x: f64,
    /// Up from the bottom.
    pub y: f64,
    /// The ligaments that start here.
    pub ligaments: Vec<Ligament>,
}

/// A whole `Mechanism2d`.
#[derive(Debug, Clone, PartialEq)]
pub struct Mechanism {
    /// The canvas's width and height, canvas units.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// The canvas's color, `#RRGGBB` as sent.
    pub background: Option<String>,
    /// Its roots.
    pub roots: Vec<Root>,
}

/// One ligament placed on the canvas: from `start` to `end` (canvas units,
/// y up), with its color and weight, and its path (`base/elevator/arm`).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// `root/ligament/...`.
    pub path: String,
    /// Where it starts.
    pub start: (f64, f64),
    /// Where it ends.
    pub end: (f64, f64),
    /// Degrees counterclockwise from +X, all its parents' turns added up.
    pub heading: f64,
    /// Its own angle as robot code sets it, degrees from the ligament it's
    /// on.
    pub angle: f64,
    /// Its length, canvas units.
    pub length: f64,
    /// `#RRGGBB`, as sent.
    pub color: Option<String>,
    /// Line width, as sent.
    pub weight: f64,
}

/// The topics a Mechanism2d table at `table` has on the server, from the
/// names of every topic the server has (`all`): everything under it.
///
/// ```
/// use raptaboard::types::mechanism::table_topics;
/// let all = ["/M/dims", "/M/base/x", "/Other/x"];
/// assert_eq!(table_topics("/M/", all.iter().copied()), ["/M/dims", "/M/base/x"]);
/// ```
pub fn table_topics<'a>(table: &str, all: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let prefix = format!("{}/", table.trim_end_matches('/'));
    all.into_iter()
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned)
        .collect()
}

/// The values under `table`, from `values` (every topic's newest value by
/// full name), by their path inside it (`base/elevator/angle`).
fn under<'a>(table: &str, values: &HashMap<&'a str, &'a Value>) -> HashMap<&'a str, &'a Value> {
    let prefix = format!("{}/", table.trim_end_matches('/'));
    values
        .iter()
        .filter_map(|(name, v)| Some((name.strip_prefix(prefix.as_str())?, *v)))
        .collect()
}

fn number(v: Option<&&Value>) -> Option<f64> {
    match v? {
        Value::Double(d) => Some(*d),
        Value::Float(f) => Some(f64::from(*f)),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn text(v: Option<&&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// The names of the subtables directly in `path` (`""` for the table
/// itself), in name order.
fn subtables(inside: &HashMap<&str, &Value>, path: &str) -> Vec<String> {
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    inside
        .keys()
        .filter_map(|name| {
            let rest = name.strip_prefix(prefix.as_str())?;
            let (first, more) = rest.split_once('/')?;
            (!more.is_empty() && !first.starts_with('.')).then(|| first.to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The ligaments directly in `path`.
fn ligaments(inside: &HashMap<&str, &Value>, path: &str) -> Vec<Ligament> {
    subtables(inside, path)
        .into_iter()
        .filter_map(|name| {
            let at = format!("{path}/{name}");
            let field = |f: &str| inside.get(format!("{at}/{f}").as_str());
            let is_line = text(field(".type")).as_deref() == Some(LIGAMENT_TYPE);
            let length = number(field("length"));
            if !is_line && length.is_none() {
                return None;
            }
            Some(Ligament {
                name,
                angle: number(field("angle")).unwrap_or(0.0),
                length: length.unwrap_or(0.0),
                color: text(field("color")),
                weight: number(field("weight")).unwrap_or(10.0),
                children: ligaments(inside, &at),
            })
        })
        .collect()
}

/// The mechanism in the table `table`, from `values` (every topic's newest
/// value by full name). `None` if the table has sent nothing yet.
///
/// ```
/// use std::collections::HashMap;
/// use nt4::Value;
/// use raptaboard::types::mechanism::mechanism_of;
/// let dims = Value::DoubleArray(vec![3.0, 2.0]);
/// let (x, y) = (Value::Double(1.5), Value::Double(0.2));
/// let (line, length, angle) = (Value::String("line".into()), Value::Double(0.8), Value::Double(90.0));
/// let values: HashMap<&str, &Value> = HashMap::from([
///     ("/M/dims", &dims),
///     ("/M/base/x", &x),
///     ("/M/base/y", &y),
///     ("/M/base/lift/.type", &line),
///     ("/M/base/lift/length", &length),
///     ("/M/base/lift/angle", &angle),
/// ]);
/// let m = mechanism_of("/M", &values).unwrap();
/// assert_eq!((m.width, m.height), (3.0, 2.0));
/// assert_eq!(m.roots[0].ligaments[0].name, "lift");
/// ```
pub fn mechanism_of(table: &str, values: &HashMap<&str, &Value>) -> Option<Mechanism> {
    let inside = under(table, values);
    if inside.is_empty() {
        return None;
    }
    let (width, height) = match inside.get("dims") {
        Some(Value::DoubleArray(d)) if d.len() >= 2 => (d[0], d[1]),
        _ => (1.0, 1.0),
    };
    let roots = subtables(&inside, "")
        .into_iter()
        .filter_map(|name| {
            let x = number(inside.get(format!("{name}/x").as_str()))?;
            let y = number(inside.get(format!("{name}/y").as_str()))?;
            Some(Root {
                ligaments: ligaments(&inside, &name),
                name,
                x,
                y,
            })
        })
        .collect();
    Some(Mechanism {
        width: width.max(1e-6),
        height: height.max(1e-6),
        background: text(inside.get("backgroundColor")),
        roots,
    })
}

impl Mechanism {
    /// Every ligament placed on the canvas, parents before children.
    ///
    /// ```
    /// use raptaboard::types::mechanism::{Ligament, Mechanism, Root};
    /// let arm = Ligament { name: "arm".into(), angle: -90.0, length: 1.0, color: None, weight: 4.0, children: vec![] };
    /// let lift = Ligament { name: "lift".into(), angle: 90.0, length: 2.0, color: None, weight: 6.0, children: vec![arm] };
    /// let m = Mechanism { width: 4.0, height: 4.0, background: None,
    ///     roots: vec![Root { name: "base".into(), x: 1.0, y: 0.0, ligaments: vec![lift] }] };
    /// let s = m.segments();
    /// assert_eq!(s[1].path, "base/lift/arm");
    /// // Up 2, then turned back to +X for 1.
    /// assert!((s[1].end.0 - 2.0).abs() < 1e-9 && (s[1].end.1 - 2.0).abs() < 1e-9);
    /// ```
    pub fn segments(&self) -> Vec<Segment> {
        fn place(l: &Ligament, from: (f64, f64), parent: f64, path: &str, out: &mut Vec<Segment>) {
            let heading = parent + l.angle;
            let (s, c) = heading.to_radians().sin_cos();
            let end = (from.0 + l.length * c, from.1 + l.length * s);
            let path = format!("{path}/{}", l.name);
            out.push(Segment {
                path: path.clone(),
                start: from,
                end,
                heading,
                angle: l.angle,
                length: l.length,
                color: l.color.clone(),
                weight: l.weight,
            });
            for child in &l.children {
                place(child, end, heading, &path, out);
            }
        }
        let mut out = Vec::new();
        for root in &self.roots {
            for l in &root.ligaments {
                place(l, (root.x, root.y), 0.0, &root.name, &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn a_table_as_wpilib_publishes_it_builds_its_tree() {
        // From RobotPy's Mechanism2d(3, 2) with an elevator and an arm on it.
        let owned: Vec<(&str, Value)> = vec![
            ("/SD/Mech/.type", Value::String("Mechanism2d".into())),
            ("/SD/Mech/.name", Value::String("Mech".into())),
            ("/SD/Mech/backgroundColor", Value::String("#000020".into())),
            ("/SD/Mech/dims", Value::DoubleArray(vec![3.0, 2.0])),
            ("/SD/Mech/base/x", Value::Double(1.5)),
            ("/SD/Mech/base/y", Value::Double(0.2)),
            ("/SD/Mech/base/elevator/.type", Value::String("line".into())),
            ("/SD/Mech/base/elevator/angle", Value::Double(90.0)),
            ("/SD/Mech/base/elevator/length", Value::Double(0.8)),
            (
                "/SD/Mech/base/elevator/color",
                Value::String("#FF0000".into()),
            ),
            ("/SD/Mech/base/elevator/weight", Value::Double(6.0)),
            (
                "/SD/Mech/base/elevator/arm/.type",
                Value::String("line".into()),
            ),
            ("/SD/Mech/base/elevator/arm/angle", Value::Double(-45.0)),
            ("/SD/Mech/base/elevator/arm/length", Value::Double(0.5)),
            // A root with no x or y yet isn't one.
            ("/SD/Mech/half/y", Value::Double(1.0)),
            // Another table's topics aren't read.
            ("/SD/Other/base/x", Value::Double(9.0)),
        ];
        let values: HashMap<&str, &Value> = owned.iter().map(|(k, v)| (*k, v)).collect();
        let m = mechanism_of("/SD/Mech", &values).unwrap();
        assert_eq!(m.background.as_deref(), Some("#000020"));
        assert_eq!(m.roots.len(), 1);
        let s = m.segments();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].path, "base/elevator");
        assert!((s[0].end.0 - 1.5).abs() < 1e-9 && (s[0].end.1 - 1.0).abs() < 1e-9);
        assert_eq!(s[0].color.as_deref(), Some("#FF0000"));
        // The arm turns 45° back from straight up: up and to the right.
        assert!((s[1].heading - 45.0).abs() < 1e-9);
        assert!(s[1].end.0 > 1.5 && s[1].end.1 > 1.0);
        assert_eq!(s[1].weight, 10.0, "WPILib's default weight");
        assert!(mechanism_of("/SD/Nothing", &values).is_none());
    }
}
