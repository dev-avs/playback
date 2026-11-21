//! Playback, GPUI build (Project Phoenix). Slice 3a: the widget window shell and its proofs.
//!
//! Flags (all test flags keep the window off-screen):
//!   --fake-state <file>             parse the fake player state and print a summary line
//!   --snapshot <scene> <out.png>    render the scene off-screen, write a PNG, exit
//!   --frame-log <file> [--resize off|shell|every-frame]   frame-driver proof, 350 ms tweens
//!   --selftest-shell                drive and assert every shell behaviour, exit non-zero on failure
//!   --instance-tag <tag>            single-instance name suffix for tests

mod fake_state;
mod frame_driver;
mod host;
mod instance;
mod shell;
mod win32;

use gpui::Application;
use shell::widget::{open_widget, Opts, ResizeMode};
use std::{path::PathBuf, sync::atomic::AtomicBool, sync::atomic::Ordering, sync::Arc};

/// Set when the last window closed and the app asked to quit (read by `--selftest-shell`).
pub static QUIT_ON_CLOSE: AtomicBool = AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let value = |name: &str| flag(name).and_then(|i| args.get(i + 1)).cloned();
    let tag = value("--instance-tag").unwrap_or_default();

    let gpui_host = Arc::new(host::GpuiHost::default());
    if let Some(p) = value("--fake-state") {
        match fake_state::FakeState::load(&PathBuf::from(&p)) {
            Ok(s) => {
                println!("{}", s.summary());
                *gpui_host.fake_latest.lock().unwrap() = serde_json::to_value(&s.state).ok();
            }
            Err(e) => {
                eprintln!("fake-state: error: {e}");
                std::process::exit(2);
            }
        }
        if flag("--snapshot").is_none() && flag("--selftest-shell").is_none() && flag("--frame-log").is_none() {
            return;
        }
    }

    // the 8 ms drag poll needs 1 ms timers
    win32::raise_timer_resolution();

    let selftest = flag("--selftest-shell").is_some();
    let snapshot = flag("--snapshot").map(|i| {
        (args.get(i + 1).cloned().unwrap_or_default(), PathBuf::from(args.get(i + 2).cloned().unwrap_or_default()))
    });
    let framelog = value("--frame-log").map(PathBuf::from);
    let mode = match value("--resize").as_deref() {
        Some("off") => ResizeMode::Off,
        Some("every-frame") => ResizeMode::EveryFrame,
        _ => ResizeMode::Shell,
    };

    // the real app is single-instance; test flags use their own tag so they never meet a running app
    let test_mode = selftest || snapshot.is_some() || framelog.is_some();
    let real_wake = if test_mode {
        None
    } else {
        match instance::acquire(&tag) {
            instance::Instance::First(rx) => Some(rx),
            instance::Instance::Second => return,
        }
    };
    let test_tag = if tag.is_empty() { format!("test-{}", std::process::id()) } else { tag.clone() };

    Application::new().run(move |cx| {
        // electron/main.cjs:1051 and 1869: closing the widget quits the app
        cx.on_window_closed(move |cx| {
            if cx.windows().is_empty() {
                QUIT_ON_CLOSE.store(true, Ordering::SeqCst);
                // GPUI ends the process when the last window is gone, so the self-test prints here
                if selftest {
                    shell::selftest::last_window_closed();
                }
                cx.quit();
            }
        })
        .detach();
        if selftest {
            shell::selftest::run(cx, test_tag);
        } else if let Some((scene, out)) = snapshot {
            shell::snapshot::run(cx, scene, out, &test_tag);
        } else if let Some(path) = framelog {
            shell::framelog::run(cx, path, mode, &test_tag);
        } else {
            let _ = open_widget(cx, Opts { test: false, wake: real_wake, resize_mode: ResizeMode::Shell });
        }
    });
}
