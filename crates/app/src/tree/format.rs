//! A value in a few words, as the tree shows it.

use nt4::Value;

use crate::types::format_number;

/// Most array items shown before the rest are summarized.
const MAX_ITEMS: usize = 6;

/// Longest string shown before it is cut off, in characters.
const MAX_CHARS: usize = 40;

/// A short one-line form of a value for the tree.
///
/// Numbers keep up to 4 decimal places, long strings and arrays are cut
/// short, and raw bytes show their length. Struct values are opened into
/// fields by the tree instead (see [`crate::types::structs`]).
///
/// ```
/// use raptaboard::tree::format_value;
/// use nt4::Value;
/// assert_eq!(format_value(&Value::Double(0.5)), "0.5");
/// assert_eq!(format_value(&Value::Double(1.0 / 3.0)), "0.3333");
/// assert_eq!(format_value(&Value::BooleanArray(vec![true, false])), "[true, false]");
/// assert_eq!(format_value(&Value::Raw(vec![0; 24])), "24 bytes");
/// ```
pub fn format_value(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(d) => format_number(*d),
        Value::Float(f) => format_number(f64::from(*f)),
        Value::Int(i) => i.to_string(),
        Value::String(s) => format!("\"{}\"", shorten(s)),
        Value::Raw(bytes) => format!("{} bytes", bytes.len()),
        Value::BooleanArray(a) => format_list(a, |b| b.to_string()),
        Value::DoubleArray(a) => format_list(a, |d| format_number(*d)),
        Value::FloatArray(a) => format_list(a, |f| format_number(f64::from(*f))),
        Value::IntArray(a) => format_list(a, |i| i.to_string()),
        Value::StringArray(a) => format_list(a, |s| format!("\"{}\"", shorten(s))),
    }
}

pub(super) fn shorten(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(MAX_CHARS).collect();
        format!("{cut}…")
    }
}

fn format_list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
    let shown: Vec<String> = items.iter().take(MAX_ITEMS).map(one).collect();
    let more = items.len().saturating_sub(MAX_ITEMS);
    if more == 0 {
        format!("[{}]", shown.join(", "))
    } else {
        format!("[{}, … +{more}]", shown.join(", "))
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn values_are_short() {
        assert_eq!(format_value(&Value::Double(2.0)), "2");
        assert_eq!(format_value(&Value::Double(-0.00001)), "0");
        assert_eq!(format_value(&Value::Float(1.25)), "1.25");
        assert_eq!(format_value(&Value::Int(-3)), "-3");
        assert_eq!(format_value(&Value::Boolean(true)), "true");
        assert_eq!(format_value(&Value::String("hi".into())), "\"hi\"");
        let long = "x".repeat(50);
        assert_eq!(
            format_value(&Value::String(long)),
            format!("\"{}…\"", "x".repeat(40))
        );
        assert_eq!(
            format_value(&Value::IntArray((1..=8).collect())),
            "[1, 2, 3, 4, 5, 6, … +2]"
        );
        assert_eq!(format_value(&Value::DoubleArray(vec![])), "[]");
    }
}
