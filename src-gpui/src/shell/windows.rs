//! Every window of the app and the rules between them: what electron/main.cjs keeps in `win`, `settingsWin`,
//! `changelogWin`, `pastChangelogWin`, `setupWin`, `updateWin` and `versionsWin`, its `open*` functions, the
//! window-level IPC handlers, the launch order and the quit rules. No UI: the cards are placeholders
//! (`cards.rs`), the behaviour around them is the real thing.

use super::cards::{open_card_window, CardTrace, CardView};
use super::placement::{self, Anchors, CardKind};
use super::widget::{open_widget, Opts, ResizeMode, Shell, Widget};
use crate::win32::{self, Rect};
use async_channel::{Receiver, Sender};
use gpui::{App, WindowHandle};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

/// Quits seen by every `Windows` of the process (read by the self-test's last-window hook).
pub static QUITS: AtomicU32 = AtomicU32::new(0);

const WS_EX_TOPMOST: u32 = 0x0000_0008;
// electron/main.cjs:1792: CommandOrControl+Alt+F
pub const FULLSCREEN_MODS: u32 = win32::MOD_CONTROL | win32::MOD_ALT;
pub const FULLSCREEN_VK: u32 = 0x46;

pub struct CardEntry {
    pub handle: WindowHandle<CardView>,
    pub shell: Rc<Shell>,
    pub hwnd: isize,
}

/// What a launch needs from outside: the two answers electron/main.cjs waits for, started when it asks.
pub struct Launch {
    /// `!fs.existsSync(last-version.json)` (electron/main.cjs:1538).
    pub first_run: bool,
    /// `runSetup(firstRun)`: true when `shouldShowSetup`. Closing the channel is the `.catch` (setup settles).
    pub start_setup: Box<dyn FnOnce() -> Receiver<bool>>,
    /// `updater.check()`: the version when it is available and not skipped. Started once Setup is out of the way.
    pub start_update: Box<dyn FnOnce() -> Receiver<Option<String>>>,
}

