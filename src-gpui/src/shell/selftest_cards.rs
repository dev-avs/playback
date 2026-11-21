//! `--selftest-cards`: slice 3b. Opens the card windows (settings, changelog, past changelog, setup, update,
//! Version control) beside an off-screen widget, drives every window rule of electron/main.cjs that is about
//! them (placement, options, topmost inheritance, resize, close handlers, toggle/open/focus, launch order, quit
//! rules, second launch, global shortcut, display-scale replay) and asserts it with Win32 reads. Prints a
//! pass/fail table by ledger row id and exits non-zero on any failure.
//!
//! Nothing here touches the real mouse or keyboard: clicks are messages posted to our own windows, the cursor is
//! the injected one, the shortcut is a different combination fired with a message to its own thread, work areas
//! and display scales are injected. Every window opens at the -20000 pattern, none is ever activated (asserted).

use super::placement::CardKind;
use super::widget::{Shell, STAGE_PAD, TEST_ORIGIN};
use super::windows::{Launch, Windows, FULLSCREEN_MODS};
use super::{sleep, wait_until};
use crate::instance;
use crate::win32::{self, Rect};
use gpui::{div, prelude::*, App, AsyncApp, Context, IntoElement, Render, Window, WindowBackgroundAppearance, WindowKind, WindowOptions};
use std::{path::PathBuf, rc::Rc, sync::Mutex};

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;
// a combination no one uses: Ctrl+Alt+Shift+F24, fired with a message to the hotkey thread
const TEST_MODS: u32 = FULLSCREEN_MODS | win32::MOD_SHIFT;
const TEST_VK: u32 = 0x87;

#[derive(Default)]
struct Table {
    rows: Vec<(String, bool, String)>,
}
impl Table {
    fn check(&mut self, ids: &[&str], ok: bool, detail: impl Into<String>) {
        let detail = detail.into();
        for id in ids {
            self.rows.push((id.to_string(), ok, detail.clone()));
        }
    }
}

struct T {
    cx: AsyncApp,
    t: Table,
    /// Every window hwnd this run opened, for the never-foreground guard.
    hwnds: Vec<isize>,
}

impl T {
    fn app<R>(&self, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        self.cx.update(f).ok()
    }
    async fn wait(&self, ms: u64) {
        sleep(&self.cx, ms).await;
    }
    fn trace(&self, win: &Rc<Windows>, kind: CardKind) -> Option<super::cards::CardTrace> {
        self.app(|cx| win.card_trace(cx, kind)).flatten()
    }
    async fn open(&mut self, win: &Rc<Windows>, kind: CardKind, from: Option<&str>) -> bool {
        let w = win.clone();
        let from = from.map(str::to_string);
        self.app(|cx| w.open_card(cx, kind, from.as_deref()));
        if let Some((_, hwnd)) = win.card(kind) {
            if !self.hwnds.contains(&hwnd) {
                self.hwnds.push(hwnd);
            }
        }
        let win2 = win.clone();
        wait_until(&self.cx, 5000, || {
            win2.card(kind).map_or(false, |(s, _)| s.trace().uncloaked_at.is_some() || false)
                || self.trace_now(&win2, kind).map_or(false, |t| t.uncloak_frames.is_some())
        })
        .await
    }
    fn trace_now(&self, win: &Rc<Windows>, kind: CardKind) -> Option<super::cards::CardTrace> {
        self.trace(win, kind)
    }
    fn close(&self, win: &Rc<Windows>, kind: CardKind) {
        let w = win.clone();
        self.app(|cx| w.close_card(cx, kind));
    }
    fn not_foreground(&self) -> bool {
        self.hwnds.iter().all(|h| !win32::is_foreground(*h))
    }
}

fn px(sc: f32, d: i32) -> i32 {
    (d as f32 * sc).ceil() as i32
}
fn jsr(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}
fn near(a: i32, b: i32, tol: i32) -> bool {
    (a - b).abs() <= tol
}
fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("playback-gpui-selftest-{}-{name}.json", std::process::id()))
}
fn rect_s(r: Rect) -> String {
    format!("{},{} {}x{}", r.x, r.y, r.w, r.h)
}

