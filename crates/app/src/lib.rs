//! The raptaboard desktop app.
//!
//! Status: shows a loaded Elastic layout (tabs and widgets on the grid; tabs
//! and widgets can be edited, see `tabs` and `grid`), the connection settings, and a live NT4 connection to the robot
//! (status and topic count), and an NT tree of every topic with live values for
//! open folders, and Text Display and Boolean Box widgets with live values
//! (other widget types show as placeholders). See `docs/spec.md` for the plan.

pub mod cli;
pub mod connection;
pub mod grid;
pub mod layout_file;
pub mod nt;
pub mod struct_view;
pub mod tabs;
pub mod tree;
pub mod ui;
pub mod widgets;
