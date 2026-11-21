//! The widget types built into raptaboard, one file each. Each is a unit
//! struct implementing [`WidgetType`](super::WidgetType) and is listed in
//! [`TYPES`](super::TYPES).

pub mod alerts;
pub mod boolean_box;
pub mod chassis_speeds;
pub mod chooser;
pub mod color_view;
pub mod enums;
pub mod field;
pub mod graph;
pub mod gyro;
pub mod large_text_display;
pub mod match_time;
pub mod number_bar;
pub mod number_slider;
pub mod pose3d;
pub mod radial_gauge;
pub mod struct_view;
pub mod swerve;
pub mod text_display;
pub mod toggle;