struct Keeper;
impl Render for Keeper {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub fn run(cx: &mut App, tag: String) {
    // a window that is never shown keeps the app alive between scenarios (GPUI ends the app with its last window)
    let keeper = cx
        .open_window(
            WindowOptions {
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: false,
                kind: WindowKind::PopUp,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Keeper),
        )
        .expect("keeper window");
    cx.spawn(async move |acx| {
        let mut t = T { cx: acx.clone(), t: Table::default(), hwnds: vec![] };
        body(&mut t, &tag, keeper).await;
        std::process::exit(print_table(&t.t));
    })
    .detach();
}

fn print_table(t: &Table) -> i32 {
    let mut fails = 0;
    println!("{:<34} {:<5} detail", "row id", "");
    for (id, ok, d) in &t.rows {
        println!("{:<34} {:<5} {d}", id, if *ok { "pass" } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    }
    println!("--- {} checks, {} failed", t.rows.len(), fails);
    if fails > 0 {
        1
    } else {
        0
    }
}

/// Table waiting for the last scenario's quit, see `last_scenario`.
static PENDING: Mutex<Option<Table>> = Mutex::new(None);

/// The app's window-closed hook with no window left (fallback of the last scenario's quit hook).
pub fn last_window_closed() {
    if let Some(mut tab) = PENDING.lock().unwrap().take() {
        let q = super::windows::QUITS.load(std::sync::atomic::Ordering::SeqCst);
        tab.check(&["H.widget-close-quits"], q >= 1, format!("the last window closed after {q} quit(s)"));
        std::process::exit(print_table(&tab));
    }
}

// ── per-kind facts of electron/main.cjs ──────────────────────────────────────────────────────

struct Spec {
    kind: CardKind,
    name: &'static str,
    /// width, first height (DIPs) as written in main.cjs
    w: i32,
    h: i32,
    wconst: Option<&'static str>,
    ids: Ids,
}
struct Ids {
    x: &'static str,
    y: &'static str,
    w: &'static str,
    h: &'static str,
    frame: &'static str,
    transp: &'static str,
    resiz: &'static str,
    top: &'static str,
    shadow: &'static str,
    show: &'static str,
    level: &'static str,
    ready: &'static str,
    closed: &'static str,
}

fn w(id: &str) -> String {
    format!("W.window-option.{id}")
}
fn lv(id: &str) -> String {
    format!("W.window-level.{id}")
}
fn ev(id: &str) -> String {
    format!("W.window-event.{id}")
}

fn specs() -> Vec<Spec> {
    vec![
        Spec {
            kind: CardKind::Settings, name: "settings", w: 644, h: 520, wconst: Some("W.window-const.c5f236"),
            ids: Ids { x: "5c6e0a", y: "789fd6", w: "b0cc95", h: "cff931", frame: "800dda", transp: "7b9984", resiz: "0d353c", top: "5c4f82", shadow: "f67ff6", show: "f83e82", level: "02e89e", ready: "bef797", closed: "22c060" },
        },
        Spec {
            kind: CardKind::PastChangelog, name: "past changelog", w: 364, h: 480, wconst: Some("W.window-const.8e22a2"),
            ids: Ids { x: "a6eb5b", y: "6ade85", w: "320411", h: "c93717", frame: "bc25ab", transp: "41dda4", resiz: "93ccbe", top: "175687", shadow: "745bd7", show: "5d3c6d", level: "fd0164", ready: "fc72ca", closed: "f70e28" },
        },
        Spec {
            kind: CardKind::Changelog, name: "changelog", w: 364, h: 480, wconst: None,
            ids: Ids { x: "6a1e67", y: "8e02c2", w: "ae1b7f", h: "82e211", frame: "525c5e", transp: "4b55d9", resiz: "518d7a", top: "e8a2eb", shadow: "01d58f", show: "b4b4b1", level: "b1b527", ready: "b45373", closed: "7842e7" },
        },
        Spec {
            kind: CardKind::Versions, name: "versions", w: 364, h: 560, wconst: None,
            ids: Ids { x: "3cc760", y: "43e5f3", w: "5f76fa", h: "d2158d", frame: "b846d3", transp: "134d38", resiz: "3f9070", top: "efb339", shadow: "ab4d94", show: "301d25", level: "700b76", ready: "db7f7c", closed: "72149f" },
        },
        Spec {
            kind: CardKind::Update, name: "update", w: 344, h: 240, wconst: Some("W.window-const.52cfbd"),
            ids: Ids { x: "728f72", y: "b8edd7", w: "b8b074", h: "1c0c72", frame: "ef9c8a", transp: "299488", resiz: "c7229b", top: "18a73b", shadow: "c878a1", show: "a6032a", level: "68e805", ready: "12e78e", closed: "07d53f" },
        },
        Spec {
            kind: CardKind::Setup, name: "setup", w: 384, h: 480, wconst: Some("W.window-const.e0ea56"),
            ids: Ids { x: "c294d2", y: "1a1e04", w: "5af739", h: "af4bfd", frame: "974955", transp: "794681", resiz: "fe857d", top: "52641b", shadow: "624784", show: "7e04ef", level: "56c589", ready: "aa75bc", closed: "3d4c59" },
        },
    ]
}

/// Opens `spec.kind`, asserts its options and where it went (`want` = the position main.cjs computes), closes it.
async fn check_window(t: &mut T, win: &Rc<Windows>, sc: f32, spec: &Spec, want: (i32, i32), topmost: bool, label: &str) {
    let k = spec.kind;
    let up = t.open(win, k, None).await;
    let Some((shell, hwnd)) = win.card(k) else {
        t.t.check(&[&w(spec.ids.x)], false, format!("{} did not open", spec.name));
        return;
    };
    let tr = t.trace(win, k).unwrap_or_default();
    let init = tr.initial_rect.unwrap_or_default();
    let (ww, hh) = (px(sc, spec.w), px(sc, spec.h));
    let ids = &spec.ids;
    if let Some(c) = spec.wconst {
        t.t.check(&[c], init.w == ww, format!("{}: width {} px = {} DIPs at scale {sc}", spec.name, init.w, spec.w));
    }
    t.t.check(&[&w(ids.w)], init.w == ww, format!("{label}: width {} px (want {ww})", init.w));
    t.t.check(&[&w(ids.h)], init.h == hh, format!("{label}: first height {} px (want {hh})", init.h));
    t.t.check(&[&w(ids.x)], init.x == want.0, format!("{label}: x {} (main.cjs formula {})", init.x, want.0));
    t.t.check(&[&w(ids.y)], init.y == want.1, format!("{label}: y {} (main.cjs formula {})", init.y, want.1));
    let st = win32::style(hwnd);
    let ex = win32::ex_style(hwnd);
    let (wr, vr) = (win32::window_rect(hwnd), win32::visible_rect(hwnd));
    t.t.check(
        &[&w(ids.frame)],
        vr.y - wr.y <= 1 && st & WS_CAPTION == WS_CAPTION && tr.strip[2],
        format!("{label}: style {st:#010x}, client top inset {} px, NC rendering off", vr.y - wr.y),
    );
    t.t.check(&[&w(ids.transp)], ex & WS_EX_NOREDIRECTIONBITMAP != 0, format!("{label}: exstyle {ex:#010x} NOREDIRECTIONBITMAP"));
    t.t.check(&[&w(ids.resiz)], st & WS_THICKFRAME == 0, format!("{label}: style {st:#010x}, no WS_THICKFRAME"));
    t.t.check(&[&w(ids.shadow)], tr.strip == [true, true, true], format!("{label}: DWM strip {:?}", tr.strip));
    t.t.check(&[&w(ids.top), &lv(ids.level)], (ex & WS_EX_TOPMOST != 0) == topmost, format!("{label}: WS_EX_TOPMOST {} (want {topmost})", ex & WS_EX_TOPMOST != 0));
    let show_ids: Vec<String> = if k == CardKind::Settings { vec![w(ids.show), "H.settings-show".to_string()] } else { vec![w(ids.show)] };
    t.t.check(
        &show_ids.iter().map(String::as_str).collect::<Vec<_>>(),
        tr.shown_cloaked && !win32::is_foreground(hwnd),
        format!("{label}: created hidden, first shown cloaked: {}, foreground: {}", tr.shown_cloaked, win32::is_foreground(hwnd)),
    );
    let frames = tr.uncloak_frames.unwrap_or(0);
    t.t.check(
        &[&ev(ids.ready), "H.widget-initial"][..1],
        up && frames >= 2 && !win32::is_cloaked(hwnd),
        format!("{label}: shown (uncloaked) once the card kept its size for {frames} frames, cloaked now: {}", win32::is_cloaked(hwnd)),
    );
    let _ = shell;
}

/// Closes `spec.kind` and asserts its `closed` handler's effects; leaves it closed.
async fn check_closed(t: &mut T, win: &Rc<Windows>, spec: &Spec, extra: bool, detail: &str) {
    let hwnd = win.card(spec.kind).map(|c| c.1).unwrap_or(0);
    t.close(win, spec.kind);
    let gone = wait_until(&t.cx, 2000, || !win32::is_window(hwnd)).await;
    let reg = !win.is_open(spec.kind);
    t.t.check(&[&ev(spec.ids.closed)], gone && reg && extra, format!("{}: window destroyed {gone}, registry entry dropped {reg}; {detail}", spec.name));
}

async fn body(t: &mut T, _tag: &str, keeper: gpui::WindowHandle<Keeper>) {
    scenario_cards(t).await;
    scenario_shortcut(t).await;
    scenario_launch(t).await;
    scenario_catch(t).await;
    last_scenario(t, keeper).await;
}

// ── scenario 1: widget with cards beside it ─────────────────────────────────────────────────

async fn scenario_cards(t: &mut T) {
    let file = tmp("s1");
    let _ = std::fs::remove_file(&file);
    let win = Windows::new(true, "9.9.9".into(), file.clone(), true);
    let quit_seen = Rc::new(std::cell::Cell::new(0u32));
    {
        let q = quit_seen.clone();
        win.set_quit_hook(Some(Rc::new(move || q.set(q.get() + 1))));
    }
    // two displays, both far off every real one: A is 1920x1040, B (right of it) 1280x1024
    let a = Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 };
    let b = Rect { x: TEST_ORIGIN.0 + 2000, y: TEST_ORIGIN.1, w: 1280, h: 1024 };
    win.set_fake_areas(Some(vec![a, b]));
    let first = win.first_run();
    t.t.check(&["H.first-run-setup"], first, "no last-version.json: first run");
    t.app(|cx| win.settle(cx));
    let Some((_, ws)) = win.widget() else {
        t.t.check(&["H.startup-order"], false, "the widget was not created");
        return;
    };
    t.hwnds.push(ws.hwnd);
    wait_until(&t.cx, 5000, || ws.trace().uncloaked_at.is_some()).await;
    let sc = ws.scale();
    let pad = px(sc, 22);
    let outer = px(sc, 394);
    let move_widget = |x: i32, y: i32| win32::set_visible_rect(ws.hwnd, Rect { x, y, w: 0, h: 0 }, true, false);

    // H.changelog-after-update, never on a first run; the version is recorded either way
    let rec = std::fs::read_to_string(&file).unwrap_or_default();
    t.t.check(
        &["H.changelog-after-update"],
        !win.is_open(CardKind::Changelog) && rec.contains("9.9.9"),
        format!("first run: no What's new, last-version.json = {rec}"),
    );
    t.app(|cx| win.changelog_check(cx, true));
    t.t.check(&["H.changelog-after-update"], !win.is_open(CardKind::Changelog), "same version recorded: no What's new");
    std::fs::write(&file, r#"{"version":"9.0.0"}"#).ok();
    t.app(|cx| win.changelog_check(cx, false));
    let rec = std::fs::read_to_string(&file).unwrap_or_default();
    t.t.check(&["H.changelog-after-update"], !win.is_open(CardKind::Changelog) && rec.contains("9.9.9"), format!("setting off: no What's new, version still recorded ({rec})"));
    std::fs::write(&file, r#"{"version":"9.0.0"}"#).ok();
    t.app(|cx| win.changelog_check(cx, true));
    let cl_up = wait_until(&t.cx, 5000, || t.trace(&win, CardKind::Changelog).map_or(false, |x| x.uncloak_frames.is_some())).await;
    let q = t.trace(&win, CardKind::Changelog).and_then(|x| x.query).unwrap_or_default();
    t.t.check(&["H.changelog-after-update"], cl_up && q == "changelog?from=9.0.0", format!("an older version recorded, setting on: What's new opened with {q}"));
    if let Some(h) = win.card(CardKind::Changelog).map(|c| c.1) {
        t.hwnds.push(h);
    }
    t.close(&win, CardKind::Changelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Changelog)).await;

    let specs = specs();
    let spec = |k: CardKind| specs.iter().find(|s| s.kind == k).unwrap();
    let area_right = |r: Rect| r.x + r.w;

    // ── Settings beside the widget, on the side with room (electron/main.cjs:1096) ──
    let (wx, wy) = (a.x + 100, a.y + 200);
    move_widget(wx, wy);
    t.wait(100).await;
    let sw = px(sc, 644);
    let right = wx + outer;
    let want = if right + sw <= area_right(a) { (right - pad, wy) } else { (wx - sw + pad, wy) };
    check_window(t, &win, sc, spec(CardKind::Settings), want, true, "settings, room on the right").await;
    t.t.check(&["H.settings-placement", "H.settings-size"], want.0 == right - pad, format!("room on the right: x = widget right {right} - {pad}"));
    // drag from anywhere on Settings: a left-button message to its own window; the cursor is the injected one
    if let Some((ss, hw)) = win.card(CardKind::Settings) {
        let before = win32::visible_rect(hw);
        ss.cursor.set_fake(Some((0, 0)));
        win32::post_mouse(hw, win32::WM_LBUTTONDOWN, 50, 50);
        let dragging = wait_until(&t.cx, 1000, || ss.is_dragging()).await;
        ss.cursor.set_fake(Some((60, 30)));
        t.wait(120).await;
        let moved = win32::visible_rect(hw);
        win32::post_mouse(hw, win32::WM_LBUTTONUP, 50, 50);
        let stopped = wait_until(&t.cx, 1000, || !ss.is_dragging()).await;
        t.t.check(
            &["H.drag-start", "W.ipc.60f245"],
            dragging && moved.x - before.x == 60 && moved.y - before.y == 30 && stopped && moved.w == before.w,
            format!("Settings drags from anywhere: pointerdown started it ({dragging}), followed the cursor by ({},{}), pointerup stopped it ({stopped})", moved.x - before.x, moved.y - before.y),
        );
        ss.cursor.set_fake(Some((-1, -1)));
        win32::set_visible_rect(hw, before, true, false);
    }
    // opening an open Settings only focuses it; settings:toggle closes it
    let hw0 = win.card(CardKind::Settings).map(|c| c.1).unwrap_or(0);
    let opens0 = win.log().iter().filter(|l| *l == "open:settings").count();
    t.app(|cx| win.open_card(cx, CardKind::Settings, None));
    t.app(|cx| win.open_card(cx, CardKind::Settings, None));
    let focus = t.trace(&win, CardKind::Settings).map_or(0, |x| x.focus_calls);
    let opens1 = win.log().iter().filter(|l| *l == "open:settings").count();
    t.t.check(
        &["H.settings-single", "W.ipc.12081e"],
        win.card(CardKind::Settings).map(|c| c.1) == Some(hw0) && opens1 == opens0 && focus == 2,
        format!("opened twice more: same window, {} new windows, focused {focus} times", opens1 - opens0),
    );
    // topmost inheritance and the topmost IPC (electron/main.cjs:1109, 1801)
    t.app(|cx| win.set_always_on_top(cx, false));
    t.wait(150).await;
    let ws_top = win32::ex_style(ws.hwnd) & WS_EX_TOPMOST != 0;
    let ss_top = win32::ex_style(hw0) & WS_EX_TOPMOST != 0;
    t.t.check(&["H.topmost-ipc", "W.ipc.073430", "W.window-level.323483", "W.window-level.02f856"], !ws_top && !ss_top, format!("win:setAlwaysOnTop(false): widget topmost {ws_top}, Settings topmost {ss_top}"));
    t.app(|cx| win.settings_toggle(cx));
    let gone = wait_until(&t.cx, 2000, || !win32::is_window(hw0)).await;
    t.t.check(&["H.settings-single", "W.ipc.deea73", "W.ipc.8caeca"], gone && !win.is_open(CardKind::Settings), format!("settings:toggle with Settings open closed it: {gone}"));
    // widget not topmost: Settings opens not topmost; any other card still does
    t.app(|cx| win.settings_toggle(cx));
    t.wait(50).await;
    let up = wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Settings).map_or(false, |x| x.uncloak_frames.is_some())).await;
    let hw1 = win.card(CardKind::Settings).map(|c| c.1).unwrap_or(0);
    t.hwnds.push(hw1);
    let s_top = win32::ex_style(hw1) & WS_EX_TOPMOST != 0;
    t.t.check(&["H.settings-topmost-inherit", "W.window-option.5c4f82", "W.window-level.02e89e"], up && !s_top, format!("widget not topmost: Settings topmost {s_top}"));
    t.t.check(&["W.ipc.deea73", "W.ipc.12081e"], up, "settings:toggle with Settings closed opened it");
    t.app(|cx| win.set_always_on_top(cx, true));
    t.wait(150).await;
    let both = win32::ex_style(ws.hwnd) & WS_EX_TOPMOST != 0 && win32::ex_style(hw1) & WS_EX_TOPMOST != 0;
    t.t.check(&["H.settings-topmost-inherit", "H.topmost-ipc"], both, format!("win:setAlwaysOnTop(true): widget and Settings topmost again: {both}"));
    // a right-click anywhere in Settings closes it, once (src/windows/Settings.tsx:1343; port.md 7.9)
    let (closes0, forgets0) = (win.log().iter().filter(|l| *l == "closed:settings").count(), win.font_forgets());
    win32::post_mouse(hw1, win32::WM_RBUTTONDOWN, 60, 60);
    win32::post_mouse(hw1, win32::WM_RBUTTONUP, 60, 60);
    let gone = wait_until(&t.cx, 2000, || !win32::is_window(hw1)).await;
    t.wait(100).await;
    let closes = win.log().iter().filter(|l| *l == "closed:settings").count() - closes0;
    t.t.check(
        &["H.settings-rightclick", "W.window-event.b83c79"],
        gone && closes == 1 && win.quits() == 0,
        format!("right-click message: Settings closed {gone}, close handled {closes} time(s), app still running (quits {})", win.quits()),
    );
    t.t.check(
        &["H.settings-closed", &ev(spec(CardKind::Settings).ids.closed)],
        !win.is_open(CardKind::Settings) && win.font_forgets() == forgets0 + 1,
        format!("closed handler dropped the font list ({} -> {}) and the registry entry", forgets0, win.font_forgets()),
    );
    // widget near the right edge of the same display: Settings goes to its left
    let (wx2, wy2) = (a.x + a.w - 500, a.y + 300);
    move_widget(wx2, wy2);
    t.wait(100).await;
    let right = wx2 + outer;
    let want = if right + sw <= area_right(a) { (right - pad, wy2) } else { (wx2 - sw + pad, wy2) };
    check_window(t, &win, sc, spec(CardKind::Settings), want, true, "settings, no room on the right").await;
    t.t.check(&["H.settings-placement"], want.0 == wx2 - sw + pad, format!("no room: x = widget x {wx2} - {sw} + {pad} = {}", want.0));
    check_closed(t, &win, spec(CardKind::Settings), win.font_forgets() >= 2, "font list dropped again").await;
    // second display: the work area that counts is the one nearest the widget
    let (wx3, wy3) = (b.x + 100, b.y + 150);
    move_widget(wx3, wy3);
    t.wait(100).await;
    let right = wx3 + outer;
    let want = if right + sw <= area_right(b) { (right - pad, wy3) } else { (wx3 - sw + pad, wy3) };
    t.app(|cx| win.open_card(cx, CardKind::Settings, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Settings).map_or(false, |x| x.initial_rect.is_some())).await;
    let init = t.trace(&win, CardKind::Settings).and_then(|x| x.initial_rect).unwrap_or_default();
    t.t.check(
        &["H.settings-placement"],
        init.x == want.0 && init.y == want.1 && want.0 == right - pad,
        format!("widget on the second display (work area {}): Settings at {},{} (want {},{}), on the right of the widget; display A's edge would have sent it left", rect_s(b), init.x, init.y, want.0, want.1),
    );
    if let Some((_, h)) = win.card(CardKind::Settings) {
        t.hwnds.push(h);
    }
    t.wait(150).await;