#[derive(Default)]
struct St {
    widget: Option<(WindowHandle<Widget>, Rc<Shell>)>,
    cards: HashMap<Key, CardEntry>,
    /// Work areas a test injects (`screen.getDisplayNearestPoint` otherwise).
    areas: Option<Vec<Rect>>,
    settled: bool,
    settled_tx: Option<Sender<()>>,
    quitting: bool,
    quit_hook: Option<Rc<dyn Fn()>>,
    hotkey: Option<win32::Hotkey>,
    /// Launch order: what opened, in the order it did.
    log: Vec<String>,
    font_forgets: u32,
    quits: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key(CardKind);

pub struct Windows {
    pub test: bool,
    pub resize_mode: ResizeMode,
    /// `app.getVersion()`
    pub version: String,
    /// `<userData>/last-version.json`
    pub last_version_file: PathBuf,
    /// The "Show What's new after an update" setting (`changelog:check`'s argument).
    pub show_changelog: bool,
    st: RefCell<St>,
}

// encodeURIComponent, for `changelog?from=<version>` (electron/main.cjs:1247)
fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Windows {
    pub fn new(test: bool, version: String, last_version_file: PathBuf, show_changelog: bool) -> Rc<Self> {
        Rc::new(Self {
            test,
            resize_mode: if test { ResizeMode::Off } else { ResizeMode::Shell },
            version,
            last_version_file,
            show_changelog,
            st: RefCell::new(St::default()),
        })
    }

    /// electron/main.cjs:1538: no last-version.json yet, a fresh install.
    pub fn first_run(&self) -> bool {
        !self.last_version_file.exists()
    }

    // ── state the tests read ─────────────────────────────────────────────────────────────────

    pub fn log(&self) -> Vec<String> {
        self.st.borrow().log.clone()
    }
    fn note(&self, s: impl Into<String>) {
        self.st.borrow_mut().log.push(s.into());
    }
    pub fn is_open(&self, kind: CardKind) -> bool {
        self.st.borrow().cards.contains_key(&Key(kind))
    }
    pub fn card(&self, kind: CardKind) -> Option<(Rc<Shell>, isize)> {
        self.st.borrow().cards.get(&Key(kind)).map(|e| (e.shell.clone(), e.hwnd))
    }
    pub fn widget(&self) -> Option<(WindowHandle<Widget>, Rc<Shell>)> {
        self.st.borrow().widget.clone()
    }
    pub fn card_trace(&self, cx: &mut App, kind: CardKind) -> Option<CardTrace> {
        let h = self.st.borrow().cards.get(&Key(kind))?.handle;
        h.update(cx, |v, _, _| v.trace.clone()).ok()
    }
    pub fn with_card<R>(&self, cx: &mut App, kind: CardKind, f: impl FnOnce(&mut CardView, &mut gpui::Context<CardView>) -> R) -> Option<R> {
        let h = self.st.borrow().cards.get(&Key(kind))?.handle;
        h.update(cx, |v, _, cx| f(v, cx)).ok()
    }
    pub fn font_forgets(&self) -> u32 {
        self.st.borrow().font_forgets
    }
    pub fn quits(&self) -> u32 {
        self.st.borrow().quits
    }
    pub fn has_hotkey(&self) -> bool {
        self.st.borrow().hotkey.is_some()
    }
    pub fn set_fake_areas(&self, areas: Option<Vec<Rect>>) {
        self.st.borrow_mut().areas = areas;
    }
    pub fn set_quit_hook(&self, hook: Option<Rc<dyn Fn()>>) {
        self.st.borrow_mut().quit_hook = hook;
    }

    // ── placement ────────────────────────────────────────────────────────────────────────────

    /// `screen.getDisplayNearestPoint({ x, y }).workArea`
    fn area_near(&self, x: i32, y: i32) -> Rect {
        let fake = self.st.borrow().areas.clone();
        match fake {
            Some(a) if !a.is_empty() => placement::nearest(&a, x, y).unwrap_or(a[0]),
            _ => win32::monitor_near_point(x, y).1,
        }
    }

    fn anchors(&self, scale: f32) -> Anchors {
        let st = self.st.borrow();
        let widget = st.widget.as_ref().map(|(_, s)| win32::visible_rect(s.hwnd));
        let outer = st.widget.as_ref().map_or(0, |(_, s)| placement::px(s.widget_outer_w(), scale));
        let changelog = st.cards.get(&Key(CardKind::Changelog)).map(|e| win32::visible_rect(e.hwnd));
        Anchors { widget, widget_outer_w: outer, changelog, scale }
    }

    /// Where `kind` would open now, for a display scale.
    pub fn position_for(&self, kind: CardKind, scale: f32) -> (i32, i32) {
        let a = self.anchors(scale);
        let (ax, ay) = placement::anchor_point(kind, &a);
        placement::place(kind, &a, self.area_near(ax, ay))
    }

    fn widget_topmost(&self) -> Option<bool> {
        self.st.borrow().widget.as_ref().map(|(_, s)| win32::ex_style(s.hwnd) & WS_EX_TOPMOST != 0)
    }

    // ── open / close ─────────────────────────────────────────────────────────────────────────

    /// `openSettings`, `openChangelog`, … Opening an open card only focuses it. `from` is the changelog's
    /// `?from=<version>`.
    pub fn open_card(self: &Rc<Self>, cx: &mut App, kind: CardKind, from: Option<&str>) {
        let existing = self.st.borrow().cards.get(&Key(kind)).map(|e| e.handle);
        if let Some(h) = existing {
            let _ = h.update(cx, |v, window, _| v.focus(window));
            return;
        }
        if self.st.borrow().quitting {
            return;
        }
        // electron/main.cjs:1109: Settings is topmost when the widget is (or when there is none); the rest always
        let topmost = if kind.follows_widget_topmost() { self.widget_topmost().unwrap_or(true) } else { true };
        let hash = match (kind, from) {
            (CardKind::Changelog, Some(v)) => format!("changelog?from={}", encode_uri_component(v)),
            _ => kind.route().to_string(),
        };
        let me = self.clone();
        let place = Box::new(move |scale: f32| me.position_for(kind, scale));
        let (handle, shell) = open_card_window(cx, self, kind, Some(hash), place, topmost, self.test);
        let hwnd = shell.hwnd;
        if self.test {
            // tests never read the real mouse
            shell.cursor.set_fake(Some((-1, -1)));
        }
        self.st.borrow_mut().cards.insert(Key(kind), CardEntry { handle, shell, hwnd });
        self.note(format!("open:{}", kind.route()));
        if kind == CardKind::Changelog {
            self.send_changelog_open(cx);
        }
    }

    /// `win.close()` of a card: `closeSettings`, `changelog:close`.
    pub fn close_card(self: &Rc<Self>, cx: &mut App, kind: CardKind) {
        let entry = self.st.borrow().cards.get(&Key(kind)).map(|e| (e.handle, e.hwnd));
        let Some((h, _)) = entry else { return };
        self.before_close(cx, kind);
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }

    /// The `closed` handlers that must run before the window is gone: Setup's `setupDone()` opens the widget
    /// first, so there is never a moment with no window (GPUI ends the app with its last one).
    pub fn before_close(self: &Rc<Self>, cx: &mut App, kind: CardKind) {
        if kind == CardKind::Setup {
            self.settle(cx);
        }
    }

    /// The card's window is gone (release of its view): electron/main.cjs `closed` handlers.
    pub fn on_card_closed(self: &Rc<Self>, cx: &mut App, kind: CardKind, hwnd: isize) {
        {
            let mut st = self.st.borrow_mut();
            if st.cards.get(&Key(kind)).map(|e| e.hwnd) == Some(hwnd) {
                st.cards.remove(&Key(kind));
            }
        }
        self.note(format!("closed:{}", kind.route()));
        match kind {
            // electron/main.cjs:1130 (the next list re-reads the system fonts)
            CardKind::Settings => {
                playback_core::fonts::forget();
                self.st.borrow_mut().font_forgets += 1;
            }
            // electron/main.cjs:1240
            CardKind::Changelog => self.send_changelog_open(cx),
            // electron/main.cjs:1455
            CardKind::Setup => self.settle(cx),
            _ => {}
        }
    }

    // electron/main.cjs:1126 (system-context-menu on Settings' header) and src/windows/Settings.tsx:1343
    pub fn right_click_close(self: &Rc<Self>, cx: &mut App, kind: CardKind) {
        let me = self.clone();
        // the click is being handled by the window that is to close
        cx.defer(move |cx| me.close_card(cx, kind));
    }

    // electron/main.cjs:1192 `sendChangelogOpen`
    fn send_changelog_open(&self, cx: &mut App) {
        let open = self.is_open(CardKind::Changelog);
        let _ = self.with_card(cx, CardKind::Settings, |v, _| v.trace.changelog_open = Some(open));
    }

    // ── the IPC handlers that touch windows ──────────────────────────────────────────────────

    /// `settings:toggle`
    pub fn settings_toggle(self: &Rc<Self>, cx: &mut App) {
        if self.is_open(CardKind::Settings) {
            self.close_card(cx, CardKind::Settings);
        } else {
            self.open_card(cx, CardKind::Settings, None);
        }
    }

    /// `win:setAlwaysOnTop`: the widget and Settings; every other card is always on top.
    pub fn set_always_on_top(self: &Rc<Self>, cx: &mut App, on: bool) {
        let widget = self.st.borrow().widget.as_ref().map(|(_, s)| s.clone());
        if let Some(s) = widget {
            s.set_always_on_top(cx, on);
        }
        if let Some((s, _)) = self.card(CardKind::Settings) {
            s.set_always_on_top(cx, on);
        }
    }

    /// `win:close` (the widget's close button) and every other `app.quit()`.
    pub fn app_quit(self: &Rc<Self>, cx: &mut App) {
        self.quit(cx);
    }

    // ── launch ───────────────────────────────────────────────────────────────────────────────

    fn create_widget(self: &Rc<Self>, cx: &mut App) {
        let (handle, shell, _driver) = open_widget(cx, Opts { test: self.test, wake: None, resize_mode: self.resize_mode });
        if self.test {
            shell.cursor.set_fake(Some((-1, -1)));
        }
        self.st.borrow_mut().widget = Some((handle, shell));
        self.note("widget");
        // electron/main.cjs:1051: `win.on("closed")` quits
        if let Ok(ent) = handle.entity(cx) {
            let me = self.clone();
            cx.observe_release(&ent, move |_, cx| me.on_widget_closed(cx)).detach();
        }
    }

    fn on_widget_closed(self: &Rc<Self>, cx: &mut App) {
        self.st.borrow_mut().widget = None;
        self.note("closed:widget");
        if !self.st.borrow().quitting {
            self.quit(cx);
        }
    }

    /// Setup is out of the way (not needed, or closed): `setupDone()`. The widget is created, its page
    /// checks What's new, and whoever waits for the settle goes on. Once.
    pub fn settle(self: &Rc<Self>, cx: &mut App) {
        {
            let mut st = self.st.borrow_mut();
            if st.settled || st.quitting {
                return;
            }
            st.settled = true;
        }
        // electron/main.cjs:1552
        self.create_widget(cx);
        // the widget page's `changelog:check` (src/App.tsx), once it has loaded
        self.changelog_check(cx, self.show_changelog);
        if let Some(tx) = self.st.borrow().settled_tx.clone() {
            let _ = tx.try_send(());
        }
    }

    /// `changelog:check(enabled)` (electron/main.cjs:1335): records this version, and opens What's new from the
    /// previous one when the setting is on and a different version was recorded. Never on a first run.
    pub fn changelog_check(self: &Rc<Self>, cx: &mut App, enabled: bool) {
        let previous = std::fs::read_to_string(&self.last_version_file)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["version"].as_str().map(str::to_string));
        let current = self.version.clone();
        if let Err(e) = std::fs::write(&self.last_version_file, serde_json::json!({ "version": current }).to_string()) {
            eprintln!("[changelog] could not record the version: {e}");
        }
        if let Some(prev) = previous.filter(|p| !p.is_empty()) {
            if enabled && prev != current {
                self.open_card(cx, CardKind::Changelog, Some(&prev));
            }
        }
    }

