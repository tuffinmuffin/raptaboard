//! The raptaboard desktop app.
//!
//! Milestone 1 status: shows a loaded Elastic layout (tabs and widget boxes on
//! the grid), the connection settings, and a live NT4 connection to the robot
//! (status and topic count). The NT tree and real widgets come next; see
//! `docs/spec.md` for the plan.

pub mod cli;
pub mod connection;
pub mod nt;
pub mod ui;