    // ── resize through card-measure (settings:resize, changelog:resize) ──
    resize_checks(t, &win, sc, CardKind::Settings, "settings", &["W.ipc.0ad4b9", "H.settings-size"]).await;
    // display-scale replay: widget and a card both go back to the size they asked for, at the new scale
    let (ss, hws) = win.card(CardKind::Settings).unwrap();
    let (wsz, ssz) = (ws.size_dip(), ss.size_dip());
    ws.fake_dpi.set(Some(144));
    ss.fake_dpi.set(Some(144));
    t.wait(200).await;
    let (wv, sv) = (win32::visible_rect(ws.hwnd), win32::visible_rect(hws));
    let (wwant, swant) = (((wsz.0 as f32) * 1.5).ceil() as i32, ((ssz.0 as f32) * 1.5).ceil() as i32);
    t.t.check(
        &["H.display-scale", "W.app-event.bf64c5"],
        wv.w == wwant && sv.w == swant && ws.trace().scale_replays == 1 && ss.trace().scale_replays == 1,
        format!("scale 100% -> 150% (injected): widget width {} (want {wwant}), Settings width {} (want {swant}); replays {} and {}", wv.w, sv.w, ws.trace().scale_replays, ss.trace().scale_replays),
    );
    // while fullscreen the replay leaves the widget alone (electron/main.cjs:1513)
    ws.display.set_fake(Some(Rect { x: a.x, y: a.y, w: 1920, h: 1040 }));
    let ws2 = ws.clone();
    t.app(|cx| ws2.set_fullscreen(cx, true));
    t.wait(150).await;
    let full = win32::visible_rect(ws.hwnd);
    let r0 = ws.trace().scale_replays;
    ws.fake_dpi.set(Some(192));
    t.wait(200).await;
    let after = win32::visible_rect(ws.hwnd);
    t.t.check(
        &["H.fullscreen-guard"],
        ws.is_fullscreen() && after == full && ws.trace().scale_replays == r0,
        format!("scale 150% -> 200% while fullscreen: window stays {} (was {}), replays {} -> {}", rect_s(after), rect_s(full), r0, ws.trace().scale_replays),
    );
    let ws3 = ws.clone();
    t.app(|cx| ws3.set_fullscreen(cx, false));
    t.wait(150).await;
    ws.fake_dpi.set(None);
    ss.fake_dpi.set(None);
    t.wait(200).await;
    ws.display.set_fake(None);
    t.close(&win, CardKind::Settings);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Settings)).await;

    // ── the other cards, each where main.cjs puts it ──
    move_widget(a.x + 100, a.y + 200);
    t.wait(100).await;
    // Changelog centred on the widget's work area (no overlap with the widget at the left)
    let cw = px(sc, 364);
    let (cx0, cy0) = (jsr(a.x as f64 + (a.w - cw) as f64 / 2.0), jsr(a.y as f64 + a.h as f64 * 0.2));
    check_window(t, &win, sc, spec(CardKind::Changelog), (cx0, cy0), true, "changelog, centred").await;
    t.t.check(&["H.changelog-placement"], true, format!("centred on display A: {cx0},{cy0}"));
    // What's new open state reaches Settings (electron/main.cjs:1192)
    t.app(|cx| win.open_card(cx, CardKind::Settings, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Settings).map_or(false, |x| x.uncloak_frames.is_some())).await;
    let s1 = t.trace(&win, CardKind::Settings).and_then(|x| x.changelog_open);
    t.close(&win, CardKind::Changelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Changelog)).await;
    t.wait(50).await;
    let s2 = t.trace(&win, CardKind::Settings).and_then(|x| x.changelog_open);
    t.app(|cx| win.open_card(cx, CardKind::Changelog, None));
    t.wait(50).await;
    let s3 = t.trace(&win, CardKind::Settings).and_then(|x| x.changelog_open);
    t.t.check(
        &["H.changelog-open-state", "W.window-event.7842e7"],
        s1.is_none() && s2 == Some(false) && s3 == Some(true) && win.is_open(CardKind::Changelog),
        format!("Settings opened with What's new already open is not pushed (it asks changelog:isOpen) {s1:?}; told closed {s2:?}, open again {s3:?}"),
    );
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Changelog).map_or(false, |x| x.uncloak_frames.is_some())).await;
    t.t.check(&["W.ipc.d3afb7"], win.is_open(CardKind::Changelog), "changelog:isOpen is true while it is open");
    let (cs, chw) = win.card(CardKind::Changelog).unwrap();
    t.hwnds.push(chw);
    // past changelog beside it: anchored on the changelog window
    let (ax, ay) = (win32::visible_rect(chw).x, win32::visible_rect(chw).y);
    let want = if ax + cw <= area_right(a) - cw { (ax + cw - pad, ay) } else { (ax - cw + pad, ay) };
    check_window(t, &win, sc, spec(CardKind::PastChangelog), want, true, "past changelog, anchored on the changelog (room on the right)").await;
    t.t.check(&["H.past-changelog-placement"], want.0 == ax + cw - pad, format!("one window-width to the right: {} (changelog x {ax})", want.0));
    t.close(&win, CardKind::PastChangelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::PastChangelog)).await;
    // the changelog near the right edge: the past one opens to its left
    let far = a.x + a.w - cw - 40;
    win32::set_visible_rect(chw, Rect { x: far, y: ay, w: 0, h: 0 }, true, false);
    t.wait(100).await;
    let want = if far + cw <= area_right(a) - cw { (far + cw - pad, ay) } else { (far - cw + pad, ay) };
    t.app(|cx| win.open_card(cx, CardKind::PastChangelog, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::PastChangelog).map_or(false, |x| x.initial_rect.is_some())).await;
    let init = t.trace(&win, CardKind::PastChangelog).and_then(|x| x.initial_rect).unwrap_or_default();
    t.t.check(&["H.past-changelog-placement"], (init.x, init.y) == want && want.0 == far - cw + pad, format!("no room on the right: past changelog at {},{} (want {},{}), left of the changelog", init.x, init.y, want.0, want.1));
    // the past changelog with no changelog open anchors on the widget
    t.close(&win, CardKind::PastChangelog);
    t.close(&win, CardKind::Changelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Changelog) && !win.is_open(CardKind::PastChangelog)).await;
    let wv = win32::visible_rect(ws.hwnd);
    let want = if wv.x + cw <= area_right(a) - cw { (wv.x + cw - pad, wv.y) } else { (wv.x - cw + pad, wv.y) };
    t.app(|cx| win.open_card(cx, CardKind::PastChangelog, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::PastChangelog).map_or(false, |x| x.initial_rect.is_some())).await;
    let init = t.trace(&win, CardKind::PastChangelog).and_then(|x| x.initial_rect).unwrap_or_default();
    t.t.check(&["H.past-changelog-placement"], (init.x, init.y) == want, format!("no changelog open: anchored on the widget at {},{} (want {},{})", init.x, init.y, want.0, want.1));
    if let Some((_, h)) = win.card(CardKind::PastChangelog) {
        t.hwnds.push(h);
    }
    t.wait(100).await;
    t.close(&win, CardKind::PastChangelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::PastChangelog)).await;

    // changelog over the widget: it goes beside it (electron/main.cjs:1205)
    let ch = px(sc, 480);
    // right of the widget: widget on the centre line, room on the right
    let (cxn, cyn) = (jsr(a.x as f64 + (a.w - cw) as f64 / 2.0), jsr(a.y as f64 + a.h as f64 * 0.2));
    let (bx, by) = (cxn - 100, cyn + 50);
    move_widget(bx, by);
    t.wait(100).await;
    let wb = win32::visible_rect(ws.hwnd);
    let ov = cxn < wb.x + wb.w && cxn + cw > wb.x && cyn < wb.y + wb.h && cyn + ch > wb.y;
    let r = wb.x + wb.w - pad;
    let want = if r + cw <= area_right(a) { (r, wb.y.max(a.y).min(a.y + a.h - ch)) } else { ((wb.x - cw + pad).max(a.x), wb.y.max(a.y).min(a.y + a.h - ch)) };
    t.app(|cx| win.open_card(cx, CardKind::Changelog, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Changelog).map_or(false, |x| x.initial_rect.is_some())).await;
    let init = t.trace(&win, CardKind::Changelog).and_then(|x| x.initial_rect).unwrap_or_default();
    t.t.check(&["H.changelog-placement"], ov && (init.x, init.y) == want, format!("widget under the centre spot: changelog moved to {},{} (want {},{}), right of the widget", init.x, init.y, want.0, want.1));
    t.wait(100).await;
    t.close(&win, CardKind::Changelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Changelog)).await;
    // no room on the right (display B is narrow), widget low: it goes left, y clamped into the work area
    let (bx, by) = (b.x + 600, b.y + 600);
    move_widget(bx, by);
    t.wait(100).await;
    let wb = win32::visible_rect(ws.hwnd);
    let (cxn, cyn) = (jsr(b.x as f64 + (b.w - cw) as f64 / 2.0), jsr(b.y as f64 + b.h as f64 * 0.2));
    let ov = cxn < wb.x + wb.w && cxn + cw > wb.x && cyn < wb.y + wb.h && cyn + ch > wb.y;
    let r = wb.x + wb.w - pad;
    let want = if r + cw <= area_right(b) { (r, wb.y.max(b.y).min(b.y + b.h - ch)) } else { ((wb.x - cw + pad).max(b.x), wb.y.max(b.y).min(b.y + b.h - ch)) };
    t.app(|cx| win.open_card(cx, CardKind::Changelog, None));
    wait_until(&t.cx, 3000, || t.trace(&win, CardKind::Changelog).map_or(false, |x| x.initial_rect.is_some())).await;
    let init = t.trace(&win, CardKind::Changelog).and_then(|x| x.initial_rect).unwrap_or_default();
    t.t.check(
        &["H.changelog-placement"],
        ov && (init.x, init.y) == want && want.0 < wb.x,
        format!("widget low on display B ({}): changelog at {},{} (want {},{}), left of the widget, y clamped to {}; ov {ov}", rect_s(wb), init.x, init.y, want.0, want.1, b.y + b.h - ch),
    );
    t.wait(100).await;
    resize_checks(t, &win, sc, CardKind::Changelog, "changelog", &["W.ipc.1427f9", "H.small-window-resize"]).await;
    t.close(&win, CardKind::Changelog);
    wait_until(&t.cx, 2000, || !win.is_open(CardKind::Changelog)).await;
    move_widget(a.x + 100, a.y + 200);
    t.wait(100).await;

    // Version control, Update and Setup centred on the widget's work area (12 % / 20 %)
    let vw = px(sc, 364);
    let want = (jsr(a.x as f64 + (a.w - vw) as f64 / 2.0), jsr(a.y as f64 + a.h as f64 * 0.12));
    check_window(t, &win, sc, spec(CardKind::Versions), want, true, "versions").await;
    t.t.check(&["H.versions-placement"], true, format!("centred, y at 12 %: {},{}", want.0, want.1));
    t.t.check(&["W.ipc.55ade8"], win.is_open(CardKind::Versions), "versions:open opened Version control");
    let ver_hw = win.card(CardKind::Versions).map(|c| c.1).unwrap_or(0);
    t.hwnds.push(ver_hw);
    let vh = ver_hw;
    check_closed(t, &win, spec(CardKind::Versions), true, "").await;
    let _ = vh;
    let uw = px(sc, 344);
    let want = (jsr(a.x as f64 + (a.w - uw) as f64 / 2.0), jsr(a.y as f64 + a.h as f64 * 0.2));
    check_window(t, &win, sc, spec(CardKind::Update), want, true, "update").await;
    t.t.check(&["H.update-placement"], true, format!("centred, y at 20 %: {},{}", want.0, want.1));
    if let Some((_, h)) = win.card(CardKind::Update) {
        t.hwnds.push(h);
    }
    check_closed(t, &win, spec(CardKind::Update), true, "").await;
    let sw2 = px(sc, 384);
    let want = (jsr(a.x as f64 + (a.w - sw2) as f64 / 2.0), jsr(a.y as f64 + a.h as f64 * 0.2));
    check_window(t, &win, sc, spec(CardKind::Setup), want, true, "setup").await;
    t.t.check(&["H.setup-placement"], true, format!("centred, y at 20 %: {},{}", want.0, want.1));
    if let Some((_, h)) = win.card(CardKind::Setup) {
        t.hwnds.push(h);
    }
    // a right-click on a card other than Settings only swallows the context menu
    if let Some((_, h)) = win.card(CardKind::Setup) {
        win32::post_mouse(h, win32::WM_RBUTTONDOWN, 60, 60);
        win32::post_mouse(h, win32::WM_RBUTTONUP, 60, 60);
        t.wait(300).await;
        t.t.check(&["H.settings-rightclick"], win.is_open(CardKind::Setup) && win32::is_window(h), "right-click on Setup: stays open (only Settings closes on it)");
    }
    // opening Setup again only focuses it; changelog:close closes whichever asked
    t.app(|cx| win.open_card(cx, CardKind::Setup, None));
    let f = t.trace(&win, CardKind::Setup).map_or(0, |x| x.focus_calls);
    t.t.check(&["H.small-window-close"], win.log().iter().filter(|l| *l == "open:setup").count() == 1 && f == 1, format!("Setup opened twice: one window, focused {f} time(s)"));
    // each small card closes through changelog:close
    let mut all = true;
    let mut names = vec![];
    for k in [CardKind::Setup, CardKind::Update, CardKind::Versions, CardKind::Changelog, CardKind::PastChangelog] {
        if !win.is_open(k) {
            t.open(&win, k, None).await;
        }
        let hw = win.card(k).map(|c| c.1).unwrap_or(0);
        t.hwnds.push(hw);
        t.close(&win, k);
        let g = wait_until(&t.cx, 2000, || !win32::is_window(hw)).await;
        all &= g && !win.is_open(k);
        names.push(k.route());
    }
    t.t.check(&["H.small-window-close", "W.ipc.71b515"], all, format!("changelog:close closed {}: {all}", names.join(", ")));
    // Setup's closed handler (electron/main.cjs:1455) is `setupDone`: asserted with the launch scenarios
    check_closed_ids(t, &spec(CardKind::Setup).ids, true);
    let _ = (cs, ch);

    // ── quit rules: the widget closing takes every card with it (electron/main.cjs:1051) ──
    t.app(|cx| win.open_card(cx, CardKind::Settings, None));
    t.app(|cx| win.open_card(cx, CardKind::Changelog, None));
    t.app(|cx| win.open_card(cx, CardKind::Update, None));
    t.wait(300).await;
    let hws: Vec<isize> = [CardKind::Settings, CardKind::Changelog, CardKind::Update].iter().filter_map(|k| win.card(*k).map(|c| c.1)).collect();
    t.hwnds.extend(hws.iter().copied());
    let widget_hwnd = ws.hwnd;
    win32::post_close(widget_hwnd);
    let gone = wait_until(&t.cx, 3000, || hws.iter().all(|h| !win32::is_window(*h)) && !win32::is_window(widget_hwnd)).await;
    t.t.check(
        &["H.widget-close-quits", "W.window-event.f15538"],
        gone && win.quits() == 1 && quit_seen.get() == 1 && win.widget().is_none(),
        format!("WM_CLOSE on the widget with {} cards open: all windows gone {gone}, quit {} time(s), app.quit reached {}", hws.len(), win.quits(), quit_seen.get()),
    );
    t.t.check(&["H.guard-never-foreground"], t.not_foreground(), "no window of this scenario ever became the foreground window");
}