    /// electron/main.cjs:1535-1572, in order: Setup at once on a first run; then what `runSetup` says; the widget
    /// once Setup has settled; then the update check, whose answer opens the update prompt.
    pub fn launch(self: &Rc<Self>, cx: &mut App, l: Launch) {
        let (tx, settled) = async_channel::bounded::<()>(1);
        self.st.borrow_mut().settled_tx = Some(tx);
        let me = self.clone();
        cx.spawn(async move |acx| {
            if l.first_run {
                let _ = acx.update(|cx| me.open_card(cx, CardKind::Setup, None));
            }
            let need = (l.start_setup)().recv().await.unwrap_or(false);
            let _ = acx.update(|cx| {
                if need {
                    me.open_card(cx, CardKind::Setup, None);
                } else {
                    me.settle(cx);
                }
            });
            let _ = settled.recv().await;
            me.note("update-check");
            if let Ok(Some(_)) = (l.start_update)().recv().await {
                let _ = acx.update(|cx| me.open_card(cx, CardKind::Update, None));
            }
        })
        .detach();
    }

    // ── second launch, global shortcut, quit ─────────────────────────────────────────────────

    /// Polls the second-launch signal of `instance::acquire` (one display frame at 60 Hz, port.md 7.6).
    pub fn start_wake(self: &Rc<Self>, cx: &mut App, rx: std::sync::mpsc::Receiver<()>) {
        let me = self.clone();
        cx.spawn(async move |acx| loop {
            acx.background_executor().timer(Duration::from_millis(16)).await;
            if me.st.borrow().quitting {
                break;
            }
            let mut n = 0;
            while rx.try_recv().is_ok() {
                n += 1;
            }
            for _ in 0..n {
                let _ = acx.update(|cx| me.second_instance(cx));
            }
        })
        .detach();
    }

