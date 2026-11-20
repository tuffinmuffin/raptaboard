//! The robot data types raptaboard reads: how a table or topic robot code
//! publishes is turned into something to show, apart from the widgets that
//! draw it (in [`crate::widgets`]). More custom data structures are meant to
//! join these.
//!
//! - [`alerts`]: WPILib's alert groups (errors, warnings, infos).
//! - [`chooser`]: a SendableChooser's table (options, default, active,
//!   selected).

pub mod alerts;
pub mod chooser;
