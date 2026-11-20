//! The installed app: where its uninstaller is, starting programs that
//! outlive it (an installer replacing it, the cleanup after it), and
//! uninstalling.
//!
//! The installer is electron-builder's web installer, kept from the Electron
//! build so every existing install updates into this one: it puts
//! `Uninstall Playback.exe` next to the app, which is how an installed copy
//! tells itself from a dev build.

use crate::migrate::LEGACY_DATA;
use crate::spicetify::{self, Paths, Spicetify};
use crate::support::paths::data_dir;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Manager};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

fn exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// The uninstaller beside the app, named after its exe ("Uninstall
/// Playback.exe"; it was "Uninstall player.exe"). None for a dev build.
pub fn uninstaller() -> Option<PathBuf> {
    let exe = exe()?;
    let name = exe.file_stem()?.to_string_lossy().into_owned();
    Some(exe.with_file_name(format!("Uninstall {name}.exe"))).filter(|p| p.exists())
}

/// Starts a program that keeps running after the app exits: its own process
/// group, and out of any job the app was started in (a job can be set to
/// end everything in it along with the app).
pub fn run_detached(program: &Path, args: &[&str]) -> Result<(), String> {
    let start = |flags| {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    let flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    start(flags | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| start(flags))
        .map(drop)
        .map_err(|e| format!("could not start {}: {e}", program.display()))
}

/// PowerShell's quoting of a literal path.
fn quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "''"))
}

/// Settings → App → Uninstall: the bridge out of Spicetify (restarts
/// Spotify), then, once the app has exited, the app and everything it
/// kept: the data folder (settings file, Spicy key, sign-in, lyrics cache),
/// the one from before the rename, and the web view's profile.
pub async fn uninstall(app: &AppHandle) -> Result<(), String> {
    let (Some(uninstaller), Some(exe)) = (uninstaller(), exe()) else {
        return Err("Only the installed app can uninstall itself.".into());
    };
    match spicetify::remove(&Paths::of_this_pc(), &Spicetify).await {
        Ok(actions) => actions.iter().for_each(|a| eprintln!("[uninstall] {a}")),
        Err(e) => eprintln!("[uninstall] bridge: {e}"),
    }
    let data = data_dir();
    let mut folders = vec![data.clone(), data.with_file_name(LEGACY_DATA)];
    folders.extend(app.path().app_local_data_dir().ok());
    let name = exe
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut steps = vec![
        format!("Wait-Process -Id {} -ErrorAction SilentlyContinue", std::process::id()),
        // the web view's processes let go of its profile a moment later
        "Start-Sleep -Seconds 2".into(),
        format!(
            "Get-Process {} -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -eq {} }} | Stop-Process -Force",
            quote(Path::new(&name)),
            quote(&exe)
        ),
        format!("Start-Process -FilePath {} -ArgumentList '/S' -Wait", quote(&uninstaller)),
    ];
    steps.extend(folders.iter().map(|f| {
        format!(
            "Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue",
            quote(f)
        )
    }));
    let script: Vec<u16> = steps.join("; ").encode_utf16().collect();
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        script
            .iter()
            .flat_map(|u| u.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    run_detached(
        Path::new("powershell.exe"),
        &[
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded,
        ],
    )
    .map_err(|_| "Could not start the uninstaller.".to_string())?;
    app.exit(0);
    Ok(())
}
