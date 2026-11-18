//! Writing values as text: decimal places, whole numbers in decimal, hex or
//! binary, and enum fields by name or number. These settings belong to any
//! widget that shows a value as text (Text Display now; number widgets in
//! milestone 3), so their keys, readers and dialog rows live here together.

use eframe::egui;
use nt4::Value;
use serde_json::{Map, Value as Json};

use super::ValueInfo;
use super::props::set_property;
use crate::struct_view::EnumDisplay;
use crate::tree::format_number;

/// Show numbers with exactly this many decimal places (raptaboard).
pub const DECIMAL_PLACES: &str = "decimal_places";
/// Whole numbers as `"decimal"`, `"hex"` or `"binary"` (raptaboard).
pub const INT_FORMAT: &str = "int_format";
/// Write whole numbers with at least this many digits, padded with zeros
/// (`0x001A` for 4) (raptaboard).
pub const INT_DIGITS: &str = "int_digits";
/// Enum struct fields: `"name"`, `"number"` or `"both"` (raptaboard).
pub const ENUM_DISPLAY: &str = "enum_display";

/// Every setting key in this file, for a widget's
/// [`setting_keys`](super::WidgetType::setting_keys).
pub const KEYS: [&str; 4] = [DECIMAL_PLACES, INT_FORMAT, INT_DIGITS, ENUM_DISPLAY];

/// The most `decimal_places` used.
pub const MAX_DECIMAL_PLACES: usize = 10;
/// Decimal places offered when "Fixed" is first ticked.
const FIRST_DECIMAL_PLACES: usize = 2;
/// The most digits `int_digits` pads to (a 64-bit number in binary).
pub const MAX_INT_DIGITS: usize = 64;

/// How whole numbers are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IntFormat {
    /// `26`
    #[default]
    Decimal,
    /// `0x1A`
    Hex,
    /// `0b11010`
    Binary,
}

impl IntFormat {
    /// All choices, in the order a menu lists them.
    pub const ALL: [IntFormat; 3] = [IntFormat::Decimal, IntFormat::Hex, IntFormat::Binary];

    /// The word saved in a layout's `int_format` setting.
    pub fn key(self) -> &'static str {
        match self {
            IntFormat::Decimal => "decimal",
            IntFormat::Hex => "hex",
            IntFormat::Binary => "binary",
        }
    }

    /// The choice saved as `key`, if it's one of [`IntFormat::key`]'s words.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }

    /// Words for a menu.
    pub fn label(self) -> &'static str {
        match self {
            IntFormat::Decimal => "Decimal (26)",
            IntFormat::Hex => "Hex (0x1A)",
            IntFormat::Binary => "Binary (0b11010)",
        }
    }

    /// Write `n` this way, with at least `digits` digits (padded with
    /// zeros, like C's `%08X`; 0 for no padding). Negative numbers keep
    /// their minus sign: `-0x1A`.
    ///
    /// ```
    /// use raptaboard::widgets::format::IntFormat;
    /// assert_eq!(IntFormat::Decimal.format(26, 0), "26");
    /// assert_eq!(IntFormat::Hex.format(26, 0), "0x1A");
    /// assert_eq!(IntFormat::Hex.format(26, 8), "0x0000001A");
    /// assert_eq!(IntFormat::Binary.format(-5, 4), "-0b0101");
    /// ```
    pub fn format(self, n: i64, digits: usize) -> String {
        let sign = if n < 0 { "-" } else { "" };
        let m = n.unsigned_abs();
        match self {
            IntFormat::Decimal => format!("{sign}{m:0digits$}"),
            IntFormat::Hex => format!("{sign}0x{m:0digits$X}"),
            IntFormat::Binary => format!("{sign}0b{m:0digits$b}"),
        }
    }
}

/// How whole numbers are written: the `int_format` and `int_digits`
/// settings together.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IntStyle {
    /// Decimal, hex or binary.
    pub format: IntFormat,
    /// At least this many digits, padded with zeros; 0 for no padding.
    pub digits: usize,
}

