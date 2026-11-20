use std::time::{Duration, Instant};
use windows::Foundation::TimeSpan;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;

#[derive(Clone, Debug)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub app_name: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub sampled_at: Instant,
    pub art_rgba: Option<(u32, u32, Vec<u8>)>,
    pub accent_rgb: (u8, u8, u8),
}

impl Default for NowPlaying {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            app_name: String::new(),
            playing: false,
            position: 0.0,
            duration: 0.0,
            sampled_at: Instant::now(),
            art_rgba: None,
            accent_rgb: (46, 36, 58),
        }
    }
}

impl NowPlaying {
    pub fn track_key(&self) -> String {
        format!("{}|{}", self.title, self.artist)
    }

    pub fn current_position(&self) -> f64 {
        if self.playing {
            (self.position + self.sampled_at.elapsed().as_secs_f64()).min(self.duration.max(0.0))
        } else {
            self.position
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MediaAction {
    TogglePlay,
    Next,
    Previous,
    Seek(f64),
}

fn pick_session(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Option<GlobalSystemMediaTransportControlsSession> {
    if let Ok(cur) = manager.GetCurrentSession() {
        return Some(cur);
    }
    let sessions = manager.GetSessions().ok()?;
    let count = sessions.Size().ok()?;
    for i in 0..count {
        if let Ok(s) = sessions.GetAt(i) {
            return Some(s);
        }
    }
    None
}

fn friendly_app(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    if lower.contains("spotify") {
        "Spotify".into()
    } else if lower.contains("chrome") {
        "Chrome".into()
    } else if lower.contains("msedge") || lower.contains("edge") {
        "Edge".into()
    } else if lower.contains("firefox") {
        "Firefox".into()
    } else if lower.contains("apple") || lower.contains("itunes") {
        "Apple Music".into()
    } else {
        id.split(['!', '.'])
            .next()
            .unwrap_or("Media")
            .trim()
            .to_string()
    }
}

type ThumbnailData = (u32, u32, Vec<u8>, (u8, u8, u8));

fn read_thumbnail(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<ThumbnailData> {
    let thumb = props.Thumbnail().ok()?;
    let stream = thumb.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 10 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let loaded = reader.LoadAsync(size).ok()?.get().ok()?;
    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let small = img.resize_to_fill(96, 96, image::imageops::FilterType::Triangle);
    let rgba = small.to_rgba8();
    let accent = average_accent(&rgba);
    // GPUI ImageBuffer expects BGRA on Windows/DirectX; convert RGBA -> BGRA.
    let mut bgra = rgba.into_raw();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Some((96, 96, bgra, accent))
}

fn average_accent(rgba: &image::RgbaImage) -> (u8, u8, u8) {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in rgba.pixels().step_by(8) {
        if px[3] > 128 {
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return (46, 36, 58);
    }
    ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

pub fn poll_smtc(prev_key: &str) -> Option<NowPlaying> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .ok()?
        .get()
        .ok()?;
    let session = pick_session(&manager)?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props
        .Title()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let artist = props
        .Artist()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_default();
    if title.is_empty() && artist.is_empty() {
        return None;
    }
    let app_id = session
        .SourceAppUserModelId()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let playback = session.GetPlaybackInfo().ok();
    let playing = playback
        .and_then(|p| p.PlaybackStatus().ok())
        .is_some_and(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);

    let timeline = session.GetTimelineProperties().ok();
    let duration = timeline
        .as_ref()
        .and_then(|t| t.EndTime().ok())
        .map(|d| d.Duration as f64 / 10_000_000.0)
        .unwrap_or(0.0);
    let position = timeline
        .as_ref()
        .and_then(|t| t.Position().ok())
        .map(|d| d.Duration as f64 / 10_000_000.0)
        .unwrap_or(0.0);

    let key = format!("{title}|{artist}");
    let (art_rgba, accent_rgb) = if key != prev_key {
        if let Some((w, h, bgra, accent)) = read_thumbnail(&props) {
            (Some((w, h, bgra)), accent)
        } else {
            (None, (46, 36, 58))
        }
    } else {
        (None, (46, 36, 58))
    };

    Some(NowPlaying {
        title,
        artist,
        app_name: friendly_app(&app_id),
        playing,
        position,
        duration,
        sampled_at: Instant::now(),
        art_rgba,
        accent_rgb,
    })
}

pub fn send_action(action: MediaAction) {
    let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() else {
        return;
    };
    let Ok(manager) = op.get() else { return };
    let Some(session) = pick_session(&manager) else {
        return;
    };
    match action {
        MediaAction::TogglePlay => {
            let _ = session.TryTogglePlayPauseAsync();
        }
        MediaAction::Next => {
            let _ = session.TrySkipNextAsync();
        }
        MediaAction::Previous => {
            let _ = session.TrySkipPreviousAsync();
        }
        MediaAction::Seek(secs) => {
            let ticks = (secs.max(0.0) * 10_000_000.0) as i64;
            let _ = session.TryChangePlaybackPositionAsync(ticks);
        }
    }
}

pub fn format_time(secs: f64) -> String {
    let total = secs.max(0.0).floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

#[allow(dead_code)]
fn _keep_timespan_import(_: TimeSpan, _: Duration) {}