    /// electron/main.cjs:59: before the widget exists Setup is brought forward; else the widget is.
    pub fn second_instance(self: &Rc<Self>, cx: &mut App) {
        let widget = self.st.borrow().widget.as_ref().map(|(h, _)| *h);
        match widget {
            None => {
                let setup = self.st.borrow().cards.get(&Key(CardKind::Setup)).map(|e| e.handle);
                if let Some(h) = setup {
                    let _ = h.update(cx, |v, window, _| v.focus(window));
                }
            }
            Some(h) => {
                let _ = h.update(cx, |w, window, _| w.on_second_instance(window));
            }
        }
    }

    /// `globalShortcut.register` (electron/main.cjs:1792). The press is handed to the UI thread, where the widget
    /// decides. False when another program holds the combination (Electron goes on without it).
    pub fn register_fullscreen_shortcut(self: &Rc<Self>, cx: &mut App, mods: u32, vk: u32) -> bool {
        let (tx, rx) = async_channel::unbounded::<()>();
        let Some(hk) = win32::Hotkey::register(mods, vk, move || {
            let _ = tx.try_send(());
        }) else {
            return false;
        };
        self.st.borrow_mut().hotkey = Some(hk);
        let me = self.clone();
        cx.spawn(async move |acx| {
            while rx.recv().await.is_ok() {
                let _ = acx.update(|cx| me.fullscreen_toggle(cx));
            }
        })
        .detach();
        true
    }

    /// Posts the press to the hotkey thread (tests: the keys themselves are never pressed).
    pub fn press_shortcut(&self) {
        if let Some(h) = self.st.borrow().hotkey.as_ref() {
            h.post_press();
        }
    }

    // `win?.webContents.send("fullscreen:toggle")`
    fn fullscreen_toggle(self: &Rc<Self>, cx: &mut App) {
        let widget = self.st.borrow().widget.as_ref().map(|(h, _)| *h);
        if let Some(h) = widget {
            let _ = h.update(cx, |w, _, cx| w.on_fullscreen_toggle(cx));
        }
    }

    /// `app.quit()`: shortcuts released (`before-quit`), every window closed, the app ends.
    pub fn quit(self: &Rc<Self>, cx: &mut App) {
        let (hotkey, cards, widget, hook) = {
            let mut st = self.st.borrow_mut();
            st.quitting = true;
            st.quits += 1;
            let cards: Vec<_> = st.cards.values().map(|e| e.handle).collect();
            (st.hotkey.take(), cards, st.widget.as_ref().map(|(h, _)| *h), st.quit_hook.clone())
        };
        QUITS.fetch_add(1, Ordering::SeqCst);
        // electron/main.cjs:1860
        if let Some(h) = hotkey {
            h.unregister();
        }
        for h in cards {
            let _ = h.update(cx, |_, window, _| window.remove_window());
        }
        if let Some(h) = widget {
            let _ = h.update(cx, |_, window, _| window.remove_window());
        }
        match hook {
            Some(h) => h(),
            None => cx.quit(),
        }
    }
}
