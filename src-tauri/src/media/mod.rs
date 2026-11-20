//! What Windows is playing: the media session (smtc) and the app's own
//! volume (volume).

pub mod smtc;
pub mod volume;

/// Music apps win over anything else that plays (a video in the browser), and
/// get their cover from a catalogue rather than from the session.
const MUSIC_APPS: [&str; 4] = ["spotify", "cider", "applemusic", "itunes"];

pub fn is_music_app(app_id: &str) -> bool {
    let id = app_id.to_lowercase();
    MUSIC_APPS.iter().any(|m| id.contains(m))
}

use serde_json::Value;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// The handle the app keeps: sends button presses to the media thread and
/// holds its newest state.
pub struct Media {
    commands: Mutex<Sender<smtc::Command>>,
    latest: Arc<Mutex<Option<Value>>>,
}

impl Media {
    /// Starts the media thread; `emit` gets every state and error it sends.
    pub fn start(emit: impl Fn(Value) + Send + 'static) -> Media {
        let (tx, rx) = channel();
        let latest = Arc::new(Mutex::new(None));
        smtc::spawn(rx, latest.clone(), emit);
        Media { commands: Mutex::new(tx), latest }
    }

    /// One command line, as smtc.ps1 read them ("seek 42.5", "volume 0.3").
    pub fn send(&self, line: &str) -> bool {
        smtc::Command::parse(line).is_some_and(|c| self.commands.lock().unwrap().send(c).is_ok())
    }

    /// The newest state, for a page that loaded after it was sent.
    pub fn latest(&self) -> Option<Value> {
        self.latest.lock().unwrap().clone()
    }
}
