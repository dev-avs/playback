//! A launch, in electron/main.cjs's order: Setup first when there is
//! something to do there, then the widget, then What's new and the update
//! prompt, so no two of them open on top of each other.
//!
//! The widget's window is built at once (hidden; the cards open beside it and
//! its page starts the media and lyrics work), and shown when launch settles:
//! straight away when Setup has nothing to say, or when Setup is closed.

use crate::migrate;
use crate::spicetify::setup::Setup;
use crate::support::paths::data_file;
use crate::ui::cards::{self, Card};
use crate::ui::widget::Widget;
use crate::updates::Updater;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::watch;

pub struct Launch {
    settled: watch::Sender<bool>,
}

impl Launch {
    /// Setup is out of the way (closed, or not needed).
    pub fn settle(&self) {
        self.settled.send_replace(true);
    }

    pub fn is_settled(&self) -> bool {
        *self.settled.borrow()
    }

    /// Waits until launch has settled.
    pub async fn settled(&self) {
        let _ = self.settled.subscribe().wait_for(|s| *s).await;
    }
}

/// Builds the widget (tauri.conf.json's window, with the settings carried
/// over from the Electron build) and starts the launch. Runs in `setup`.
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    // nothing recorded yet: a fresh install. Read before the widget's page
    // records this version.
    let first_run = !data_file("last-version.json").exists();
    app.manage(Setup::new(first_run));
    app.manage(Launch {
        settled: watch::Sender::new(false),
    });
    build_widget(app)?;

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let launch = handle.state::<Launch>();
        // a first run shows Setup at once, so the Spotify restart is explained
        if first_run {
            open(&handle, Card::Setup);
        }
        // only a first run applies Spicetify on its own (it restarts Spotify);
        // later launches leave that to Setup's Apply
        let state = handle.state::<Setup>().run(first_run).await;
        if state.worth_showing() && !launch.is_settled() {
            open(&handle, Card::Setup);
        } else if !cards::is_open(&handle, Card::Setup) {
            launch.settle();
        }
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        handle.state::<Launch>().settled().await;
        if let Some(win) = widget(&handle) {
            handle.state::<Widget>().reveal(&win);
        }
        // asked once per launch, unless the user skipped this version
        let state = handle.state::<Updater>().check(&handle).await;
        if state.available().is_some_and(|v| !Updater::is_skipped(v)) {
            open(&handle, Card::Update);
        }
    });
    Ok(())
}

fn build_widget(app: &AppHandle) -> tauri::Result<()> {
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == crate::ui::WIDGET)
    else {
        return Ok(());
    };
    let mut builder = WebviewWindowBuilder::from_config(app, config)?.on_page_load(|win, load| {
        if matches!(load.event(), tauri::webview::PageLoadEvent::Finished) {
            migrate::settings_imported(win.app_handle());
        }
    });
    if let Some(script) = migrate::settings_script(app) {
        builder = builder.initialization_script(script);
    }
    builder.build()?;
    Ok(())
}

fn widget(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(crate::ui::WIDGET)
}

fn open(app: &AppHandle, card: Card) {
    if let Err(e) = cards::open(app, card, None) {
        eprintln!("[launch] could not open {card:?}: {e}");
    }
}

/// A second launch (a shortcut, the Start menu, the installer's run after
/// finishing) brings this one forward instead: Setup while it is up, else the
/// widget.
pub fn second_instance(app: &AppHandle) {
    if !app.state::<Launch>().is_settled() {
        if let Some(setup) = app.get_webview_window("setup") {
            let _ = setup.set_focus();
        }
        return;
    }
    if let Some(win) = widget(app) {
        let _ = win.unminimize();
        app.state::<Widget>().set_hidden(&win, false);
        let _ = win.set_focus();
    }
}
