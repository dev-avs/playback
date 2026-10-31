// Playback's installer: the window people download and run.
//
// It does not install anything itself. It asks GitHub for the newest release,
// downloads that release's own installer (the small NSIS stub) and app package
// with real progress, then runs the stub silently with the chosen folder:
//
//   playback-setup-<v>.exe /S --package-file=<package> /D=<folder>
//
// the same call the app's updater makes (electron-updater's NsisUpdater). The
// stub records the folder in the registry, so later updates, Version control
// and uninstall all keep working on it as they always have.
//
// Options the stub does not know about are applied after it:
//   * no desktop shortcut: the one it made is removed
//   * Spotify bridge off: written to %APPDATA%\Playback\install-options.json,
//     which Playback reads before it touches Spicetify

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

// renamed from dev-avs/player in 1.4.3 (GitHub redirects the old name)
const REPO: &str = "dev-avs/playback";
const USER_AGENT: &str = "playback-installer";

/// Set by Hold to cancel; the download stops at its next chunk.
static CANCEL: AtomicBool = AtomicBool::new(false);
const CANCELLED: &str = "cancelled";

#[derive(Serialize, Deserialize, Clone)]
struct Release {
    version: String,
    stub: String,
    package: String,
    /// the package's size in bytes, for the progress bar
    size: u64,
}

#[derive(Serialize, Clone)]
struct Installed {
    dir: String,
    version: String,
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: u64,
}

fn default_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default().join("Programs").join("Playback")
}

/// The stub's registry id for this app: a hash of the appId
/// (com.player.smtcwidget), which the rename left alone on purpose. The install
/// folder is under HKCU\Software\<id>, the same value the updater reads.
const APP_ID_KEY: &str = "33447f23-53eb-5a3c-b487-8f4ecc19730c";

/// Where the installer suggests, and whether Playback is already there.
#[tauri::command]
fn start() -> (String, Option<Installed>) {
    // Always the default folder, %LOCALAPPDATA%\Programs\Playback; Browse picks
    // another. An older install somewhere else is replaced: the stub removes it
    // before it installs.
    let (_, installed) = find_installed();
    (default_dir().to_string_lossy().into_owned(), installed)
}

/// The folder the stub recorded, and the install in it if the app is really there.
fn find_installed() -> (Option<String>, Option<Installed>) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir: Option<String> = hkcu
        .open_subkey(format!(r"Software\{APP_ID_KEY}"))
        .and_then(|k| k.get_value("InstallLocation"))
        .ok()
        .filter(|d: &String| !d.is_empty());
    let Some(dir) = dir else { return (None, None) };
    if app_exe(Path::new(&dir)).is_none() {
        return (Some(dir), None);
    }
    let version: String = hkcu
        .open_subkey(format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID_KEY}"))
        .and_then(|k| k.get_value("DisplayVersion"))
        .unwrap_or_default();
    (Some(dir.clone()), Some(Installed { dir, version }))
}

/// The app in an install folder: Playback.exe, or player.exe from before the rename.
fn app_exe(dir: &Path) -> Option<PathBuf> {
    ["Playback.exe", "player.exe"].iter().map(|n| dir.join(n)).find(|p| p.exists())
}

/// The folder Playback goes in: the picked one, or a `Playback` folder inside it,
/// checked by writing to it (a per-user install cannot ask for admin rights).
#[tauri::command]
fn check_dir(dir: String) -> Result<String, String> {
    let mut path = PathBuf::from(dir.trim());
    if !path.is_absolute() {
        return Err("Pick a full folder path.".into());
    }
    let named = path
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("Playback"))
        .unwrap_or(false);
    if !named {
        path = path.join("Playback");
    }
    let existed = path.exists();
    std::fs::create_dir_all(&path).map_err(|_| "Playback can't be installed there. Pick a folder you own.".to_string())?;
    let probe = path.join(".playback-installer-probe");
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    if !existed {
        let _ = std::fs::remove_dir(&path);
    }
    if !writable {
        return Err("Playback can't be installed there without admin rights. Pick a folder you own.".into());
    }
    Ok(path.to_string_lossy().into_owned())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())
}

