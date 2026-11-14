//! NT4 binary value updates.
//!
//! A binary websocket frame holds one or more MessagePack arrays back to back:
//! `[topic_id, timestamp_us, type_id, value]`. Topic id `-1` is the server's
//! reply to a client time ping.

use std::io::Cursor;

/// A decoded topic value.
#[derive(Debug, Clone, PartialEq)]
pub enum NtValue {
    Boolean(bool),
    Double(f64),
    Int(i64),
    Float(f32),
    String(String),
    /// Raw bytes: raw, rpc, msgpack, protobuf and struct topics.
    Raw(Vec<u8>),
    BooleanArray(Vec<bool>),
    DoubleArray(Vec<f64>),
    IntArray(Vec<i64>),
    FloatArray(Vec<f32>),
    StringArray(Vec<String>),
}

impl NtValue {
    /// The value as an `f64` if it is a number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            NtValue::Double(v) => Some(*v),
            NtValue::Float(v) => Some(f64::from(*v)),
            NtValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
}

/// One value update from a binary frame.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryMessage {
    /// Topic id from `announce`, or -1 for a time sync reply.
    pub topic_id: i64,
    /// Server time in microseconds.
    pub timestamp_us: i64,
    pub value: NtValue,
}

/// Why a binary frame could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("msgpack: {0}")]
    MsgPack(#[from] rmpv::decode::Error),
    #[error("message is not a 4 element array")]
    Shape,
    #[error("unknown type id {0}")]
    UnknownType(u64),
    #[error("value does not match type id {0}")]
    ValueMismatch(u64),
}

/// Decode every value update in one binary frame.
///
/// ```
/// use raptaboard_nt4::{decode_binary_frame, encode_binary_message, BinaryMessage, NtValue};
/// let msg = BinaryMessage { topic_id: 3, timestamp_us: 1000, value: NtValue::Double(1.5) };
/// let bytes = encode_binary_message(&msg);
/// assert_eq!(decode_binary_frame(&bytes).unwrap(), vec![msg]);
/// ```
pub fn decode_binary_frame(bytes: &[u8]) -> Result<Vec<BinaryMessage>, DecodeError> {
    let mut cursor = Cursor::new(bytes);
    let mut out = Vec::new();
    while (cursor.position() as usize) < bytes.len() {
        let item = rmpv::decode::read_value(&mut cursor)?;
        out.push(decode_message(item)?);
    }
    Ok(out)
}

fn decode_message(item: rmpv::Value) -> Result<BinaryMessage, DecodeError> {
    let rmpv::Value::Array(parts) = item else {
        return Err(DecodeError::Shape);
    };
    let [id, time, ty, value]: [rmpv::Value; 4] =
        parts.try_into().map_err(|_| DecodeError::Shape)?;
    let topic_id = id.as_i64().ok_or(DecodeError::Shape)?;
    let timestamp_us = time.as_i64().ok_or(DecodeError::Shape)?;
    let type_id = ty.as_u64().ok_or(DecodeError::Shape)?;
    let value = decode_value(type_id, value)?;
    Ok(BinaryMessage {
        topic_id,
        timestamp_us,
        value,
    })
}

