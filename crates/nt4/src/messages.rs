//! NT4 JSON control messages.
//!
//! A text websocket frame holds a JSON array of these, each shaped like
//! `{"method": "subscribe", "params": {...}}`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::NtType;

/// Options sent with a `subscribe` message. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// How often the server sends values, in seconds (default 0.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periodic: Option<f64>,
    /// Send every value change, not only the latest per period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// Only announce topics; send no values.
    #[serde(rename = "topicsonly", skip_serializing_if = "Option::is_none")]
    pub topics_only: Option<bool>,
    /// Treat each entry in `topics` as a prefix, not an exact name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<bool>,
}

/// One NT4 control message, sent by the client or the server.
///
/// ```
/// use raptaboard_nt4::{SubscriptionOptions, TextMessage};
/// let msg = TextMessage::Subscribe {
///     topics: vec!["/Telemetry/".into()],
///     subuid: 1,
///     options: SubscriptionOptions { prefix: Some(true), ..Default::default() },
/// };
/// let json = serde_json::to_string(&msg).unwrap();
/// assert_eq!(
///     json,
///     r#"{"method":"subscribe","params":{"topics":["/Telemetry/"],"subuid":1,"options":{"prefix":true}}}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "lowercase")]
pub enum TextMessage {
    // Client to server.
    Publish {
        name: String,
        pubuid: i64,
        #[serde(rename = "type")]
        ty: NtType,
        #[serde(default)]
        properties: Map<String, Value>,
    },
    Unpublish {
        pubuid: i64,
    },
    SetProperties {
        name: String,
        update: Map<String, Value>,
    },
    Subscribe {
        topics: Vec<String>,
        subuid: i64,
        #[serde(default)]
        options: SubscriptionOptions,
    },
    Unsubscribe {
        subuid: i64,
    },
    // Server to client.
    Announce {
        name: String,
        id: i64,
        #[serde(rename = "type")]
        ty: NtType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pubuid: Option<i64>,
        #[serde(default)]
        properties: Map<String, Value>,
    },
    Unannounce {
        name: String,
        id: i64,
    },
    Properties {
        name: String,
        update: Map<String, Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ack: Option<bool>,
    },
}

/// Parse one text frame (a JSON array of messages).
///
/// Messages with an unknown `method` make the whole frame fail, so callers
/// should log and skip the frame rather than disconnect.
pub fn parse_text_frame(text: &str) -> Result<Vec<TextMessage>, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn parses_an_announce_with_unit_property() {
        let frame = r#"[{"method":"announce","params":{"name":"/Telemetry/Arm/angle","id":7,
            "type":"double","properties":{"unit":"rad"}}}]"#;
        let msgs = parse_text_frame(frame).unwrap();
        let TextMessage::Announce {
            name,
            id,
            ty,
            properties,
            pubuid,
        } = &msgs[0]
        else {
            panic!("expected announce, got {msgs:?}");
        };
        assert_eq!(name, "/Telemetry/Arm/angle");
        assert_eq!(*id, 7);
        assert_eq!(*ty, NtType::Double);
        assert_eq!(*pubuid, None);
        assert_eq!(properties["unit"], "rad");
    }

    #[test]
    fn setproperties_uses_lowercase_method() {
        let msg = TextMessage::SetProperties {
            name: "/a".into(),
            update: Map::new(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["method"], "setproperties");
    }
}
