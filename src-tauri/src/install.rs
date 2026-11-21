//! Uninstalling from the Tauri build: the shared steps
//! (playback_core::install) plus the web view's profile, then exit.

use tauri::{AppHandle, Manager};

/// Settings → App → Uninstall.
pub async fn uninstall(app: &AppHandle) -> Result<(), String> {
    let extra = app.path().app_local_data_dir().ok().into_iter().collect();
    playback_core::install::uninstall(extra).await?;
    app.exit(0);
    Ok(())
}
