//! The app's windows: the widget, and the cards that open beside it.

pub mod cards;
pub mod changelog;
pub use playback_core::fonts;
pub mod widget;

/// The widget window's label (tauri.conf.json).
pub const WIDGET: &str = "widget";
