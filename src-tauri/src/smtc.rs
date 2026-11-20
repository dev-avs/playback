// Windows' media controls (SMTC): what is playing, and the buttons.
//
// A port of electron/smtc.ps1. One thread owns the session manager: it polls
// every 250 ms, emits a state when something changed (or every 5 s as a
// heartbeat), and runs commands sent to it in between. The state has the same
// shape the PowerShell script printed, so the page reads it unchanged
// (SmtcState in src/player.d.ts).
//
// Not ported yet: per-app volume (always -1, so the page hides the slider) and
// passing over a Spotify started hidden for lookups.

use serde_json::{json, Value};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Media::MediaPlaybackAutoRepeatMode as Repeat;

/// Music apps win over anything else (a video in the browser).
const MUSIC_APPS: [&str; 4] = ["spotify", "cider", "applemusic", "itunes"];

pub enum Command {
    Play,
    Pause,
    PlayPause,
    Next,
    Prev,
    Seek(f64),
    Shuffle,
    Repeat,
}

impl Command {
    /// The same words electron/main.cjs wrote to the script's stdin.
    pub fn parse(line: &str) -> Option<Command> {
        let mut parts = line.split_whitespace();
        Some(match parts.next()? {
            "play" => Command::Play,
            "pause" => Command::Pause,
            "playpause" => Command::PlayPause,
            "next" => Command::Next,
            "prev" => Command::Prev,
            "seek" => Command::Seek(parts.next()?.parse().ok()?),
            "shuffle" => Command::Shuffle,
            "repeat" => Command::Repeat,
            _ => return None,
        })
    }
}

fn is_music(s: &Session) -> bool {
    let id = s.SourceAppUserModelId().map(|h| h.to_string().to_lowercase()).unwrap_or_default();
    MUSIC_APPS.iter().any(|m| id.contains(m))
}

fn status_of(s: &Session) -> Status {
    s.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).unwrap_or(Status::Closed)
}

fn same_app(a: &Session, b: &Session) -> bool {
    match (a.SourceAppUserModelId(), b.SourceAppUserModelId()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// A playing music app (the current one if several), else a paused one, else
/// whatever Windows calls current.
fn pick(mgr: &Manager) -> Option<Session> {
    let all: Vec<Session> = mgr.GetSessions().ok()?.into_iter().collect();
    let cur = mgr.GetCurrentSession().ok();
    for want in [Status::Playing, Status::Paused] {
        let hits: Vec<&Session> = all.iter().filter(|s| is_music(s) && status_of(s) == want).collect();
        if hits.is_empty() {
            continue;
        }
        if let Some(c) = &cur {
            if hits.iter().any(|h| same_app(h, c)) {
                return Some(c.clone());
            }
        }
        return Some(hits[0].clone());
    }
    cur
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Closed => "Closed",
        Status::Opened => "Opened",
        Status::Changing => "Changing",
        Status::Stopped => "Stopped",
        Status::Playing => "Playing",
        Status::Paused => "Paused",
        _ => "Unknown",
    }
}

fn repeat_name(r: Repeat) -> &'static str {
    match r {
        Repeat::Track => "Track",
        Repeat::List => "List",
        _ => "None",
    }
}

