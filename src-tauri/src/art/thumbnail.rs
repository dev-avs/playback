//! The cover Windows' media session holds for an app, as a data: URL.
//!
//! Small (a browser hands over ~150 px), but it is the picture the app itself
//! shows. Replaces native/smtc-thumb, which existed only because PowerShell
//! could not read it.

use base64::Engine;
use std::time::Duration;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
use windows::Storage::Streams::DataReader;

enum Thumb {
    Found(String),
    /// the session has none yet; a browser often sets it a moment late
    NotYet,
    /// the session is gone, or now on another song
    Gone,
}

fn read(app_id: &str, title: &str) -> windows::core::Result<Thumb> {
    let mgr = Manager::RequestAsync()?.join()?;
    let session = mgr
        .GetSessions()?
        .into_iter()
        .find(|s| s.SourceAppUserModelId().is_ok_and(|id| id == app_id));
    let Some(session) = session else { return Ok(Thumb::Gone) };
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    // the cover must belong to the song that asked, not the one before it
    if props.Title()? != title {
        return Ok(Thumb::Gone);
    }
    let Ok(thumb) = props.Thumbnail() else { return Ok(Thumb::NotYet) };
    let stream = thumb.OpenReadAsync()?.join()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(Thumb::NotYet);
    }
    let kind = stream.ContentType()?.to_string();
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    let kind = if kind.starts_with("image/") { kind } else { "image/png".into() };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Thumb::Found(format!("data:{kind};base64,{b64}")))
}

/// Asks a few times while the session has no cover yet.
pub async fn find(app_id: &str, title: &str) -> Option<String> {
    if app_id.is_empty() || title.is_empty() {
        return None;
    }
    for _ in 0..4 {
        let (id, t) = (app_id.to_string(), title.to_string());
        let thumb = tauri::async_runtime::spawn_blocking(move || read(&id, &t)).await.ok()?;
        match thumb {
            Ok(Thumb::Found(url)) => return Some(url),
            Ok(Thumb::NotYet) => tokio::time::sleep(Duration::from_millis(400)).await,
            Ok(Thumb::Gone) => return None,
            Err(e) => {
                eprintln!("[art:thumbnail] {}", e.message());
                return None;
            }
        }
    }
    None
}