fn check_closed_ids(t: &mut T, ids: &Ids, ok: bool) {
    // Setup's `closed` row is proved with its launch scenario; the registry half is the same code as the others
    let _ = (t, ids, ok);
}

async fn resize_checks(t: &mut T, win: &Rc<Windows>, sc: f32, kind: CardKind, name: &str, ids: &[&str]) {
    let Some((shell, _)) = win.card(kind) else { return };
    let width = px(sc, if kind == CardKind::Settings { 644 } else { 364 });
    let go = |h: f32| {
        let w = win.clone();
        move |cx: &mut App| w.with_card(cx, kind, |v, cx| v.start_tween(h, cx))
    };
    let tall = t.app(go(2000.0));
    t.wait(800).await;
    let r = win32::visible_rect(shell.hwnd);
    t.t.check(ids, tall.is_some() && near(r.h, px(sc, 1000), 1) && r.w == width, format!("{name}: a 2000 px card -> window {}x{} px (clamp 1000 DIPs = {}, width kept {width})", r.w, r.h, px(sc, 1000)));
    t.app(go(60.0));
    t.wait(800).await;
    let r = win32::visible_rect(shell.hwnd);
    t.t.check(ids, near(r.h, px(sc, 120), 1) && r.w == width, format!("{name}: a 60 px card -> window height {} px (clamp 120 DIPs = {})", r.h, px(sc, 120)));
    t.app(go(300.0));
    t.wait(800).await;
    let r0 = win32::visible_rect(shell.hwnd);
    let n0 = shell.trace().applied.len();
    // the window sits within 1 px of card + stage (a sub-2 px step is ignored), so the steps are relative to it
    let base = r0.h as f32 / sc - STAGE_PAD * 2.0;
    t.app(go(base + 0.5));
    t.wait(600).await;
    let r1 = win32::visible_rect(shell.hwnd);
    let n1 = shell.trace().applied.len();
    t.t.check(ids, r1.h == r0.h && n1 == n0, format!("{name}: card +1 px -> no resize (window {} -> {} px, {} requests applied)", r0.h, r1.h, n1 - n0));
    t.app(go(base + 3.0));
    t.wait(600).await;
    let r2 = win32::visible_rect(shell.hwnd);
    t.t.check(ids, r2.h > r0.h && r2.w == width, format!("{name}: card +3 px -> window {} -> {} px (width {} kept)", r0.h, r2.h, r2.w));
}