/// WinRT DateTime (100 ns ticks since 1601) to epoch ms.
fn epoch_ms(ticks: i64) -> f64 {
    (ticks - 116_444_736_000_000_000) as f64 / 10_000.0
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn run(session: &Session, cmd: Command) -> windows::core::Result<()> {
    match cmd {
        Command::Play => drop(session.TryPlayAsync()?),
        Command::Pause => drop(session.TryPauseAsync()?),
        Command::PlayPause => drop(session.TryTogglePlayPauseAsync()?),
        Command::Next => drop(session.TrySkipNextAsync()?),
        Command::Prev => drop(session.TrySkipPreviousAsync()?),
        Command::Seek(sec) => drop(session.TryChangePlaybackPositionAsync((sec * 10_000_000.0) as i64)?),
        Command::Shuffle => {
            let on = session.GetPlaybackInfo()?.IsShuffleActive().and_then(|r| r.Value()).unwrap_or(false);
            drop(session.TryChangeShuffleActiveAsync(!on)?);
        }
        Command::Repeat => {
            let cur = session.GetPlaybackInfo()?.AutoRepeatMode().and_then(|r| r.Value()).unwrap_or(Repeat::None);
            let next = match cur {
                Repeat::None => Repeat::List,
                Repeat::List => Repeat::Track,
                _ => Repeat::None,
            };
            drop(session.TryChangeAutoRepeatModeAsync(next)?);
        }
    }
    Ok(())
}

/// What the poll loop remembers between passes.
#[derive(Default)]
struct Last {
    key: String,
    status: String,
    modes: String,
    anchor_pos: f64,
    anchor_at: f64,
    emitted_at: f64,
    none_sent: bool,
}

/// Reads one state; Some when it should be emitted.
fn sample(mgr: &Manager, last: &mut Last) -> windows::core::Result<Option<Value>> {
    let Some(s) = pick(mgr) else {
        if !last.none_sent {
            *last = Last { none_sent: true, anchor_pos: -1.0, ..Default::default() };
            return Ok(Some(json!({ "type": "state", "session": null })));
        }
        return Ok(None);
    };
    last.none_sent = false;

    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let tl = s.GetTimelineProperties()?;
    let pb = s.GetPlaybackInfo()?;

    let app_id = s.SourceAppUserModelId()?.to_string();
    let title = props.Title()?.to_string();
    let artist = props.Artist()?.to_string();
    let album = props.AlbumTitle()?.to_string();
    let key = format!("{app_id}|{title}|{artist}|{album}");
    let status = status_name(pb.PlaybackStatus()?);
    let playing = status == "Playing";
    let shuffle = pb.IsShuffleActive().and_then(|r| r.Value()).unwrap_or(false);
    let repeat = repeat_name(pb.AutoRepeatMode().and_then(|r| r.Value()).unwrap_or(Repeat::None));
    let modes = format!("{shuffle}|{repeat}");
    let pos = tl.Position()?.Duration as f64 / 10_000_000.0;
    let end = tl.EndTime()?.Duration as f64 / 10_000_000.0;
    let updated = epoch_ms(tl.LastUpdatedTime()?.UniversalTime);

    // Spotify refreshes its timeline only every few seconds: correct Position
    // by its age, then compare with the anchor's prediction to spot a real seek.
    let now = now_ms();
    let mut age = (now - updated) / 1000.0;
    if !(0.0..=30.0).contains(&age) {
        age = 0.0;
    }
    let actual = if playing { pos + age } else { pos };
    let expected = if playing { last.anchor_pos + (now - last.anchor_at) / 1000.0 } else { last.anchor_pos };
    let seeked = last.anchor_pos >= 0.0 && (actual - expected).abs() > 1.5;

    let changed = key != last.key || status != last.status || modes != last.modes;
    if !changed && !seeked && now - last.emitted_at <= 5000.0 {
        return Ok(None);
    }

    let c = pb.Controls()?;
    let state = json!({
        "type": "state",
        "appId": app_id,
        "title": title,
        "artist": artist,
        "album": album,
        "trackKey": key,
        "status": status,
        "playing": playing,
        "position": pos,
        "duration": end,
        "updatedAt": updated as i64,
        "sampledAt": now as i64,
        "shuffle": shuffle,
        "repeat": repeat,
        "volume": -1,
        "caps": {
            "play": c.IsPlayEnabled()?,
            "pause": c.IsPauseEnabled()?,
            "next": c.IsNextEnabled()?,
            "prev": c.IsPreviousEnabled()?,
            "seek": c.IsPlaybackPositionEnabled()?,
            "shuffle": c.IsShuffleEnabled()?,
            "repeat": c.IsRepeatEnabled()?,
        },
    });
    *last = Last {
        key,
        status: status.into(),
        modes,
        anchor_pos: actual,
        anchor_at: now,
        emitted_at: now,
        none_sent: false,
    };
    Ok(Some(state))
}

/// Runs forever on its own thread. `emit` gets every state and error, and the
/// newest state is also kept in `latest` for a page that loads later.
pub fn spawn(commands: Receiver<Command>, latest: Arc<Mutex<Option<Value>>>, emit: impl Fn(Value) + Send + 'static) {
    std::thread::spawn(move || {
        // WinRT on this thread, multithreaded so the async calls can complete.
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let error = |m: String| json!({ "type": "error", "message": m });
        let mgr = loop {
            match Manager::RequestAsync().and_then(|op| op.join()) {
                Ok(m) => break m,
                Err(e) => {
                    emit(error(e.message()));
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        };
        let mut last = Last { anchor_pos: -1.0, ..Default::default() };
        let mut next_poll = Instant::now();
        loop {
            if Instant::now() >= next_poll {
                match sample(&mgr, &mut last) {
                    Ok(Some(state)) => {
                        *latest.lock().unwrap() = Some(state.clone());
                        emit(state);
                    }
                    Ok(None) => {}
                    Err(e) => emit(error(e.message())),
                }
                next_poll = Instant::now() + Duration::from_millis(250);
            }
            match commands.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
                Ok(cmd) => {
                    if let Some(s) = pick(&mgr) {
                        if let Err(e) = run(&s, cmd) {
                            emit(error(format!("command failed: {}", e.message())));
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}
