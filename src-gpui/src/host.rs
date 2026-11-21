//! The GPUI side of `playback_core::host::Host` (`src-tauri/src/host.rs` is the Tauri one).
//!
//! Slice 3a draws no real UI and talks to no backend, so events are kept for the screens that will
//! read them, `media_latest` serves the `--fake-state` event when there is one, and `exit` leaves
//! the process (an installer is taking over).

use playback_core::host::Host;
use serde_json::Value;
use std::sync::Mutex;

#[derive(Default)]
pub struct GpuiHost {
    /// (event, payload, widget-only) in order of arrival.
    pub events: Mutex<Vec<(String, Value, bool)>>,
    pub fake_latest: Mutex<Option<Value>>,
}

impl Host for GpuiHost {
    fn emit(&self, event: &str, payload: Value) {
        self.events.lock().unwrap().push((event.to_string(), payload, false));
    }

    fn emit_widget(&self, event: &str, payload: Value) {
        self.events.lock().unwrap().push((event.to_string(), payload, true));
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn media_latest(&self) -> Option<Value> {
        self.fake_latest.lock().unwrap().clone()
    }

    fn exit(&self, code: i32) {
        std::process::exit(code);
    }
}