impl IntStyle {
    /// Write `n` in this style.
    ///
    /// ```
    /// use raptaboard::widgets::format::{IntFormat, IntStyle};
    /// let style = IntStyle { format: IntFormat::Hex, digits: 2 };
    /// assert_eq!(style.write(10), "0x0A");
    /// assert_eq!(style.write(4096), "0x1000");
    /// ```
    pub fn write(self, n: i64) -> String {
        self.format.format(n, self.digits)
    }
}

/// What kind of value an NT type holds, for choosing which settings apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// `double`, `float` and their arrays: decimal places apply.
    Decimal,
    /// `int` and `int[]`: hex and binary apply.
    Whole,
    /// Strings, booleans and anything else: no number settings.
    Other,
}

impl ValueKind {
    /// The kind for an NT type string.
    ///
    /// ```
    /// use raptaboard::widgets::format::ValueKind;
    /// assert_eq!(ValueKind::of("double[]"), ValueKind::Decimal);
    /// assert_eq!(ValueKind::of("int"), ValueKind::Whole);
    /// assert_eq!(ValueKind::of("string"), ValueKind::Other);
    /// ```
    pub fn of(nt_type: &str) -> Self {
        match nt_type {
            "double" | "float" | "double[]" | "float[]" => ValueKind::Decimal,
            "int" | "int[]" => ValueKind::Whole,
            _ => ValueKind::Other,
        }
    }
}

/// The `decimal_places` setting: how many decimal places numbers are shown
/// with, at most [`MAX_DECIMAL_PLACES`]. `None` if not set, for up to 4
/// places without trailing zeros.
///
/// ```
/// use raptaboard::widgets::format::decimal_places;
/// let props = serde_json::json!({"decimal_places": 2});
/// assert_eq!(decimal_places(props.as_object().unwrap()), Some(2));
/// assert_eq!(decimal_places(&serde_json::Map::new()), None);
/// ```
pub fn decimal_places(props: &Map<String, Json>) -> Option<usize> {
    let n = props.get(DECIMAL_PLACES)?.as_u64()?;
    Some((n as usize).min(MAX_DECIMAL_PLACES))
}

/// The `int_format` setting; [`IntFormat::Decimal`] if not set.
///
/// ```
/// use raptaboard::widgets::format::{int_format, IntFormat};
/// let props = serde_json::json!({"int_format": "hex"});
/// assert_eq!(int_format(props.as_object().unwrap()), IntFormat::Hex);
/// ```
pub fn int_format(props: &Map<String, Json>) -> IntFormat {
    props
        .get(INT_FORMAT)
        .and_then(Json::as_str)
        .and_then(IntFormat::from_key)
        .unwrap_or_default()
}

/// The `int_digits` setting, at most [`MAX_INT_DIGITS`]; 0 if not set.
pub fn int_digits(props: &Map<String, Json>) -> usize {
    props
        .get(INT_DIGITS)
        .and_then(Json::as_u64)
        .map_or(0, |n| (n as usize).min(MAX_INT_DIGITS))
}

/// The `int_format` and `int_digits` settings together.
///
/// ```
/// use raptaboard::widgets::format::{int_style, IntFormat};
/// let props = serde_json::json!({"int_format": "hex", "int_digits": 8});
/// let style = int_style(props.as_object().unwrap());
/// assert_eq!((style.format, style.digits), (IntFormat::Hex, 8));
/// ```
pub fn int_style(props: &Map<String, Json>) -> IntStyle {
    IntStyle {
        format: int_format(props),
        digits: int_digits(props),
    }
}

/// The `enum_display` setting; [`EnumDisplay::Name`] if not set.
///
/// ```
/// use raptaboard::struct_view::EnumDisplay;
/// use raptaboard::widgets::format::enum_display;
/// let props = serde_json::json!({"enum_display": "both"});
/// assert_eq!(enum_display(props.as_object().unwrap()), EnumDisplay::Both);
/// ```
pub fn enum_display(props: &Map<String, Json>) -> EnumDisplay {
    props
        .get(ENUM_DISPLAY)
        .and_then(Json::as_str)
        .and_then(EnumDisplay::from_key)
        .unwrap_or_default()
}

