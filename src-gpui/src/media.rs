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
    pub accent_top: (u8, u8, u8),
    pub accent_bottom: (u8, u8, u8),
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
            accent_top: (42, 32, 48),
            accent_bottom: (16, 16, 20),
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

type ThumbnailData = (u32, u32, Vec<u8>, (u8, u8, u8), (u8, u8, u8));

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
    let (top, bottom) = split_accents(&rgba);
    // GPUI ImageBuffer expects BGRA on Windows/DirectX; convert RGBA -> BGRA.
    let mut bgra = rgba.into_raw();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Some((96, 96, bgra, top, bottom))
}

fn boost_saturation(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    let avg = (rf + gf + bf) / 3.0;
    let sat = 1.45;
    let clamp = |v: f32| v.clamp(0.0, 255.0) as u8;
    (
        clamp(avg + (rf - avg) * sat),
        clamp(avg + (gf - avg) * sat),
        clamp(avg + (bf - avg) * sat),
    )
}

fn split_accents(rgba: &image::RgbaImage) -> ((u8, u8, u8), (u8, u8, u8)) {
    let h = rgba.height();
    let (mut tr, mut tg, mut tb, mut tn) = (0u64, 0u64, 0u64, 0u64);
    let (mut br, mut bg, mut bb, mut bn) = (0u64, 0u64, 0u64, 0u64);
    for (y, row) in rgba.enumerate_rows() {
        for (_, _, px) in row.step_by(4) {
            if px[3] <= 128 {
                continue;
            }
            if y < h / 2 {
                tr += px[0] as u64;
                tg += px[1] as u64;
                tb += px[2] as u64;
                tn += 1;
            } else {
                br += px[0] as u64;
                bg += px[1] as u64;
                bb += px[2] as u64;
                bn += 1;
            }
        }
    }
    let top = if tn > 0 {
        boost_saturation((tr / tn) as u8, (tg / tn) as u8, (tb / tn) as u8)
    } else {
        (42, 32, 48)
    };
    let bottom = if bn > 0 {
        boost_saturation((br / bn) as u8, (bg / bn) as u8, (bb / bn) as u8)
    } else {
        (16, 16, 20)
    };
    (top, bottom)
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
    let (art_rgba, accent_top, accent_bottom) = if key != prev_key {
        if let Some((w, h, bgra, top, bottom)) = read_thumbnail(&props) {
            (Some((w, h, bgra)), top, bottom)
        } else {
            (None, (42, 32, 48), (16, 16, 20))
        }
    } else {
        (None, (42, 32, 48), (16, 16, 20))
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
        accent_top,
        accent_bottom,
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
