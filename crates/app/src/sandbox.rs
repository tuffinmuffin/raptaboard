//! The plugin sandbox (`raptaboard plugin dev <folder>`): the dashboard with
//! one plugin widget and a NetworkTables server of its own, so a plugin can be
//! tried without a robot or the sim.
//!
//! [`Sandbox`] runs the server (on this computer only, a free port) and the
//! topics it serves: the topic or table the widget is bound to, with the
//! plugin's sample values (`[sandbox]` in its `plugin.toml`), and any the
//! user adds. The window's side panel edits them (`ui/sandbox_panel.rs`).
//! Without a server of its own (`--address`), the widget is bound to a topic
//! on the robot or sim instead and there are no values to edit.

use std::net::{IpAddr, Ipv4Addr};

use nt4::{GenericPublisher, NtType, Value};
use raptaboard_layout::{ElasticLayout, Rect as LayoutRect, Tab, WidgetContainer};
use raptaboard_plugin_host::Manifest;
use serde_json::{Map, json};

/// Where the widget's topic or table is on the sandbox's server.
pub const ROOT: &str = "/Sandbox";

/// Widget sizes to try, in grid cells (width, height).
pub const SIZES: [(u32, u32); 6] = [(1, 1), (2, 1), (2, 2), (3, 2), (4, 2), (4, 3)];

/// The value types a sandbox topic can have, for adding one.
pub const TYPES: [NtType; 7] = [
    NtType::Double,
    NtType::Int,
    NtType::Boolean,
    NtType::String,
    NtType::DoubleArray,
    NtType::BooleanArray,
    NtType::StringArray,
];

/// A topic the sandbox serves.
pub struct Topic {
    /// Its full name, like `/Sandbox/Alerts/errors`.
    pub name: String,
    /// Its type.
    pub ty: NtType,
    /// Its value now.
    pub value: Value,
    /// The text being typed for it (strings and lists), kept while it's
    /// edited so half-typed text isn't reformatted.
    pub text: String,
    publisher: GenericPublisher,
}

/// The NT server, when the sandbox has its own.
struct Served {
    /// Runs the server's tasks.
    _runtime: tokio::runtime::Runtime,
    server: nt4::Server,
}

/// The sandbox: what the widget is bound to, and the server and its topics.
pub struct Sandbox {
    /// The plugin's widget type, `plugin:<name>`.
    pub type_name: String,
    /// The topic or table the widget is bound to.
    pub bound: String,
    /// The topic's type, for a topic plugin (the widget's `data_type`).
    pub bound_type: Option<String>,
    /// The widget's size, in grid cells.
    pub cells: (u32, u32),
    /// The server's port, when the sandbox has its own server.
    pub port: Option<u16>,
    /// The topics served, by name.
    pub topics: Vec<Topic>,
    served: Option<Served>,
}

/// A TOML value from `[sandbox]` as an NT value: a float a double, an
/// integer an int, text a string, a list a list of one of those (an empty
/// list a string list); `None` for anything else.
///
/// ```
/// use raptaboard::sandbox::nt_value;
/// use nt4::Value;
/// assert_eq!(nt_value(&toml::Value::Float(1.5)), Some(Value::Double(1.5)));
/// let list = toml::Value::Array(vec![toml::Value::String("a".into())]);
/// assert_eq!(nt_value(&list), Some(Value::StringArray(vec!["a".into()])));
/// ```
pub fn nt_value(v: &toml::Value) -> Option<Value> {
    use toml::Value as T;
    Some(match v {
        T::Float(f) => Value::Double(*f),
        T::Integer(i) => Value::Int(*i),
        T::Boolean(b) => Value::Boolean(*b),
        T::String(s) => Value::String(s.clone()),
        T::Array(items) if items.iter().all(|i| matches!(i, T::String(_))) => Value::StringArray(
            items
                .iter()
                .filter_map(|i| i.as_str().map(String::from))
                .collect(),
        ),
        T::Array(items) if items.iter().all(|i| matches!(i, T::Integer(_))) => {
            Value::IntArray(items.iter().filter_map(T::as_integer).collect())
        }
        T::Array(items)
            if items
                .iter()
                .all(|i| matches!(i, T::Integer(_) | T::Float(_))) =>
        {
            Value::DoubleArray(
                items
                    .iter()
                    .filter_map(|i| i.as_float().or(i.as_integer().map(|n| n as f64)))
                    .collect(),
            )
        }
        T::Array(items) if items.iter().all(|i| matches!(i, T::Boolean(_))) => {
            Value::BooleanArray(items.iter().filter_map(T::as_bool).collect())
        }
        _ => return None,
    })
}