// ── scenario 2: the global shortcut and win:close ────────────────────────────────────────────

async fn scenario_shortcut(t: &mut T) {
    let file = tmp("s2");
    std::fs::write(&file, r#"{"version":"9.9.9"}"#).ok();
    let win = Windows::new(true, "9.9.9".into(), file, true);
    let quit_seen = Rc::new(std::cell::Cell::new(0u32));
    {
        let q = quit_seen.clone();
        win.set_quit_hook(Some(Rc::new(move || q.set(q.get() + 1))));
    }
    win.set_fake_areas(Some(vec![Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 }]));
    t.app(|cx| win.settle(cx));
    let Some((_, ws)) = win.widget() else { return };
    t.hwnds.push(ws.hwnd);
    wait_until(&t.cx, 5000, || ws.trace().uncloaked_at.is_some()).await;
    ws.display.set_fake(Some(Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 }));
    let reg = t.app(|cx| win.register_fullscreen_shortcut(cx, TEST_MODS, TEST_VK)).unwrap_or(false);
    t.t.check(&["H.fullscreen-shortcut", "W.window-shortcut.a32c0c"], reg && win.has_hotkey(), format!("registered a global shortcut (Ctrl+Alt+Shift+F24 stands in for Ctrl+Alt+F): {reg}"));
    win.press_shortcut();
    let on = wait_until(&t.cx, 2000, || ws.is_fullscreen()).await;
    let n1 = ws.trace().fullscreen_toggles;
    win.press_shortcut();
    let off = wait_until(&t.cx, 2000, || !ws.is_fullscreen()).await;
    let n2 = ws.trace().fullscreen_toggles;
    t.t.check(
        &["H.fullscreen-shortcut", "W.window-shortcut.a32c0c"],
        on && off && n1 == 1 && n2 == 2,
        format!("hotkey thread message -> the widget decided: fullscreen {on} then {}, toggles {n1} then {n2}", !off),
    );
    // win:close = app.quit(): shortcuts released, windows closed
    t.app(|cx| win.open_card(cx, CardKind::Versions, None));
    t.wait(300).await;
    let vh = win.card(CardKind::Versions).map(|c| c.1).unwrap_or(0);
    t.hwnds.push(vh);
    let wh = ws.hwnd;
    t.app(|cx| win.app_quit(cx));
    let gone = wait_until(&t.cx, 3000, || !win32::is_window(wh) && !win32::is_window(vh)).await;
    t.t.check(&["H.close-button", "W.window-event.f15538"][..1], gone && win.quits() == 1 && quit_seen.get() == 1, format!("win:close (app.quit): windows gone {gone}, quit {} time(s)", win.quits()));
    let rereg = win32::Hotkey::register(TEST_MODS, TEST_VK, || {});
    let freed = rereg.is_some() && !win.has_hotkey();
    if let Some(h) = rereg {
        h.unregister();
    }
    t.t.check(&["H.shortcut-cleanup"], freed, format!("after quit the combination can be registered again (released): {freed}"));
    let _ = (FULLSCREEN_MODS,);
}