/// A value as text, with the number settings in `props`: strings without
/// quotes, arrays as `[a, b, c]`, whole numbers as `int_format` and
/// `int_digits` say, and other numbers with exactly `decimal_places`
/// places, or up to 4 without trailing zeros if that isn't set. Raw bytes
/// (such as struct values) have no text form.
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::format::value_text;
/// let props = serde_json::json!({"decimal_places": 2, "int_format": "hex"});
/// let props = props.as_object().unwrap();
/// assert_eq!(value_text(&Value::Double(1.0), props).unwrap(), "1.00");
/// assert_eq!(value_text(&Value::Int(255), props).unwrap(), "0xFF");
/// assert_eq!(value_text(&Value::Raw(vec![1, 2]), props), None);
/// ```
pub fn value_text(value: &Value, props: &Map<String, Json>) -> Option<String> {
    fn list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
        format!("[{}]", items.iter().map(one).collect::<Vec<_>>().join(", "))
    }
    let decimals = decimal_places(props);
    let ints = int_style(props);
    let number = |n: f64| match decimals {
        Some(places) if n.is_finite() => format!("{n:.places$}"),
        _ => format_number(n),
    };
    Some(match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(d) => number(*d),
        Value::Float(f) => number(f64::from(*f)),
        Value::Int(i) => ints.write(*i),
        Value::String(s) => s.clone(),
        Value::BooleanArray(a) => list(a, |b| b.to_string()),
        Value::DoubleArray(a) => list(a, |d| number(*d)),
        Value::FloatArray(a) => list(a, |f| number(f64::from(*f))),
        Value::IntArray(a) => list(a, |i| ints.write(*i)),
        Value::StringArray(a) => list(a, |s| s.clone()),
        Value::Raw(_) => return None,
    })
}

