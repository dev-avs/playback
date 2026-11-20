//! Updates from the GitHub releases of dev-avs/playback, never without a
//! click: on launch the update window asks (Later / Update), and Settings
//! has Check and Update. Ported from electron/updater.cjs.
//!
//! An update is what electron-updater did with the same files: the
//! release's web installer stub and its package, checked against
//! latest.yml, then the stub run silently with the package handed to it,
//! and the app closed so it can be replaced. Version control installs any
//! release the same way; the ones before 1.3.7 have no latest.yml, only a
//! full installer, which is run as it is.
//!
//! Only the installed app updates; anything else reports "dev".

mod download;
mod feed;

use crate::install;
use download::Progress;
use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// Progress is sent to the page at most once per this many bytes.
const REPORT_EVERY: u64 = 512 * 1024;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Checking,
    None,
    Available,
    Downloading,
    Error,
    Dev,
}

/// UpdateState in src/player.d.ts.
#[derive(Serialize, Clone, Debug)]
pub struct UpdateState {
    status: Status,
    /// the newest version on GitHub once checked, or the one being installed
    version: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
}

impl UpdateState {
    pub fn available(&self) -> Option<&str> {
        (self.status == Status::Available)
            .then_some(self.version.as_deref())
            .flatten()
    }

    fn busy(&self) -> bool {
        matches!(
            self.status,
            Status::Dev | Status::Checking | Status::Downloading
        )
    }
}

pub struct Updater {
    state: Mutex<UpdateState>,
    /// the full installer of each legacy release, from the last listing
    legacy: Mutex<HashMap<String, String>>,
}

impl Default for Updater {
    fn default() -> Self {
        let status = if install::uninstaller().is_some() {
            Status::Idle
        } else {
            Status::Dev
        };
        Updater {
            state: Mutex::new(UpdateState {
                status,
                version: None,
                downloaded: 0,
                total: None,
                error: None,
            }),
            legacy: Mutex::default(),
        }
    }
}

fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn work_dir() -> PathBuf {
    std::env::temp_dir().join("playback-update")
}

impl Updater {
    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    /// Changes the state and tells every window.
    fn set(&self, app: &AppHandle, change: impl FnOnce(&mut UpdateState)) -> UpdateState {
        let state = {
            let mut s = self.state.lock().unwrap();
            change(&mut s);
            s.clone()
        };
        let _ = app.emit("update:changed", &state);
        state
    }

    fn fail(&self, app: &AppHandle, error: String) -> UpdateState {
        eprintln!("[update] {error}");
        self.set(app, |s| {
            s.status = Status::Error;
            s.error = Some(error);
        })
    }

    pub async fn check(&self, app: &AppHandle) -> UpdateState {
        let s = self.state();
        if s.busy() {
            return s;
        }
        self.set(app, |s| {
            s.status = Status::Checking;
            s.error = None;
        });
        match feed::fetch(None).await {
            Ok(f) => {
                let newer = feed::compare(&f.version, &current(app)) == Ordering::Greater;
                self.set(app, |s| {
                    s.status = if newer {
                        Status::Available
                    } else {
                        Status::None
                    };
                    s.version = Some(f.version);
                })
            }
            Err(e) => self.fail(app, e),
        }
    }

    /// Downloads the update and restarts into it. Checks first if nobody has.
    pub async fn install(&self, app: &AppHandle) -> UpdateState {
        let s = self.state();
        if matches!(s.status, Status::Dev | Status::Downloading) {
            return s;
        }
        let s = if s.status == Status::Available {
            s
        } else {
            self.check(app).await
        };
        match s.available().map(String::from) {
            Some(version) => self.install_release(app, None, version).await,
            None => s,
        }
    }

    /// Installs one release, newer or older than this one, and restarts
    /// into it.
    pub async fn bump(&self, app: &AppHandle, version: String) -> UpdateState {
        let s = self.state();
        if s.busy() || version == current(app) {
            return s;
        }
        let legacy = self.legacy.lock().unwrap().get(&version).cloned();
        match legacy {
            Some(url) => self.install_legacy(app, version, &url).await,
            None => {
                self.install_release(app, Some(version.clone()), version)
                    .await
            }
        }
    }