/// A starting value of type `ty`: zero, false, empty.
pub fn zero(ty: &NtType) -> Option<Value> {
    Some(match ty {
        NtType::Double => Value::Double(0.0),
        NtType::Float => Value::Float(0.0),
        NtType::Int => Value::Int(0),
        NtType::Boolean => Value::Boolean(false),
        NtType::String => Value::String(String::new()),
        NtType::DoubleArray => Value::DoubleArray(Vec::new()),
        NtType::FloatArray => Value::FloatArray(Vec::new()),
        NtType::IntArray => Value::IntArray(Vec::new()),
        NtType::BooleanArray => Value::BooleanArray(Vec::new()),
        NtType::StringArray => Value::StringArray(Vec::new()),
        _ => return None,
    })
}

/// A value as the text it's edited as: a string itself, a list one item a
/// line (strings) or comma-separated (numbers, true/false).
///
/// ```
/// use raptaboard::sandbox::{as_text, from_text};
/// use nt4::{NtType, Value};
/// let v = Value::DoubleArray(vec![1.0, 2.5]);
/// assert_eq!(as_text(&v), "1, 2.5");
/// assert_eq!(from_text(&NtType::DoubleArray, "1, 2.5"), Some(v));
/// assert_eq!(from_text(&NtType::DoubleArray, "1, x"), None);
/// ```
pub fn as_text(v: &Value) -> String {
    let join = |items: Vec<String>| items.join(", ");
    match v {
        Value::String(s) => s.clone(),
        Value::StringArray(a) => a.join("\n"),
        Value::DoubleArray(a) => join(a.iter().map(f64::to_string).collect()),
        Value::FloatArray(a) => join(a.iter().map(f32::to_string).collect()),
        Value::IntArray(a) => join(a.iter().map(i64::to_string).collect()),
        Value::BooleanArray(a) => join(a.iter().map(bool::to_string).collect()),
        other => format!("{other:?}"),
    }
}

