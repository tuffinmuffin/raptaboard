//! The robot data types raptaboard reads: how a table or topic robot code
//! publishes is turned into something to show, apart from the widgets that
//! draw it (in [`crate::widgets`]). More custom data structures are meant to
//! join these.
//!
//! - [`alerts`]: WPILib's alert groups (errors, warnings, infos).
//! - [`chooser`]: a SendableChooser's table (options, default, active,
//!   selected).
//! - [`structs`]: WPILib struct values, decoded with the schemas the server
//!   sends, as labelled fields.
//! - [`field`]: a field (size, image, AprilTags) and poses on it.
//! - [`angle`]: headings (a `Rotation2d`, a gyro's degrees) and which way
//!   they count.
//! - [`enums`]: named states on a string or int topic, and where their
//!   names come from.

pub mod alerts;
pub mod angle;
pub mod chooser;
pub mod enums;
pub mod field;
pub mod structs;

/// A number with up to 4 decimal places and no trailing zeros, as the
/// tree and struct fields show numbers.
///
/// ```
/// use raptaboard::types::format_number;
/// assert_eq!(format_number(1.0 / 3.0), "0.3333");
/// assert_eq!(format_number(2.50), "2.5");
/// assert_eq!(format_number(-0.00001), "0");
/// ```
pub fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let text = format!("{n:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}
