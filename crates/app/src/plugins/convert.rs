//! Moving values between the dashboard and a plugin: NT values, the layout's
//! settings (JSON) and the theme, in the plugin interface's types.

use eframe::egui::{self, Color32};
use nt4::Value;
use raptaboard_plugin_host::PropertySpec;
use raptaboard_plugin_host::runtime::{Color, NtValue, Theme};
use serde_json::{Map, Value as Json};

use crate::types::alerts::Level;
use crate::widgets::shared::alert_sign;

/// An NT value as a plugin gets it, or `None` for raw bytes, which plugins
/// don't read (structs come decoded in a later version).
///
/// ```
/// use raptaboard::plugins::convert::to_plugin;
/// use raptaboard_plugin_host::runtime::NtValue;
/// assert_eq!(to_plugin(&nt4::Value::Float(1.5)), Some(NtValue::Double(1.5)));
/// assert_eq!(to_plugin(&nt4::Value::Raw(vec![1])), None);
/// ```
pub fn to_plugin(value: &Value) -> Option<NtValue> {
    Some(match value {
        Value::Boolean(b) => NtValue::Boolean(*b),
        Value::Double(d) => NtValue::Double(*d),
        Value::Float(f) => NtValue::Double(f64::from(*f)),
        Value::Int(i) => NtValue::Int(*i),
        Value::String(s) => NtValue::Str(s.clone()),
        Value::BooleanArray(a) => NtValue::BoolArray(a.clone()),
        Value::DoubleArray(a) => NtValue::DoubleArray(a.clone()),
        Value::FloatArray(a) => NtValue::DoubleArray(a.iter().map(|f| f64::from(*f)).collect()),
        Value::IntArray(a) => NtValue::IntArray(a.clone()),
        Value::StringArray(a) => NtValue::StrArray(a.clone()),
        _ => return None,
    })
}

/// A value a plugin publishes, as an NT value and the type to publish it as,
/// or `None` for a struct record (not publishable yet).
pub fn from_plugin(value: &NtValue) -> Option<(Value, nt4::NtType)> {
    use nt4::NtType as T;
    Some(match value {
        NtValue::Boolean(b) => (Value::Boolean(*b), T::Boolean),
        NtValue::Double(d) => (Value::Double(*d), T::Double),
        NtValue::Int(i) => (Value::Int(*i), T::Int),
        NtValue::Str(s) => (Value::String(s.clone()), T::String),
        NtValue::BoolArray(a) => (Value::BooleanArray(a.clone()), T::BooleanArray),
        NtValue::DoubleArray(a) => (Value::DoubleArray(a.clone()), T::DoubleArray),
        NtValue::IntArray(a) => (Value::IntArray(a.clone()), T::IntArray),
        NtValue::StrArray(a) => (Value::StringArray(a.clone()), T::StringArray),
        NtValue::Record(_) => return None,
    })
}

/// A widget's property values for the plugin: each one plugin.toml declares,
/// from the layout's settings `props` when set there (and of the right
/// kind), else its default.
///
/// ```
/// use raptaboard::plugins::convert::properties;
/// use raptaboard_plugin_host::{Manifest, runtime::NtValue};
/// let m = Manifest::from_toml("name='n'\nversion='0'\napi='0.2'\n[match]\ntopic_types=['double']\n\
///     [properties]\ndecimal_places={type='number',default=-1}\nfit_text={type='bool',default=false}\n").unwrap();
/// let props = serde_json::json!({"decimal_places": 2, "fit_text": "yes"});
/// let values = properties(&m.properties, props.as_object().unwrap());
/// assert_eq!(values["decimal_places"], NtValue::Double(2.0));
/// // Not a true/false: the default.
/// assert_eq!(values["fit_text"], NtValue::Boolean(false));
/// ```
pub fn properties(
    specs: &std::collections::BTreeMap<String, PropertySpec>,
    props: &Map<String, Json>,
) -> std::collections::HashMap<String, NtValue> {
    specs
        .iter()
        .map(|(name, spec)| {
            let set = props.get(name);
            let value = match spec {
                PropertySpec::Number { default } => {
                    NtValue::Double(set.and_then(Json::as_f64).unwrap_or(*default))
                }
                PropertySpec::Bool { default } => {
                    NtValue::Boolean(set.and_then(Json::as_bool).unwrap_or(*default))
                }
                PropertySpec::String { default }
                | PropertySpec::Color { default }
                | PropertySpec::Sound { default } => {
                    NtValue::Str(set.and_then(Json::as_str).unwrap_or(default).to_string())
                }
                PropertySpec::Enum { default, options } => NtValue::Str(
                    set.and_then(Json::as_str)
                        .filter(|s| options.iter().any(|o| o == s))
                        .unwrap_or(default)
                        .to_string(),
                ),
            };
            (name.clone(), value)
        })
        .collect()
}

/// An egui color in the plugin interface's terms.
pub fn color(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color { r, g, b, a }
}

/// A plugin's color for egui.
pub fn egui_color(c: &Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The dashboard's colors now, for a plugin to draw with.
pub fn theme(visuals: &egui::Visuals) -> Theme {
    Theme {
        dark: visuals.dark_mode,
        text: color(visuals.text_color()),
        weak_text: color(visuals.weak_text_color()),
        background: color(visuals.panel_fill),
        card: color(visuals.extreme_bg_color),
        accent: color(visuals.selection.bg_fill),
        error: color(alert_sign::color(Level::Error)),
        warning: color(alert_sign::color(Level::Warning)),
        info: color(alert_sign::color(Level::Info)),
    }
}

/// A plugin's tab mark level as the dashboard's.
pub fn level(level: raptaboard_plugin_host::runtime::Level) -> Level {
    use raptaboard_plugin_host::runtime::Level as L;
    match level {
        L::Error => Level::Error,
        L::Warning => Level::Warning,
        L::Info => Level::Info,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn values_go_to_a_plugin_and_back() {
        for v in [
            Value::Boolean(true),
            Value::Double(1.5),
            Value::Int(-3),
            Value::String("hi".into()),
            Value::StringArray(vec!["a".into()]),
            Value::DoubleArray(vec![1.0, 2.0]),
        ] {
            let there = to_plugin(&v).unwrap();
            assert_eq!(from_plugin(&there).unwrap().0, v);
        }
        assert_eq!(
            to_plugin(&Value::FloatArray(vec![0.5])),
            Some(NtValue::DoubleArray(vec![0.5]))
        );
        assert!(from_plugin(&NtValue::Record(vec![])).is_none());
    }

    #[test]
    fn an_enum_property_keeps_only_its_options() {
        let m = raptaboard_plugin_host::Manifest::from_toml(
            "name='n'\nversion='0'\napi='0.2'\n[match]\ntopic_types=['double']\n\
             [properties]\nmode={type='enum',default='a',options=['a','b']}\n",
        )
        .unwrap();
        let pick = |v: Json| {
            let props: Map<String, Json> = [("mode".to_string(), v)].into_iter().collect();
            properties(&m.properties, &props)["mode"].clone()
        };
        assert_eq!(pick("b".into()), NtValue::Str("b".into()));
        assert_eq!(pick("z".into()), NtValue::Str("a".into()));
    }

    #[test]
    fn colors_round_trip() {
        let c = Color32::from_rgba_unmultiplied(10, 20, 30, 200);
        assert_eq!(egui_color(&color(c)), c);
    }
}