/// The settings dialog rows for these settings, offered by what the value
/// is: "Enum shows" for an enum field, "Whole numbers" and "Digits" for
/// ints (and enums shown as numbers), and "Decimal places" for doubles and
/// floats. If the type isn't known, both kinds of number row show.
pub fn rows(ui: &mut egui::Ui, props: &mut Map<String, Json>, value: ValueInfo<'_>) {
    let kind = value.nt_type.map(ValueKind::of);
    let is_enum = value.is_enum();
    let decimals = matches!(kind, None | Some(ValueKind::Decimal));
    // An enum shown by name has no number to format.
    let whole = matches!(kind, None | Some(ValueKind::Whole))
        && !(is_enum && enum_display(props) == EnumDisplay::Name);

    if is_enum {
        ui.label("Enum shows");
        let mut display = enum_display(props);
        egui::ComboBox::from_id_salt("enum_display")
            .selected_text(display.label())
            .show_ui(ui, |ui| {
                for choice in EnumDisplay::ALL {
                    ui.selectable_value(&mut display, choice, choice.label());
                }
            });
        if display != enum_display(props) {
            let value = (display != EnumDisplay::default()).then(|| display.key().into());
            set_property(props, ENUM_DISPLAY, value);
        }
        ui.end_row();
    }

    if whole {
        ui.label("Whole numbers");
        let mut format = int_format(props);
        egui::ComboBox::from_id_salt("int_format")
            .selected_text(format.label())
            .show_ui(ui, |ui| {
                for choice in IntFormat::ALL {
                    ui.selectable_value(&mut format, choice, choice.label());
                }
            });
        if format != int_format(props) {
            let value = (format != IntFormat::default()).then(|| format.key().into());
            set_property(props, INT_FORMAT, value);
        }
        ui.end_row();

        ui.label("Digits");
        ui.horizontal(|ui| {
            let saved = int_digits(props);
            let mut padded = saved > 0;
            let mut digits = if padded { saved } else { 2 };
            let ticked = ui
                .checkbox(&mut padded, "Pad with zeros")
                .on_hover_text("Show at least this many digits, like %08X: 0x0000001A.")
                .changed();
            let moved = padded
                && ui
                    .add(egui::DragValue::new(&mut digits).range(1..=MAX_INT_DIGITS))
                    .changed();
            if ticked || moved {
                set_property(props, INT_DIGITS, padded.then(|| digits.into()));
            }
        });
        ui.end_row();
    }

    if decimals {
        ui.label("Decimal places");
        ui.horizontal(|ui| {
            let saved = decimal_places(props);
            let mut fixed = saved.is_some();
            let mut places = saved.unwrap_or(FIRST_DECIMAL_PLACES);
            let ticked = ui
                .checkbox(&mut fixed, "Fixed")
                .on_hover_text(
                    "Always show this many decimal places. Off: up to 4, without trailing zeros.",
                )
                .changed();
            let moved = fixed
                && ui
                    .add(egui::DragValue::new(&mut places).range(0..=MAX_DECIMAL_PLACES))
                    .changed();
            if ticked || moved {
                set_property(props, DECIMAL_PLACES, fixed.then(|| places.into()));
            }
        });
        ui.end_row();
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn props(json: Json) -> Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    fn text(value: Value, settings: &Map<String, Json>) -> String {
        value_text(&value, settings).unwrap()
    }

    #[test]
    fn each_type_as_text() {
        let none = Map::new();
        assert_eq!(text(Value::String("Blue 1".into()), &none), "Blue 1");
        assert_eq!(text(Value::Double(1.0 / 3.0), &none), "0.3333");
        assert_eq!(text(Value::Int(-4), &none), "-4");
        assert_eq!(text(Value::Boolean(false), &none), "false");
        assert_eq!(text(Value::DoubleArray(vec![1.0, 2.5]), &none), "[1, 2.5]");
        assert_eq!(value_text(&Value::Raw(vec![0; 8]), &none), None);
    }

    #[test]
    fn fixed_decimal_places() {
        let two = props(serde_json::json!({"decimal_places": 2}));
        // Fixed places keep trailing zeros, so the number doesn't jump around.
        assert_eq!(text(Value::Double(1.0), &two), "1.00");
        assert_eq!(text(Value::Float(-0.125), &two), "-0.12");
        assert_eq!(
            text(Value::DoubleArray(vec![1.0, 2.555]), &two),
            "[1.00, 2.56]"
        );
        // Whole numbers and non-numbers aren't changed.
        assert_eq!(text(Value::Int(7), &two), "7");
        assert_eq!(text(Value::Double(f64::NAN), &two), "NaN");
        let zero = props(serde_json::json!({"decimal_places": 0}));
        assert_eq!(text(Value::Double(2.6), &zero), "3");
        // Out of range settings are kept in range; bad ones are ignored.
        let bad = props(serde_json::json!({"decimal_places": -1, "enum_display": "loud"}));
        assert_eq!(decimal_places(&bad), None);
        assert_eq!(enum_display(&bad), EnumDisplay::Name);
        let many = props(serde_json::json!({"decimal_places": 99}));
        assert_eq!(decimal_places(&many), Some(MAX_DECIMAL_PLACES));
    }

    #[test]
    fn whole_numbers_in_hex_and_binary() {
        let hex = props(serde_json::json!({"int_format": "hex", "decimal_places": 1}));
        assert_eq!(text(Value::Int(255), &hex), "0xFF");
        assert_eq!(text(Value::IntArray(vec![10, -1]), &hex), "[0xA, -0x1]");
        // Fractional numbers ignore it and keep their decimal places.
        assert_eq!(text(Value::Double(10.0), &hex), "10.0");
        let bin = props(serde_json::json!({"int_format": "binary"}));
        assert_eq!(text(Value::Int(5), &bin), "0b101");
        assert_eq!(IntFormat::Hex.format(i64::MIN, 0), "-0x8000000000000000");
        // Padding, like %08X; numbers wider than it aren't cut.
        let padded = props(serde_json::json!({"int_format": "hex", "int_digits": 4}));
        assert_eq!(
            text(Value::IntArray(vec![26, 0x12345]), &padded),
            "[0x001A, 0x12345]"
        );
        let decimal = props(serde_json::json!({"int_digits": 3}));
        assert_eq!(text(Value::Int(-7), &decimal), "-007");
        let huge = props(serde_json::json!({"int_digits": 1000}));
        assert_eq!(int_digits(&huge), MAX_INT_DIGITS);
        let bad = props(serde_json::json!({"int_format": 16}));
        assert_eq!(int_format(&bad), IntFormat::Decimal);
    }
}