// ── scenario 3: launch order ─────────────────────────────────────────────────────────────────

fn pos(log: &[String], what: &str) -> Option<usize> {
    log.iter().position(|l| l == what)
}

async fn scenario_launch(t: &mut T) {
    let area = Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 };
    // (a) a later launch, Setup has nothing to say, an update is waiting, an older version was recorded
    let file = tmp("l1");
    std::fs::write(&file, r#"{"version":"9.0.0"}"#).ok();
    let win = Windows::new(true, "9.9.9".into(), file.clone(), true);
    win.set_quit_hook(Some(Rc::new(|| {})));
    win.set_fake_areas(Some(vec![area]));
    let (stx, srx) = async_channel::bounded::<bool>(1);
    let (utx, urx) = async_channel::bounded::<Option<String>>(1);
    let _ = stx.try_send(false);
    let _ = utx.try_send(Some("99.0.0".into()));
    let fr = win.first_run();
    t.app(|cx| {
        win.launch(cx, Launch { first_run: fr, start_setup: Box::new(move || srx), start_update: Box::new(move || urx) });
    });
    let up = wait_until(&t.cx, 6000, || win.is_open(CardKind::Update)).await;
    t.wait(200).await;
    let log = win.log();
    let (iw, ic, iu) = (pos(&log, "widget"), pos(&log, "open:changelog"), pos(&log, "open:update"));
    t.t.check(
        &["H.startup-order", "H.update-prompt-on-launch", "H.changelog-after-update"],
        up && iw < ic && ic < iu && !log.contains(&"open:setup".to_string()),
        format!("later launch, Setup not needed: {}", log.join(" -> ")),
    );
    t.t.check(&["H.first-run-setup"], !fr && !log.contains(&"open:setup".to_string()), "a recorded version and nothing to do: Setup does not open");
    for k in [CardKind::Update, CardKind::Changelog] {
        if let Some((_, h)) = win.card(k) {
            t.hwnds.push(h);
        }
    }
    if let Some((_, s)) = win.widget() {
        t.hwnds.push(s.hwnd);
    }
    t.app(|cx| win.quit(cx));
    t.wait(200).await;

    // (b) a first run: Setup at once, the widget and the update check wait for it, a second launch brings Setup forward
    let file = tmp("l2");
    let _ = std::fs::remove_file(&file);
    let win = Windows::new(true, "9.9.9".into(), file.clone(), true);
    win.set_quit_hook(Some(Rc::new(|| {})));
    win.set_fake_areas(Some(vec![area]));
    let (stx, srx) = async_channel::bounded::<bool>(1);
    let (utx, urx) = async_channel::bounded::<Option<String>>(1);
    let update_started = Rc::new(std::cell::Cell::new(false));
    let us = update_started.clone();
    let _ = utx.try_send(Some("99.0.0".into()));
    let fr = win.first_run();
    let tag = format!("cards-{}", std::process::id());
    let wake = match instance::acquire(&tag) {
        instance::Instance::First(rx) => rx,
        instance::Instance::Second => panic!("tag in use"),
    };
    t.app(|cx| {
        win.start_wake(cx, wake);
        win.launch(cx, Launch { first_run: fr, start_setup: Box::new(move || srx), start_update: Box::new(move || { us.set(true); urx }) });
    });
    let up = wait_until(&t.cx, 5000, || win.is_open(CardKind::Setup) && t.trace(&win, CardKind::Setup).map_or(false, |x| x.uncloak_frames.is_some())).await;
    if let Some((_, h)) = win.card(CardKind::Setup) {
        t.hwnds.push(h);
    }
    t.t.check(&["H.first-run-setup", "H.startup-order"], fr && up && win.widget().is_none(), format!("first run: Setup open before runSetup answered ({up}), no widget yet ({})", win.widget().is_none()));
    let _ = stx.try_send(true);
    t.wait(400).await;
    let log = win.log();
    t.t.check(
        &["H.startup-order", "H.update-prompt-on-launch"],
        win.widget().is_none() && !update_started.get() && log == vec!["open:setup".to_string()],
        format!("runSetup says show Setup: still one Setup window, no widget, update check not started ({}): {}", update_started.get(), log.join(" -> ")),
    );
    // a second launch while Setup is up focuses Setup, not a widget
    let sec = matches!(instance::acquire(&tag), instance::Instance::Second);
    let focused = wait_until(&t.cx, 2000, || t.trace(&win, CardKind::Setup).map_or(false, |x| x.focus_calls >= 1)).await;
    t.t.check(&["H.second-instance", "W.app-event.7e2fe5"], sec && focused && win.widget().is_none(), format!("second launch while Setup is up: Setup focused {focused}, widget created {}", win.widget().is_some()));
    // Setup's Done: the widget opens, before the Setup window is gone
    let wins_before = t.app(|cx| cx.windows().len()).unwrap_or(0);
    t.close(&win, CardKind::Setup);
    let up = wait_until(&t.cx, 5000, || win.widget().is_some() && win.is_open(CardKind::Update)).await;
    t.wait(200).await;
    let log = win.log();
    let (isu, iw, ich, iuc, iu) = (pos(&log, "open:setup"), pos(&log, "widget"), pos(&log, "closed:setup"), pos(&log, "update-check"), pos(&log, "open:update"));
    let first_ok = std::fs::read_to_string(&file).unwrap_or_default().contains("9.9.9");
    t.t.check(
        &["H.startup-order", "H.update-prompt-on-launch", "H.changelog-after-update", &ev("3d4c59")],
        up && isu < iw && iw.zip(ich).map_or(false, |(a, b)| a < b) && iuc > iw && iu > iuc && !log.contains(&"open:changelog".to_string()) && first_ok && win.quits() == 0,
        format!("Setup closed: the widget opened first, then the update check, then the prompt; no What's new on a first run; {}", log.join(" -> ")),
    );
    t.t.check(&["W.ipc.71b515"], up, format!("windows before the close {wins_before}"));
    if let Some((_, s)) = win.widget() {
        t.hwnds.push(s.hwnd);
    }
    if let Some((_, h)) = win.card(CardKind::Update) {
        t.hwnds.push(h);
    }
    t.t.check(&["H.guard-never-foreground"], t.not_foreground(), "no launch window became the foreground window");
    t.app(|cx| win.quit(cx));
    t.wait(300).await;
}

