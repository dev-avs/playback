//! Single instance (electron/main.cjs:56-65, H.single-instance / H.second-instance).
//!
//! The first process owns a named mutex and a named event; a second launch signals the event and
//! exits 0 before anything starts. The first process's poll loop sees the signal and brings the
//! widget forward (restore if minimized, show, focus).

use std::sync::mpsc::{channel, Receiver};
use windows::core::HSTRING;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};

pub enum Instance {
    /// This process is the app. Each `()` is a second launch that asked to be brought forward.
    First(Receiver<()>),
    /// Another process is the app and has been told; this one must exit 0.
    Second,
}

/// `tag` is "" for the real app; tests use their own so they never meet a running app.
pub fn acquire(tag: &str) -> Instance {
    let base = if tag.is_empty() { "Local\\Playback.Gpui".to_string() } else { format!("Local\\Playback.Gpui.{tag}") };
    unsafe {
        let mutex = CreateMutexW(None, true, &HSTRING::from(base.as_str()));
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        // an auto-reset event, created by whichever side gets there first
        let ev = CreateEventW(None, false, false, &HSTRING::from(format!("{base}.wake")));
        if mutex.is_err() || ev.is_err() {
            // cannot tell; run rather than refuse to start
            let (_tx, rx) = channel();
            return Instance::First(rx);
        }
        let ev = ev.unwrap();
        if already {
            let _ = SetEvent(ev);
            return Instance::Second;
        }
        let (tx, rx) = channel();
        let raw = ev.0 as isize;
        std::thread::spawn(move || loop {
            let h = HANDLE(raw as *mut _);
            if WaitForSingleObject(h, INFINITE).0 != 0 || tx.send(()).is_err() {
                break;
            }
        });
        // the mutex handle stays open for the life of the process
        std::mem::forget(mutex);
        Instance::First(rx)
    }
}
