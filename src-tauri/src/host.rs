//! The Tauri side of `playback_core::host::Host`: events go out through the
//! app handle, the version is the package's, what is playing is `Media`'s.

use playback_core::host::Host;
use playback_core::media::Media;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

pub struct TauriHost(pub AppHandle);

impl Host for TauriHost {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.emit(event, payload);
    }

    fn emit_widget(&self, event: &str, payload: Value) {
        let _ = self.0.emit_to(crate::ui::WIDGET, event, payload);
    }

    fn version(&self) -> String {
        self.0.package_info().version.to_string()
    }

    fn media_latest(&self) -> Option<Value> {
        self.0.state::<Media>().latest()
    }

    fn exit(&self, code: i32) {
        self.0.exit(code);
    }
}