    fn start_download(&self, app: &AppHandle, version: &str) {
        self.set(app, |s| {
            *s = UpdateState {
                status: Status::Downloading,
                version: Some(version.into()),
                downloaded: 0,
                total: None,
                error: None,
            };
        });
    }

    /// Counts bytes in, telling the page every so often.
    fn counter<'a>(&'a self, app: &'a AppHandle, base: u64) -> impl FnMut(Progress) + 'a {
        let (mut got, mut told) = (0u64, 0u64);
        move |p| match p {
            Progress::Size(n) => {
                self.set(app, |s| s.total = Some(s.total.unwrap_or(base) + n));
            }
            Progress::Got(n) => {
                got += n;
                if got - told >= REPORT_EVERY {
                    told = got;
                    self.set(app, |s| s.downloaded = base + got);
                }
            }
        }
    }

    /// `pinned`: that version's own feed rather than the newest.
    async fn install_release(
        &self,
        app: &AppHandle,
        pinned: Option<String>,
        version: String,
    ) -> UpdateState {
        self.start_download(app, &version);
        let done = async {
            let feed = feed::fetch(pinned.as_deref()).await?;
            if feed.version != version {
                return Err(format!("v{version} is not on GitHub"));
            }
            let dir = work_dir().join(&feed.version);
            let (stub, package) = (dir.join(&feed.stub.file), dir.join(&feed.package.file));
            download::fetch(
                &feed.url(&feed.stub),
                &stub,
                Some(&feed.stub.sha512),
                self.counter(app, 0),
            )
            .await?;
            let base = fs::metadata(&stub).map(|m| m.len()).unwrap_or(0);
            download::fetch(
                &feed.url(&feed.package),
                &package,
                Some(&feed.package.sha512),
                self.counter(app, base),
            )
            .await?;
            // as electron-updater runs it: silent, the package handed over,
            // and the app started again once it is in place
            let package_arg = format!("--package-file={}", package.display());
            install::run_detached(&stub, &["--updated", "/S", "--force-run", &package_arg])
        }
        .await;
        match done {
            Ok(()) => {
                eprintln!("[update] installing {version}");
                app.exit(0);
                self.state()
            }
            Err(e) => self.fail(app, e),
        }
    }

    /// A legacy release: its full installer, run as it is (not silent, so
    /// it starts the app again when it is done), then this one quits so it
    /// can be replaced.
    async fn install_legacy(&self, app: &AppHandle, version: String, url: &str) -> UpdateState {
        self.start_download(app, &version);
        let file = work_dir().join(format!("player-setup-{version}.exe"));
        let done = async {
            download::fetch(url, &file, None, self.counter(app, 0)).await?;
            install::run_detached(&file, &[])
        }
        .await;
        match done {
            Ok(()) => {
                app.exit(0);
                self.state()
            }
            Err(e) => self.fail(app, e),
        }
    }

    /// Every release version control can install, for the page.
    pub async fn releases(&self) -> Value {
        let (answer, legacy) = feed::releases().await;
        if !legacy.is_empty() {
            *self.legacy.lock().unwrap() = legacy;
        }
        answer
    }

    /// "Skip this version" in the update window: launch stops asking about
    /// it, and asks again when a newer one is out. Check in Settings still
    /// finds it.
    pub fn skip(&self) {
        if let Some(version) = self.state().version {
            let body = serde_json::json!({ "version": version }).to_string();
            if let Err(e) = fs::write(skip_file(), body) {
                eprintln!("[update] could not record the skip: {e}");
            }
        }
    }

    pub fn is_skipped(version: &str) -> bool {
        fs::read_to_string(skip_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .is_some_and(|v| v["version"] == version)
    }
}

fn skip_file() -> PathBuf {
    crate::support::paths::data_file("update-skip.json")
}