/// The value of type `ty` that `text` (as [`as_text`] writes it) gives, or
/// `None` if it isn't one.
pub fn from_text(ty: &NtType, text: &str) -> Option<Value> {
    let items = || {
        text.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
    };
    Some(match ty {
        NtType::String => Value::String(text.to_string()),
        NtType::StringArray => Value::StringArray(
            text.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
        ),
        NtType::DoubleArray => Value::DoubleArray(
            items()
                .iter()
                .map(|s| s.parse().ok())
                .collect::<Option<_>>()?,
        ),
        NtType::IntArray => Value::IntArray(
            items()
                .iter()
                .map(|s| s.parse().ok())
                .collect::<Option<_>>()?,
        ),
        NtType::BooleanArray => Value::BooleanArray(
            items()
                .iter()
                .map(|s| s.parse().ok())
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

/// The bound topic's type for a topic plugin: its sample value's, if
/// `topic_types` lists it, else the first type listed.
fn topic_type(manifest: &Manifest, sample: Option<&Value>) -> NtType {
    let listed: Vec<NtType> = manifest
        .match_rules
        .topic_types
        .iter()
        .filter_map(|t| t.parse().ok())
        .collect();
    if let Some(ty) = sample.map(Value::default_type)
        && listed.contains(&ty)
    {
        return ty;
    }
    listed.into_iter().next().unwrap_or(NtType::Double)
}

impl Sandbox {
    /// A sandbox for `manifest`'s plugin with a server of its own, serving
    /// the plugin's sample values.
    pub fn start(manifest: &Manifest) -> std::io::Result<Sandbox> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("raptaboard-sandbox")
            .enable_all()
            .build()?;
        let server = runtime.block_on(nt4::Server::start(nt4::ServerConfig {
            listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            persist_file: None,
        }))?;
        let port = server.local_addr().port();
        let mut sandbox = Sandbox::bound_to(manifest, None);
        sandbox.port = Some(port);
        sandbox.served = Some(Served {
            _runtime: runtime,
            server,
        });
        let sample = &manifest.sandbox;
        if manifest.shows_topics() {
            let value = sample.value.as_ref().and_then(nt_value);
            let ty = topic_type(manifest, value.as_ref());
            let value = value
                .filter(|v| v.default_type() == ty)
                .or_else(|| zero(&ty))
                .unwrap_or(Value::Double(0.0));
            let bound = sandbox.bound.clone();
            sandbox.add_value(&bound, ty, value);
        } else {
            if let Some(ty) = &manifest.match_rules.type_tag {
                let name = format!("{}/.type", sandbox.bound);
                sandbox.add_value(&name, NtType::String, Value::String(ty.clone()));
            }
            for (key, v) in &sample.values {
                if let Some(value) = nt_value(v) {
                    let name = format!("{}/{key}", sandbox.bound);
                    sandbox.add_value(&name, value.default_type(), value);
                }
            }
            // Keys the plugin needs, with no sample: zero.
            for key in &manifest.match_rules.has {
                let name = format!("{}/{key}", sandbox.bound);
                if sandbox.topic(&name).is_none() {
                    sandbox.add_value(&name, NtType::Double, Value::Double(0.0));
                }
            }
        }
        Ok(sandbox)
    }

    /// A sandbox for `manifest`'s plugin with no server of its own: the
    /// widget bound to `topic` (on the robot or sim), or to where the
    /// sandbox's own server would have it.
    pub fn bound_to(manifest: &Manifest, topic: Option<&str>) -> Sandbox {
        let bound = topic.map(String::from).unwrap_or_else(|| {
            if manifest.shows_topics() {
                format!("{ROOT}/value")
            } else {
                let name = manifest
                    .match_rules
                    .type_tag
                    .clone()
                    .unwrap_or_else(|| manifest.name.clone());
                format!("{ROOT}/{name}")
            }
        });
        let bound_type = manifest.shows_topics().then(|| {
            let sample = manifest.sandbox.value.as_ref().and_then(nt_value);
            topic_type(manifest, sample.as_ref()).to_string()
        });
        Sandbox {
            type_name: format!("{}{}", crate::plugins::TYPE_PREFIX, manifest.name),
            bound,
            bound_type,
            cells: (manifest.size.default[0], manifest.size.default[1]),
            port: None,
            topics: Vec::new(),
            served: None,
        }
    }

    /// Whether it has a server of its own (and so values to edit).
    pub fn serves(&self) -> bool {
        self.served.is_some()
    }

    /// The topic named `name`, if the sandbox serves it.
    pub fn topic(&self, name: &str) -> Option<&Topic> {
        self.topics.iter().find(|t| t.name == name)
    }

    /// `name` in full: as given if it starts with `/`, else under the
    /// widget's table (or the sandbox's root, for a topic plugin).
    pub fn full_name(&self, name: &str) -> String {
        let name = name.trim();
        if name.starts_with('/') {
            return name.to_string();
        }
        let under = if self.bound_type.is_some() {
            ROOT
        } else {
            self.bound.as_str()
        };
        format!("{under}/{name}")
    }

    fn add_value(&mut self, name: &str, ty: NtType, value: Value) {
        let Some(served) = &self.served else {
            return;
        };
        let publisher = served
            .server
            .publish_generic(name, ty.clone(), Default::default());
        let _ = publisher.set(value.clone());
        self.topics.retain(|t| t.name != name);
        self.topics.push(Topic {
            name: name.to_string(),
            ty,
            text: as_text(&value),
            value,
            publisher,
        });
        self.topics.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Serve a new topic `name` (see [`Sandbox::full_name`]) of type `ty`,
    /// starting at zero. Returns its full name, or why not.
    pub fn add(&mut self, name: &str, ty: NtType) -> Result<String, String> {
        if !self.serves() {
            return Err("the sandbox has no server of its own (--address)".into());
        }
        let trimmed = name.trim().trim_matches('/');
        if trimmed.is_empty() || trimmed.contains("//") {
            return Err(format!("{name:?} isn't a topic name"));
        }
        let full = self.full_name(name);
        if self.topic(&full).is_some() {
            return Err(format!("{full} is already there"));
        }
        let value = zero(&ty).ok_or_else(|| format!("can't serve a {ty} topic here"))?;
        self.add_value(&full, ty, value);
        Ok(full)
    }

    /// Change topic `name`'s value. It must be of the topic's type.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        let topic = self
            .topics
            .iter_mut()
            .find(|t| t.name == name)
            .ok_or_else(|| format!("{name} isn't served"))?;
        if value.default_type() != topic.ty && !matches!(topic.ty, NtType::Float) {
            return Err(format!("{name} is a {} topic", topic.ty));
        }
        topic
            .publisher
            .set(value.clone())
            .map_err(|e| e.to_string())?;
        topic.value = value;
        Ok(())
    }

    /// Stop serving topic `name` (as a robot that stops publishing it).
    pub fn remove(&mut self, name: &str) {
        self.topics.retain(|t| t.name != name);
    }

    /// The widget: the plugin's type, bound to the sandbox's topic or table,
    /// at its size.
    pub fn widget(&self, grid: f64) -> WidgetContainer {
        let mut properties = Map::new();
        properties.insert("topic".into(), json!(self.bound));
        properties.insert("period".into(), json!(0.02));
        if let Some(ty) = &self.bound_type {
            properties.insert("data_type".into(), json!(ty));
        }
        WidgetContainer {
            title: self.type_name.clone(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: f64::from(self.cells.0) * grid,
                height: f64::from(self.cells.1) * grid,
            },
            widget_type: self.type_name.clone(),
            properties,
            extra: Map::new(),
        }
    }

    /// The sandbox's layout: one tab with the widget.
    pub fn layout(&self) -> ElasticLayout {
        let mut layout = ElasticLayout::default();
        let mut tab = Tab::new("Plugin sandbox");
        tab.grid_layout.containers = vec![self.widget(f64::from(layout.grid_size()))];
        layout.tabs = vec![tab];
        layout
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn manifest(text: &str) -> Manifest {
        Manifest::from_toml(&format!("name='trial'\nversion='0.1.0'\napi='0.2'\n{text}")).unwrap()
    }

    #[test]
    fn a_table_plugin_gets_its_type_and_sample_values() {
        let m = manifest(
            "[match]\ntype='Alerts'\nhas=['level']\n\
             [sandbox.values]\nerrors=['Encoder lost']\nwarnings=[]\ncount=3\n",
        );
        let s = Sandbox::start(&m).unwrap();
        assert_eq!(s.bound, "/Sandbox/Alerts");
        assert_eq!(s.type_name, "plugin:trial");
        let value = |name: &str| s.topic(name).map(|t| t.value.clone());
        assert_eq!(
            value("/Sandbox/Alerts/.type"),
            Some(Value::String("Alerts".into()))
        );
        assert_eq!(
            value("/Sandbox/Alerts/errors"),
            Some(Value::StringArray(vec!["Encoder lost".into()]))
        );
        assert_eq!(
            value("/Sandbox/Alerts/warnings"),
            Some(Value::StringArray(vec![]))
        );
        assert_eq!(value("/Sandbox/Alerts/count"), Some(Value::Int(3)));
        // A key it needs, with no sample: zero.
        assert_eq!(value("/Sandbox/Alerts/level"), Some(Value::Double(0.0)));
        assert!(s.port.is_some());
        let w = s.widget(128.0);
        assert_eq!(w.topic(), Some("/Sandbox/Alerts"));
        assert!(!w.properties.contains_key("data_type"));
    }

    #[test]
    fn a_topic_plugin_gets_its_sample_in_a_type_it_shows() {
        let m = manifest("[match]\ntopic_types=['int','double']\n[sandbox]\nvalue=2.5\n");
        let s = Sandbox::start(&m).unwrap();
        assert_eq!(s.bound, "/Sandbox/value");
        assert_eq!(s.bound_type.as_deref(), Some("double"));
        assert_eq!(s.topic("/Sandbox/value").unwrap().value, Value::Double(2.5));
        // No sample: zero, of the first type it shows.
        let m = manifest("[match]\ntopic_types=['boolean']\n");
        let s = Sandbox::start(&m).unwrap();
        assert_eq!(
            s.topic("/Sandbox/value").unwrap().value,
            Value::Boolean(false)
        );
        assert_eq!(s.widget(128.0).properties["data_type"], json!("boolean"));
    }

    #[test]
    fn topics_are_added_set_and_removed() {
        let m = manifest("[match]\ntype='Arm'\n");
        let mut s = Sandbox::start(&m).unwrap();
        assert_eq!(
            s.add("angle", NtType::Double).as_deref(),
            Ok("/Sandbox/Arm/angle")
        );
        assert!(s.add("angle", NtType::Double).is_err(), "twice");
        assert!(s.add(" ", NtType::Double).is_err());
        assert_eq!(s.add("/Other/x", NtType::Int).as_deref(), Ok("/Other/x"));
        s.set("/Sandbox/Arm/angle", Value::Double(1.5)).unwrap();
        assert_eq!(
            s.topic("/Sandbox/Arm/angle").unwrap().value,
            Value::Double(1.5)
        );
        assert!(
            s.set("/Sandbox/Arm/angle", Value::Int(1)).is_err(),
            "wrong type"
        );
        s.remove("/Sandbox/Arm/angle");
        assert!(s.topic("/Sandbox/Arm/angle").is_none());
        // Without a server: nothing to add, the widget bound where asked.
        let mut remote = Sandbox::bound_to(&m, Some("/SmartDashboard/Arm"));
        assert!(remote.add("x", NtType::Double).is_err());
        assert_eq!(remote.bound, "/SmartDashboard/Arm");
    }

    #[test]
    fn values_edit_as_text() {
        for (ty, text, value) in [
            (
                NtType::StringArray,
                "a\nb",
                Value::StringArray(vec!["a".into(), "b".into()]),
            ),
            (NtType::IntArray, "1, 2", Value::IntArray(vec![1, 2])),
            (
                NtType::BooleanArray,
                "true, false",
                Value::BooleanArray(vec![true, false]),
            ),
            (NtType::String, "hi", Value::String("hi".into())),
        ] {
            assert_eq!(from_text(&ty, text), Some(value.clone()), "{ty}");
            assert_eq!(as_text(&value), text);
        }
        assert_eq!(
            from_text(&NtType::IntArray, ""),
            Some(Value::IntArray(vec![]))
        );
        assert_eq!(
            nt_value(&toml::Value::Array(vec![])),
            Some(Value::StringArray(vec![]))
        );
        let mixed = toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Float(0.5)]);
        assert_eq!(nt_value(&mixed), Some(Value::DoubleArray(vec![1.0, 0.5])));
    }
}
