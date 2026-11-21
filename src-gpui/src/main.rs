//! Playback, GPUI build (Project Phoenix). Slice 3a: the widget window shell and its proofs.
//!
//! Flags (all test flags keep the window off-screen):
//!   --fake-state <file>             parse the fake player state and print a summary line
//!   --snapshot <scene> <out.png>    render the scene off-screen, write a PNG, exit
//!   --frame-log <file> [--resize off|shell|every-frame]   frame-driver proof, 350 ms tweens
//!   --selftest-shell                drive and assert every shell behaviour, exit non-zero on failure
//!   --selftest-cards                the same for the card windows, launch order and quit rules (slice 3b)
//!   --instance-tag <tag>            single-instance name suffix for tests

mod fake_state;
mod frame_driver;
mod host;
mod instance;
mod shell;
mod win32;

use gpui::Application;
use shell::windows::{Launch, Windows, FULLSCREEN_MODS, FULLSCREEN_VK};
use shell::widget::ResizeMode;
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
        if flag("--snapshot").is_none() && flag("--selftest-shell").is_none() && flag("--selftest-cards").is_none() && flag("--frame-log").is_none() {
            return;
        }
    }

    // the 8 ms drag poll needs 1 ms timers
    win32::raise_timer_resolution();

    let selftest = flag("--selftest-shell").is_some();
    let selftest_cards = flag("--selftest-cards").is_some();
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
    let test_mode = selftest || selftest_cards || snapshot.is_some() || framelog.is_some();
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
                if selftest_cards {
                    shell::selftest_cards::last_window_closed();
                }
                cx.quit();
            }
        })
        .detach();
        if selftest {
            shell::selftest::run(cx, test_tag);
        } else if selftest_cards {
            shell::selftest_cards::run(cx, test_tag);
        } else if let Some((scene, out)) = snapshot {
            shell::snapshot::run(cx, scene, out, &test_tag);
        } else if let Some(path) = framelog {
            shell::framelog::run(cx, path, mode, &test_tag);
        } else {
            real_launch(cx, real_wake, gpui_host.clone());
        }
    });
}

/// The real app: the launch order of electron/main.cjs:1535-1572 over the shared backend. The backend runs on
/// its own runtime thread (`playback_core::rt`), its answers come back over channels.
fn real_launch(cx: &mut gpui::App, wake: Option<std::sync::mpsc::Receiver<()>>, host: Arc<host::GpuiHost>) {
    use playback_core::{spicetify::setup::Setup, support::paths::data_file, updates::Updater};
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("backend runtime");
        let _ = tx.send(rt.handle().clone());
        rt.block_on(std::future::pending::<()>());
    });
    playback_core::rt::init(rx.recv().expect("backend runtime"));

    let windows = Windows::new(false, env!("CARGO_PKG_VERSION").to_string(), data_file("last-version.json"), true);
    let first_run = windows.first_run();
    if let Some(w) = wake {
        windows.start_wake(cx, w);
    }
    // electron/main.cjs:1792: a combination another program holds is not an error
    windows.register_fullscreen_shortcut(cx, FULLSCREEN_MODS, FULLSCREEN_VK);
    windows.launch(
        cx,
        Launch {
            first_run,
            start_setup: Box::new(move || {
                let (tx, rx) = async_channel::bounded(1);
                playback_core::rt::spawn(async move {
                    let _ = tx.send(Setup::new(first_run).run(first_run).await.worth_showing()).await;
                });
                rx
            }),
            start_update: Box::new(move || {
                let (tx, rx) = async_channel::bounded(1);
                playback_core::rt::spawn(async move {
                    let state = Updater::default().check(&*host).await;
                    let v = state.available().filter(|v| !Updater::is_skipped(v)).map(str::to_string);
                    let _ = tx.send(v).await;
                });
                rx
            }),
        },
    );
}