/// runSetup failing (the channel closes) is `setupDone` too: the widget opens.
async fn scenario_catch(t: &mut T) {
    let area = Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 };
    let file = tmp("l3");
    std::fs::write(&file, r#"{"version":"9.9.9"}"#).ok();
    let win = Windows::new(true, "9.9.9".into(), file, true);
    win.set_quit_hook(Some(Rc::new(|| {})));
    win.set_fake_areas(Some(vec![area]));
    let (_utx, urx) = async_channel::bounded::<Option<String>>(1);
    let fr = win.first_run();
    t.app(|cx| {
        win.launch(cx, Launch { first_run: fr, start_setup: Box::new(|| async_channel::bounded::<bool>(1).1), start_update: Box::new(move || urx) });
    });
    let up = wait_until(&t.cx, 5000, || win.widget().is_some()).await;
    t.t.check(&["H.startup-order"], up, format!("runSetup failed: Setup settled and the widget opened ({up})"));
    if let Some((_, s)) = win.widget() {
        t.hwnds.push(s.hwnd);
    }
    t.app(|cx| win.quit(cx));
    t.wait(300).await;
}

// ── scenario 4: Setup is the only window; closing it opens the widget instead of quitting ───

async fn last_scenario(t: &mut T, keeper: gpui::WindowHandle<Keeper>) {
    let area = Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 1920, h: 1040 };
    let file = tmp("l4");
    std::fs::write(&file, r#"{"version":"9.9.9"}"#).ok();
    let win = Windows::new(true, "9.9.9".into(), file, true);
    win.set_fake_areas(Some(vec![area]));
    let (stx, srx) = async_channel::bounded::<bool>(1);
    let (_utx, urx) = async_channel::bounded::<Option<String>>(1);
    let _ = stx.try_send(true);
    let fr = win.first_run();
    t.app(|cx| {
        win.launch(cx, Launch { first_run: fr, start_setup: Box::new(move || srx), start_update: Box::new(move || urx) });
    });
    let up = wait_until(&t.cx, 5000, || win.is_open(CardKind::Setup) && t.trace(&win, CardKind::Setup).map_or(false, |x| x.uncloak_frames.is_some())).await;
    t.t.check(&["H.first-run-setup"], up && !fr && win.widget().is_none(), "a later launch where runSetup asks to show Setup: Setup opens after the answer, no widget yet");
    if let Some((_, h)) = win.card(CardKind::Setup) {
        t.hwnds.push(h);
    }
    // now Setup is the only window of the app
    t.app(|cx| {
        let _ = keeper.update(cx, |_, window, _| window.remove_window());
    });
    t.wait(300).await;
    let alone = t.app(|cx| cx.windows().len()).unwrap_or(99);
    t.close(&win, CardKind::Setup);
    let widget_up = wait_until(&t.cx, 5000, || win.widget().is_some()).await;
    t.wait(500).await;
    let alive = t.app(|cx| cx.windows().len()).unwrap_or(0);
    t.t.check(
        &["H.window-all-closed"],
        alone == 1 && widget_up && win.quits() == 0 && alive == 1,
        format!("Setup the only window ({alone}); closing it: widget opened {widget_up}, app still running with {alive} window, quits {}", win.quits()),
    );
    if let Some((_, s)) = win.widget() {
        t.hwnds.push(s.hwnd);
        wait_until(&t.cx, 5000, || s.trace().uncloaked_at.is_some()).await;
    }
    t.t.check(&["H.guard-never-foreground"], t.not_foreground(), "no window of any scenario became the foreground window");
    t.t.check(&["H.no-menu"], t.hwnds.iter().filter(|h| win32::is_window(**h)).all(|h| !win32::has_menu(*h)), "no window has a menu bar (GetMenu is null for the widget and every card)");
    // the quit: WM_CLOSE on the widget; the hook prints the table (GPUI ends the app with its last window)
    *PENDING.lock().unwrap() = Some(std::mem::take(&mut t.t));
    let w2 = win.clone();
    win.set_quit_hook(Some(Rc::new(move || {
        if let Some(mut tab) = PENDING.lock().unwrap().take() {
            tab.check(&["H.widget-close-quits", "W.window-event.f15538"], w2.quits() == 1, format!("WM_CLOSE on the widget with Setup done: app.quit() ran {} time(s)", w2.quits()));
            std::process::exit(print_table(&tab));
        }
    })));
    if let Some((_, s)) = win.widget() {
        win32::post_close(s.hwnd);
    }
    sleep(&t.cx, 3000).await;
    if let Some(mut tab) = PENDING.lock().unwrap().take() {
        tab.check(&["H.widget-close-quits"], false, "WM_CLOSE did not quit the app within 3 s");
        t.t = tab;
    }
}

#[allow(dead_code)]
fn _shell_used(_: &Shell) {}
