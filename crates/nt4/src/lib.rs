//! NetworkTables 4.1 (NT4) protocol pieces for raptaboard.
//!
//! This crate holds the parts of NT4 that don't need a network socket, so they
//! can be tested on their own:
//!
//! - [`NtType`]: the type strings a topic announces (`"double"`, `"struct:Pose2d"`, ...).
//! - [`TextMessage`]: the JSON control messages (publish, subscribe, announce, ...).
//! - [`decode_binary_frame`]: the MessagePack value updates `[id, time, type, value]`.
//! - [`path`]: helpers for topic paths, such as finding a table's `.type` topic.
//!
//! The websocket client that uses these is the next step of milestone 1.
//! Protocol reference: <https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc>

pub mod messages;
pub mod path;
pub mod types;
pub mod value;

pub use messages::{SubscriptionOptions, TextMessage};
pub use types::NtType;
pub use value::{BinaryMessage, DecodeError, NtValue, decode_binary_frame, encode_binary_message};

/// Websocket subprotocol for NT 4.1, tried first.
pub const SUBPROTOCOL_V4_1: &str = "v4.1.networktables.first.wpi.edu";
/// Websocket subprotocol for NT 4.0, the fallback.
pub const SUBPROTOCOL_V4_0: &str = "networktables.first.wpi.edu";
/// NT4 port served by robot code.
pub const ROBOT_CODE_PORT: u16 = 5810;
/// NT4 port of the SystemCore internal server.
pub const SYSTEMCORE_PORT: u16 = 6810;

/// Build the websocket URL for a client, e.g. `ws://10.32.0.2:5810/nt/raptaboard`.
///
/// ```
/// assert_eq!(
///     raptaboard_nt4::server_url("10.32.0.2", 5810, "raptaboard"),
///     "ws://10.32.0.2:5810/nt/raptaboard"
/// );
/// ```
pub fn server_url(host: &str, port: u16, client_name: &str) -> String {
    format!("ws://{host}:{port}/nt/{client_name}")
}