/// The newest release on GitHub and its two files.
#[tauri::command]
async fn latest() -> Result<Release, String> {
    let res = client()?
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Couldn't reach GitHub. Check your connection.".to_string())?;
    if !res.status().is_success() {
        return Err(format!("GitHub answered {}.", res.status()));
    }
    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let version = body["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    let assets = body["assets"].as_array().cloned().unwrap_or_default();
    // by suffix: the files were player-… up to 1.4.2 and playback-… since
    let find = |suffix: String| {
        assets
            .iter()
            .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(&suffix)))
            .map(|a| (a["browser_download_url"].as_str().unwrap_or_default().to_string(), a["size"].as_u64().unwrap_or(0)))
    };
    let (stub, _) = find(format!("-setup-{version}.exe")).ok_or("The newest release has no installer.")?;
    let (package, size) = find(format!("-{version}-x64.nsis.7z")).ok_or("The newest release has no package.")?;
    Ok(Release { version, stub, package, size })
}

/// Downloads `url` to `dest`, reporting each part of `total` through `report`.
async fn download(url: &str, dest: &Path, mut report: impl FnMut(u64)) -> Result<(), String> {
    let res = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| "The download didn't start. Check your connection.".to_string())?;
    if !res.status().is_success() {
        return Err(format!("GitHub answered {}.", res.status()));
    }
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut stream = res.bytes_stream();
    let mut got = 0u64;
    while let Some(chunk) = stream.next().await {
        if CANCEL.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let chunk = chunk.map_err(|_| "The download stopped. Check your connection.".to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        got += chunk.len() as u64;
        report(got);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// The whole install. Emits `progress` ({done, total} bytes) while it
/// downloads, then `phase` "installing" while the stub runs.
#[tauri::command]
async fn install(app: AppHandle, release: Release, dir: String, desktop: bool, bridge: bool) -> Result<(), String> {
    CANCEL.store(false, Ordering::Relaxed);
    let work = std::env::temp_dir().join("playback-installer");
    tokio::fs::create_dir_all(&work).await.map_err(|e| e.to_string())?;
    let stub = work.join(format!("playback-setup-{}.exe", release.version));
    let package = work.join(format!("playback-{}-x64.nsis.7z", release.version));

    let fetched = fetch(&app, &release, &stub, &package).await;
    if fetched.is_err() || CANCEL.load(Ordering::Relaxed) {
        let _ = tokio::fs::remove_dir_all(&work).await;
        return Err(fetched.err().unwrap_or_else(|| CANCELLED.into()));
    }

    // Before Playback first runs, which reads it.
    write_options(bridge)?;

    let _ = app.emit("phase", "installing");
    let status = tokio::process::Command::new(&stub)
        .arg("/S")
        .arg(format!("--package-file={}", package.display()))
        .arg(format!("/D={dir}"))
        .status()
        .await
        .map_err(|e| format!("The installer didn't start: {e}"))?;
    let _ = tokio::fs::remove_dir_all(&work).await;
    if !status.success() || app_exe(Path::new(&dir)).is_none() {
        return Err("The install didn't finish. Close Playback if it's open, then try again.".into());
    }

    if !desktop {
        if let Some(desk) = dirs::desktop_dir() {
            let _ = std::fs::remove_file(desk.join("Playback.lnk"));
            let _ = std::fs::remove_file(desk.join("player.lnk"));
        }
    }
    Ok(())
}

/// Hold to cancel: only the download can stop; the install itself runs to the end.
#[tauri::command]
fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// The stub, then the package with progress.
async fn fetch(app: &AppHandle, release: &Release, stub: &Path, package: &Path) -> Result<(), String> {
    download(&release.stub, stub, |_| {}).await?;
    let mut last = Instant::now() - Duration::from_secs(1);
    let total = release.size;
    download(&release.package, &package, |done| {
        // a few dozen updates a second is plenty for the bar
        if last.elapsed() >= Duration::from_millis(40) || done == total {
            last = Instant::now();
            let _ = app.emit("progress", Progress { done, total });
        }
    })
    .await
}

/// %APPDATA%\Playback\install-options.json, read by Playback (electron/main.cjs).
fn write_options(bridge: bool) -> Result<(), String> {
    let folder = dirs::config_dir().ok_or("No AppData folder.")?.join("Playback");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let json = serde_json::json!({ "bridge": bridge });
    std::fs::write(folder.join("install-options.json"), json.to_string()).map_err(|e| e.to_string())
}

/// Starts the installed app.
#[tauri::command]
fn launch(dir: String) -> Result<(), String> {
    let exe = app_exe(Path::new(&dir)).ok_or("Playback isn't in that folder.")?;
    std::process::Command::new(exe)
        .current_dir(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![start, check_dir, latest, install, cancel, launch])
        .run(tauri::generate_context!())
        .expect("Playback installer failed to start");
}
