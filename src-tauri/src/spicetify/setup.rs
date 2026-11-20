//! The Setup window's state: the latest `ensure`, kept for the page to ask
//! for, and the choice made in the installer (Spotify bridge off).

use super::{check, ensure, Paths, Spicetify, Status, SOURCE};
use crate::support::paths::data_file;
use serde::Serialize;
use std::fs;
use tokio::sync::Mutex;

/// SetupState in src/player.d.ts.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub status: Status,
    pub ready: bool,
    pub actions: Vec<String>,
    pub needs_apply: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bridge_off: bool,
    pub first_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SetupState {
    /// Whether Setup opens at launch: always on the first run, later only
    /// when the user has something to do there. A missing Spotify or
    /// Spicetify is not brought up again after the first run, since the
    /// widget works without them.
    pub fn worth_showing(&self) -> bool {
        self.first_run || self.needs_apply || self.error.is_some()
    }
}

/// The installer writes `"bridge": false` here when it was turned off
/// there; then Spicetify is left alone until Setup's Turn on. No file means
/// wanted, as for every install from before the option.
const OPTIONS: &str = "install-options.json";

fn bridge_wanted() -> bool {
    fs::read_to_string(data_file(OPTIONS))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .is_none_or(|o| o["bridge"] != false)
}

fn want_bridge() {
    if let Err(e) = fs::write(data_file(OPTIONS), r#"{"bridge":true}"#) {
        eprintln!("[setup] could not record the bridge choice: {e}");
    }
}

pub struct Setup {
    first_run: bool,
    /// Held while a run is going, so asking for the state waits for it.
    latest: Mutex<Option<SetupState>>,
}

impl Setup {
    pub fn new(first_run: bool) -> Setup {
        Setup { first_run, latest: Mutex::new(None) }
    }

    /// Puts the bridge in place; `allow_apply` lets it run `spicetify
    /// apply`, which restarts Spotify.
    pub async fn run(&self, allow_apply: bool) -> SetupState {
        let mut latest = self.latest.lock().await;
        let state = self.work(allow_apply).await;
        *latest = Some(state.clone());
        state
    }

    /// The latest run's outcome, waiting for one that is going.
    pub async fn state(&self) -> SetupState {
        if let Some(s) = self.latest.lock().await.clone() {
            return s;
        }
        self.run(false).await
    }

    /// Setup's Apply (and Turn on, for a bridge left off in the installer).
    pub async fn apply(&self) -> SetupState {
        want_bridge();
        self.run(true).await
    }

    async fn work(&self, allow_apply: bool) -> SetupState {
        let paths = Paths::of_this_pc();
        let first_run = self.first_run;
        if !bridge_wanted() {
            // turned off in the installer: only report, never touch Spicetify
            let status = check(SOURCE, &paths);
            return SetupState { ready: status.ready(), status, actions: vec![], needs_apply: false, bridge_off: true, first_run, error: None };
        }
        match ensure(SOURCE, allow_apply, &paths, &Spicetify).await {
            Ok(r) => {
                for a in &r.actions {
                    eprintln!("[setup] {a}");
                }
                if r.needs_apply {
                    eprintln!("[setup] the bridge waits on `spicetify apply`");
                }
                SetupState {
                    ready: r.status.ready(),
                    status: r.status,
                    actions: r.actions,
                    needs_apply: r.needs_apply,
                    bridge_off: false,
                    first_run,
                    error: None,
                }
            }
            // say what failed, and offer the apply again
            Err(e) => {
                eprintln!("[setup] {e}");
                let status = check(SOURCE, &paths);
                let ready = status.ready();
                SetupState { needs_apply: status.spicetify && !ready, ready, status, actions: vec![], bridge_off: false, first_run, error: Some(e) }
            }
        }
    }
}
