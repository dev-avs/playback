//! A hidden Spotify, started for a few seconds when another app plays: the
//! Spicetify extension then supplies the song's id and a fresh token, and
//! this closes it again. Ported from electron/spotifyQuickOpen.cjs, with the
//! window hiding smtc.ps1 did ('hide-spotify').
//!
//! Only ever a Spotify this started, never the user's: none starts when
//! Spotify already runs, and one the user takes over (it plays) is left be.

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, ShowWindow, GW_OWNER, SW_HIDE,
};

const MAX_OPEN: Duration = Duration::from_secs(25);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ARGS: [&str; 4] = [
    "--minimized",
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
];

/// The process this started, while it is ours to close.
static OPENED: Mutex<Option<u32>> = Mutex::new(None);
/// Bumped per open, so an old timer cannot close a newer copy.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn exe() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("Spotify").join("Spotify.exe")).filter(|p| p.exists())
}

/// Process ids of every Spotify.exe.
fn spotify_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return pids };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("spotify.exe") {
                pids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    pids
}

unsafe extern "system" fn hide_one(hwnd: HWND, pids: LPARAM) -> BOOL {
    let pids = &*(pids.0 as *const Vec<u32>);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let top_level = GetWindow(hwnd, GW_OWNER).is_err();
    if pids.contains(&pid) && top_level && IsWindowVisible(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    BOOL(1)
}

/// --minimized still puts a window on the taskbar: hide Spotify's windows
/// the moment they show, for as long as this copy is ours.
fn keep_hidden(generation: u64) {
    std::thread::spawn(move || {
        while GENERATION.load(Ordering::SeqCst) == generation && OPENED.lock().unwrap().is_some() {
            let pids = spotify_pids();
            unsafe {
                let _ = EnumWindows(Some(hide_one), LPARAM(&pids as *const Vec<u32> as isize));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

pub fn is_open() -> bool {
    OPENED.lock().unwrap().is_some()
}

/// Starts Spotify hidden, unless it already runs. True when one is (now)
/// running for us.
pub fn open() -> bool {
    let mut opened = OPENED.lock().unwrap();
    if opened.is_some() {
        return true;
    }
    let Some(exe) = exe() else { return false };
    if !spotify_pids().is_empty() {
        return false;
    }
    let Ok(child) = Command::new(exe).args(ARGS).creation_flags(CREATE_NO_WINDOW).spawn() else { return false };
    *opened = Some(child.id());
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    drop(opened);
    eprintln!("[quick-open] started Spotify hidden");
    keep_hidden(generation);
    std::thread::spawn(move || {
        std::thread::sleep(MAX_OPEN);
        if GENERATION.load(Ordering::SeqCst) == generation {
            close();
        }
    });
    true
}

/// Closes the copy this started, and everything it started.
pub fn close() {
    let Some(pid) = OPENED.lock().unwrap().take() else { return };
    eprintln!("[quick-open] closing Spotify");
    let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(CREATE_NO_WINDOW).spawn();
}

/// The user took this Spotify over (it plays): it stays.
pub fn release() {
    OPENED.lock().unwrap().take();
}