fn decode_value(type_id: u64, v: rmpv::Value) -> Result<NtValue, DecodeError> {
    use rmpv::Value as V;
    let bad = || DecodeError::ValueMismatch(type_id);
    let array = |v: V| match v {
        V::Array(a) => Ok(a),
        _ => Err(bad()),
    };
    Ok(match type_id {
        0 => NtValue::Boolean(v.as_bool().ok_or_else(bad)?),
        1 => NtValue::Double(number(&v).ok_or_else(bad)?),
        2 => NtValue::Int(v.as_i64().ok_or_else(bad)?),
        3 => NtValue::Float(number(&v).ok_or_else(bad)? as f32),
        4 => NtValue::String(v.as_str().ok_or_else(bad)?.to_owned()),
        5 => NtValue::Raw(match v {
            V::Binary(b) => b,
            _ => return Err(bad()),
        }),
        16 => NtValue::BooleanArray(
            array(v)?
                .iter()
                .map(|x| x.as_bool().ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        ),
        17 => NtValue::DoubleArray(
            array(v)?
                .iter()
                .map(|x| number(x).ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        ),
        18 => NtValue::IntArray(
            array(v)?
                .iter()
                .map(|x| x.as_i64().ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        ),
        19 => NtValue::FloatArray(
            array(v)?
                .iter()
                .map(|x| number(x).map(|f| f as f32).ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        ),
        20 => NtValue::StringArray(
            array(v)?
                .iter()
                .map(|x| x.as_str().map(str::to_owned).ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        ),
        other => return Err(DecodeError::UnknownType(other)),
    })
}

/// Read a msgpack number as f64; some servers pack whole-number doubles as ints.
fn number(v: &rmpv::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|u| u as f64))
}

/// Encode one value update, for publishing or tests.
pub fn encode_binary_message(msg: &BinaryMessage) -> Vec<u8> {
    use rmpv::Value as V;
    let (type_id, value) = match &msg.value {
        NtValue::Boolean(b) => (0, V::from(*b)),
        NtValue::Double(d) => (1, V::from(*d)),
        NtValue::Int(i) => (2, V::from(*i)),
        NtValue::Float(f) => (3, V::from(*f)),
        NtValue::String(s) => (4, V::from(s.as_str())),
        NtValue::Raw(b) => (5, V::Binary(b.clone())),
        NtValue::BooleanArray(a) => (16, V::Array(a.iter().map(|&x| V::from(x)).collect())),
        NtValue::DoubleArray(a) => (17, V::Array(a.iter().map(|&x| V::from(x)).collect())),
        NtValue::IntArray(a) => (18, V::Array(a.iter().map(|&x| V::from(x)).collect())),
        NtValue::FloatArray(a) => (19, V::Array(a.iter().map(|&x| V::from(x)).collect())),
        NtValue::StringArray(a) => (
            20,
            V::Array(a.iter().map(|x| V::from(x.as_str())).collect()),
        ),
    };
    let item = V::Array(vec![
        V::from(msg.topic_id),
        V::from(msg.timestamp_us),
        V::from(type_id),
        value,
    ]);
    let mut out = Vec::new();
    rmpv::encode::write_value(&mut out, &item).expect("writing to a Vec cannot fail");
    out
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn decodes_several_messages_in_one_frame() {
        let msgs = vec![
            BinaryMessage {
                topic_id: 1,
                timestamp_us: 10,
                value: NtValue::Boolean(true),
            },
            BinaryMessage {
                topic_id: 2,
                timestamp_us: 20,
                value: NtValue::StringArray(vec!["a".into()]),
            },
            BinaryMessage {
                topic_id: 3,
                timestamp_us: 30,
                value: NtValue::Raw(vec![1, 2, 3]),
            },
            BinaryMessage {
                topic_id: -1,
                timestamp_us: 40,
                value: NtValue::Int(123_456),
            },
        ];
        let mut frame = Vec::new();
        for m in &msgs {
            frame.extend(encode_binary_message(m));
        }
        assert_eq!(decode_binary_frame(&frame).unwrap(), msgs);
    }

    #[test]
    fn integer_encoded_double_still_decodes() {
        // Servers may pack a whole-number double as a msgpack int.
        let item = rmpv::Value::Array(vec![1.into(), 0.into(), 1.into(), 2.into()]);
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &item).unwrap();
        assert_eq!(
            decode_binary_frame(&bytes).unwrap()[0].value,
            NtValue::Double(2.0)
        );
    }

    #[test]
    fn wrong_shape_is_an_error() {
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &rmpv::Value::from(5)).unwrap();
        assert!(matches!(
            decode_binary_frame(&bytes),
            Err(DecodeError::Shape)
        ));
    }
}
