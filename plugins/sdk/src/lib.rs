//! Write a raptaboard dashboard widget in Rust.
//!
//! A widget is a type implementing [`Widget`], handed to [`widget!`]. Build it
//! for WebAssembly with `cargo build --release --target wasm32-wasip2`, then put
//! the `.wasm` beside a `plugin.toml` as `widget.wasm`:
//!
//! ```ignore
//! use raptaboard_plugin::prelude::*;
//!
//! #[derive(Default)]
//! struct Hello;
//!
//! impl Widget for Hello {
//!     fn subscriptions(&self) -> Vec<Subscription> {
//!         vec![Subscription::Bound]
//!     }
//!
//!     fn render(&mut self, frame: FrameInfo) -> Picture {
//!         let value = number(&host::get("")).map(|n| n.to_string());
//!         let mut picture = Picture::empty();
//!         picture.text(centered_text(&frame, value.as_deref().unwrap_or("—"), 20.0));
//!         picture
//!     }
//! }
//!
//! raptaboard_plugin::widget!(Hello);
//! ```
//!
//! The dashboard makes one copy of the plugin per widget, so a widget's state
//! is simply the fields of its type. The interface itself is `wit/dashboard.wit`
//! in the raptaboard repo; [`bindings`] is the code made from it.

pub mod bindings {
    //! The code `wit-bindgen` makes from `wit/dashboard.wit`.
    wit_bindgen::generate!({
        path: "../../wit",
        world: "widget",
        pub_export_macro: true,
        default_bindings_module: "raptaboard_plugin::bindings",
    });
}

pub use bindings::raptaboard::dashboard::host;
pub use bindings::raptaboard::dashboard::types::*;

/// Everything a widget usually needs, for `use raptaboard_plugin::prelude::*`.
pub mod prelude {
    pub use crate::bindings::raptaboard::dashboard::host;
    pub use crate::bindings::raptaboard::dashboard::types::*;
    pub use crate::{
        Widget, centered_text, color, format_number, number, property_bool, property_number,
        property_string,
    };
}

/// A dashboard widget. Only `subscriptions` and `render` are required.
pub trait Widget: Default + 'static {
    /// The values to be sent to [`Widget::update`]. Asked again when a
    /// property changes.
    fn subscriptions(&self) -> Vec<Subscription>;

    /// Changes to subscribed values since the last call, oldest first. The
    /// newest value of each is also there from [`host::get`].
    fn update(&mut self, _samples: Vec<Sample>) {}

    /// Draw the widget: shapes in pixels from its top-left corner.
    fn render(&mut self, frame: FrameInfo) -> Picture;

    /// The mark for the widget's tab, if it needs attention.
    fn tab_mark(&self) -> Option<Level> {
        None
    }

    /// A click, key or other input inside the widget.
    fn on_input(&mut self, _event: Input) {}
}

/// Make a [`Widget`] type the plugin's widget. Use it once, at the crate root.
#[macro_export]
macro_rules! widget {
    ($widget:ty) => {
        struct __RaptaboardPlugin;

        ::std::thread_local! {
            static __RAPTABOARD_WIDGET: ::std::cell::RefCell<$widget> =
                ::std::cell::RefCell::new(<$widget as ::std::default::Default>::default());
        }

        fn __raptaboard_with<R>(f: impl FnOnce(&mut $widget) -> R) -> R {
            __RAPTABOARD_WIDGET.with(|w| f(&mut w.borrow_mut()))
        }

        impl $crate::bindings::Guest for __RaptaboardPlugin {
            fn subscriptions() -> ::std::vec::Vec<$crate::Subscription> {
                __raptaboard_with(|w| $crate::Widget::subscriptions(w))
            }
            fn update(samples: ::std::vec::Vec<$crate::Sample>) {
                __raptaboard_with(|w| $crate::Widget::update(w, samples))
            }
            fn render(frame: $crate::FrameInfo) -> $crate::Picture {
                __raptaboard_with(|w| $crate::Widget::render(w, frame))
            }
            fn tab_mark() -> ::std::option::Option<$crate::Level> {
                __raptaboard_with(|w| $crate::Widget::tab_mark(w))
            }
            fn on_input(event: $crate::Input) {
                __raptaboard_with(|w| $crate::Widget::on_input(w, event))
            }
        }

        $crate::bindings::export!(__RaptaboardPlugin);
    };
}

impl Picture {
    /// A picture with nothing in it, drawn again only when something changes.
    pub fn empty() -> Picture {
        Picture {
            shapes: Vec::new(),
            regions: Vec::new(),
            redraw_in_s: None,
            summary: None,
        }
    }

    /// Add a piece of text.
    pub fn text(&mut self, text: Text) -> &mut Self {
        self.shapes.push(Shape::Text(text));
        self
    }

    /// Add any shape.
    pub fn shape(&mut self, shape: Shape) -> &mut Self {
        self.shapes.push(shape);
        self
    }
}

/// A color from red, green and blue (0-255), fully opaque.
pub fn color(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

/// A value as a number, if it's a single number (double, int).
pub fn number(value: &Option<NtValue>) -> Option<f64> {
    match value {
        Some(NtValue::Double(d)) => Some(*d),
        Some(NtValue::Int(i)) => Some(*i as f64),
        _ => None,
    }
}

/// A number with `decimals` places, or with up to 4 and trailing zeros
/// dropped when `decimals` is `None` (the dashboard's own style).
///
/// ```
/// use raptaboard_plugin::format_number;
/// assert_eq!(format_number(3.14159, Some(2)), "3.14");
/// assert_eq!(format_number(2.5, None), "2.5");
/// assert_eq!(format_number(-0.00001, None), "0");
/// ```
pub fn format_number(n: f64, decimals: Option<usize>) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let text = match decimals {
        Some(d) => format!("{n:.d$}"),
        None => {
            let t = format!("{n:.4}");
            t.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    };
    // "-0.00" reads better as "0.00".
    match text.strip_prefix('-') {
        Some(rest) if rest.parse::<f64>() == Ok(0.0) => rest.to_string(),
        _ => text,
    }
}

/// A number property, or `default` if it isn't set or isn't a number.
pub fn property_number(name: &str, default: f64) -> f64 {
    number(&host::property(name)).unwrap_or(default)
}

/// A true/false property, or `default`.
pub fn property_bool(name: &str, default: bool) -> bool {
    match host::property(name) {
        Some(NtValue::Boolean(b)) => b,
        _ => default,
    }
}

/// A text property (also enum and sound properties), or `default`.
pub fn property_string(name: &str, default: &str) -> String {
    match host::property(name) {
        Some(NtValue::Str(s)) => s,
        _ => default.to_string(),
    }
}

/// One line of text in the middle of the widget, in the theme's text color.
pub fn centered_text(frame: &FrameInfo, text: &str, size: f32) -> Text {
    let height = host::measure_text(text, size, None, false).height;
    Text {
        pos: Point {
            x: frame.size.width / 2.0,
            y: (frame.size.height - height) / 2.0,
        },
        text: text.to_string(),
        size,
        color: frame.theme.text,
        align: Align::Center,
        max_width: None,
        monospace: false,
    }
}
