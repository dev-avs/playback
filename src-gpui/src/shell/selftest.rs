//! `--selftest-shell`: opens the widget off-screen, drives every shell behaviour and asserts it with
//! Win32 reads (GetWindowRect, DwmGetWindowAttribute, GetWindowLongPtr). Prints a pass/fail table by
//! ledger row id and exits non-zero on any failure. The mouse is never touched: hover and drag use the
//! injected cursor of `widget::Cursor`; no window is activated.

use super::widget::{open_widget, Opts, ResizeMode, Shell, Widget, STAGE_PAD, TEST_ORIGIN, WIDGET_H0, WIDGET_W};
use super::{sleep, wait_until, with_widget};
use crate::instance;
use crate::win32::{self, Rect};
use gpui::{App, WindowHandle};
use std::{rc::Rc, sync::Mutex};

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;
const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

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
    cx: gpui::AsyncApp,
    handle: WindowHandle<Widget>,
    shell: Rc<Shell>,
    t: Table,
}

impl T {
    fn app<R>(&self, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        self.cx.update(f).ok()
    }
    /// Window size now, in DIPs.
    fn size(&self) -> (i32, i32) {
        self.shell.size_dip()
    }
    fn rect(&self) -> Rect {
        win32::visible_rect(self.shell.hwnd)
    }
    async fn wait(&self, ms: u64) {
        sleep(&self.cx, ms).await;
    }
    fn win_resize(&self, h: f32, hovered: bool) {
        let s = self.shell.clone();
        self.app(|app| s.win_resize(app, h, hovered));
    }
    /// Park the fake cursor outside the window.
    fn cursor_away(&self) {
        self.shell.cursor.set_fake(Some((-1, -1)));
    }
}

pub fn run(cx: &mut App, tag: String) {
    let rx = match instance::acquire(&tag) {
        instance::Instance::First(rx) => rx,
        instance::Instance::Second => panic!("selftest instance tag already in use"),
    };
    let (handle, shell, _driver) = open_widget(cx, Opts { test: true, wake: Some(rx), resize_mode: ResizeMode::Off });
    shell.cursor.set_fake(Some((-1, -1)));
    cx.spawn(async move |acx| {
        let mut t = T { cx: acx.clone(), handle, shell, t: Table::default() };
        body(&mut t, &tag).await;
        let code = print_table(&t.t);
        std::process::exit(code);
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

fn near(a: i32, b: i32, tol: i32) -> bool {
    (a - b).abs() <= tol
}

async fn body(t: &mut T, tag: &str) {
    let hwnd = t.shell.hwnd;
    let sc = t.shell.scale();
    let px = |d: f32| (d * sc).ceil() as i32;

    // show: cloak until the card size is stable (port.md 7.7)
    let up = wait_until(&t.cx, 5000, || t.shell.trace().uncloaked_at.is_some()).await;
    let tr = t.shell.trace();
    let strip = tr.strip;
    t.t.check(&["H.widget-initial"], up && tr.shown_cloaked, format!("shown cloaked: {}, uncloaked once the card was laid out: {up}", tr.shown_cloaked));
    let frames = tr.uncloak_frames.unwrap_or(0);
    t.t.check(&["H.widget-initial"], frames >= 2, format!("uncloaked after {frames} frames at a stable card size (need >= 2)"));
    t.t.check(&["H.widget-initial"], !win32::is_cloaked(hwnd), "DWMWA_CLOAKED reads 0 after the lift");

    // options (electron/main.cjs:1004-1011)
    let init = tr.initial_rect.unwrap_or_default();
    t.t.check(
        &["W.window-option.a912a4", "W.window-const.b0eb94", "W.window-const.32944d"],
        init.w == px(WIDGET_W) && WIDGET_W == 394.0 && STAGE_PAD == 22.0,
        format!("initial width {} px (scale {sc}), WIDGET_W {WIDGET_W}, STAGE_PAD {STAGE_PAD}", init.w),
    );
    t.t.check(&["W.window-option.332d75", "H.widget-initial"], init.h == px(WIDGET_H0) && WIDGET_H0 == 420.0, format!("initial height {} px", init.h));
    t.t.check(&["H.widget-initial"], init.x == TEST_ORIGIN.0 && init.y == TEST_ORIGIN.1, format!("test placement {},{}", init.x, init.y));
    let st = win32::style(hwnd);
    let ex = win32::ex_style(hwnd);
    // GPUI keeps WS_CAPTION for the system menu and snapping, and hands the whole window to the client area
    // (WM_NCCALCSIZE), so frameless is read from the geometry: no title bar above the client, no frame on top
    let (wr, vr) = (win32::window_rect(hwnd), win32::visible_rect(hwnd));
    t.t.check(
        &["W.window-option.3abc21"],
        vr.y - wr.y <= 1 && st & WS_CAPTION == WS_CAPTION && strip[2],
        format!("style {st:#010x}; client top inset {} px (no title bar), NC rendering policy disabled", vr.y - wr.y),
    );
    t.t.check(&["W.window-option.a96e0e"], ex & WS_EX_NOREDIRECTIONBITMAP != 0, format!("exstyle {ex:#010x}: WS_EX_NOREDIRECTIONBITMAP (composited alpha)"));
    t.t.check(&["W.window-option.1a7457"], st & WS_THICKFRAME == 0, format!("style {st:#010x}: no WS_THICKFRAME"));
    t.t.check(&["W.window-option.6251c9", "H.topmost-level"], ex & WS_EX_TOPMOST != 0, format!("exstyle {ex:#010x}: WS_EX_TOPMOST"));
    t.t.check(
        &["W.window-option.9c51c2"],
        ex & WS_EX_APPWINDOW != 0 && ex & WS_EX_TOOLWINDOW == 0,
        format!("exstyle {ex:#010x}: APPWINDOW set, TOOLWINDOW clear (taskbar button kept)"),
    );
    t.t.check(
        &["W.window-option.b14c5d"],
        strip == [true; 3] && win32::dwm_corner(hwnd) == win32::CORNER_DONOTROUND,
        format!("DWM strip Set HRESULTs ok (corner, border, nc-render) = {strip:?}, corner reads back DONOTROUND"),
    );
    t.t.check(&["H.hide-when-idle"], !win32::is_foreground(hwnd), "shown without taking foreground (focus: false, SW_SHOWNOACTIVATE)");

    // hover by cursor poll (port.md 7.6)
    let r = t.rect();
    t.shell.cursor.set_fake(Some((r.x + r.w / 2, r.y + r.h / 2)));
    let h_in = wait_until(&t.cx, 500, || t.shell.hovered.get()).await;
    t.shell.cursor.set_fake(Some((r.x - 50, r.y + 10)));
    let h_out = wait_until(&t.cx, 500, || !t.shell.hovered.get()).await;
    t.t.check(&["H.widget-height-rules"], h_in && h_out, format!("hover follows the polled cursor: in={h_in}, out={h_out}"));
    t.cursor_away();

    // set-window-size: grow at once, jitter, shrink debounce
    t.wait(500).await;
    let (cw, ch) = t.size();
    t.win_resize((ch + 100) as f32, false);
    t.wait(150).await;
    let (_, h1) = t.size();
    t.t.check(
        &["H.set-window-size", "H.widget-height-rules"],
        near(h1, ch + 100, 1) && t.size().0 == cw,
        format!("grow {ch} -> {h1} within 150 ms, width kept {cw}"),
    );
    let n0 = t.shell.trace().applied.len();
    t.win_resize((h1 + 2) as f32, false);
    t.wait(400).await;
    t.t.check(&["H.widget-height-rules"], t.shell.trace().applied.len() == n0 && near(t.size().1, h1, 0), "|current - h| < 3 ignored (no size applied)");

    t.win_resize((h1 - 120) as f32, false);
    t.wait(110).await;
    let mid = t.size().1;
    t.wait(300).await;
    let end = t.size().1;
    t.t.check(&["H.widget-height-rules"], near(mid, h1, 0) && near(end, h1 - 120, 1), format!("shrink debounced: at 110 ms {mid} (was {h1}), at 410 ms {end}"));

    // a newer request inside the debounce replaces the pending one
    let (_, h2) = t.size();
    let n1 = t.shell.trace().applied.len();
    t.win_resize((h2 - 60) as f32, false);
    t.wait(150).await;
    t.win_resize((h2 - 90) as f32, false);
    t.wait(150).await;
    let still = t.size().1;
    t.wait(250).await;
    let n2 = t.shell.trace().applied.len();
    t.t.check(
        &["H.widget-height-rules"],
        near(still, h2, 0) && n2 == n1 + 1 && near(t.size().1, h2 - 90, 1),
        format!("a newer request replaces the pending shrink: {} size applied, final {}", n2 - n1, t.size().1),
    );

    // never shrink while hovered
    let (_, h3) = t.size();
    let r = t.rect();
    t.shell.cursor.set_fake(Some((r.x + r.w / 2, r.y + r.h / 2)));
    wait_until(&t.cx, 500, || t.shell.hovered.get()).await;
    t.win_resize((h3 - 80) as f32, true);
    t.wait(450).await;
    t.t.check(&["H.widget-height-rules"], near(t.size().1, h3, 0), format!("shrink refused while hovered ({h3} kept after 450 ms)"));
    t.cursor_away();
    wait_until(&t.cx, 500, || !t.shell.hovered.get()).await;

    // clamps
    t.win_resize(10.0, false);
    let lo = t.shell.wanted().0;
    t.win_resize(5000.0, false);
    let hi = t.shell.wanted().0;
    t.wait(200).await;
    t.t.check(&["H.widget-height-clamp"], lo == Some(80) && hi == Some(1400), format!("win:resize clamps 80..1400: 10 -> {lo:?}, 5000 -> {hi:?}"));
    t.win_resize(420.0, false);
    t.wait(500).await;

    // drag (poll-driven, 8 ms)
    let r0 = t.rect();
    t.shell.cursor.set_fake(Some((r0.x + 100, r0.y + 100)));
    let s = t.shell.clone();
    t.app(|app| s.start_drag(app));
    t.shell.cursor.set_fake(Some((r0.x + 160, r0.y + 130)));
    let t0 = std::time::Instant::now();
    t.wait(400).await;
    let ticks = t.shell.trace().drag_ticks;
    let el = t0.elapsed().as_secs_f64() * 1000.0;
    let r1 = t.rect();
    t.t.check(
        &["H.drag-poll", "H.drag-start"],
        r1.x == r0.x + 60 && r1.y == r0.y + 30 && r1.w == r0.w,
        format!("window followed the cursor delta (+60,+30): moved {},{} size kept", r1.x - r0.x, r1.y - r0.y),
    );
    let per = el / ticks.max(1) as f64;
    t.t.check(&["H.drag-poll", "W.window-const.ce4dcf"], per > 6.0 && per < 11.0, format!("{ticks} ticks in {el:.0} ms = {per:.1} ms per tick (8 expected)"));
    // a mid-drag resize is dropped, then replayed by stop_drag
    let (_, hd) = t.size();
    t.win_resize((hd + 90) as f32, false);
    t.wait(150).await;
    let held = t.size().1;
    let s = t.shell.clone();
    t.app(|app| s.stop_drag(app));
    t.wait(150).await;
    t.t.check(
        &["H.drag-stop", "H.widget-height-rules"],
        near(held, hd, 0) && near(t.size().1, hd + 90, 1) && t.shell.trace().replays >= 1,
        format!("mid-drag resize refused ({held}), replayed on stop -> {}", t.size().1),
    );
    t.t.check(&["H.drag-stop"], !t.shell.is_dragging(), "drag state cleared");
    t.cursor_away();
    win32::set_visible_rect(hwnd, Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w: 0, h: 0 }, true, false);
    t.win_resize(420.0, false);
    t.wait(500).await;

    // fullscreen
    let before = t.rect();
    let disp = Rect { x: TEST_ORIGIN.0 - 3000, y: TEST_ORIGIN.1, w: 1280, h: 720 };
    t.shell.display.set_fake(Some(disp));
    let s = t.shell.clone();
    t.app(|app| s.set_fullscreen(app, true));
    t.wait(150).await;
    t.t.check(&["H.fullscreen-enter"], t.rect() == disp && t.shell.is_fullscreen(), format!("fullscreen covers the display bounds {:?}", t.rect()));
    let na = t.shell.trace().applied.len();
    t.win_resize(700.0, false);
    let s = t.shell.clone();
    t.app(|app| s.start_drag(app));
    t.wait(150).await;
    t.t.check(
        &["H.fullscreen-guard"],
        t.shell.trace().applied.len() == na && !t.shell.is_dragging() && t.rect() == disp,
        "height request and drag ignored while fullscreen",
    );
    let s = t.shell.clone();
    t.app(|app| s.set_fullscreen(app, false));
    t.wait(400).await;
    let back = t.rect();
    t.t.check(
        &["H.fullscreen-enter"],
        back.x == before.x && back.y == before.y && back.w == before.w && !t.shell.is_fullscreen(),
        format!("leave restores the saved bounds ({},{} {}x{})", back.x, back.y, back.w, back.h),
    );
    t.t.check(&["H.fullscreen-enter", "H.fullscreen-guard"], near(t.size().1, 700, 1), format!("leave replays the last wanted height ({} DIP)", t.size().1));
    t.shell.display.set_fake(None);
    t.win_resize(420.0, false);
    t.wait(500).await;

    // width setting
    let s = t.shell.clone();
    t.app(|app| s.set_width(app, 100.0));
    t.wait(100).await;
    let w_lo = t.size().0;
    let s = t.shell.clone();
    t.app(|app| s.set_width(app, 1000.0));
    t.wait(100).await;
    let w_hi = t.size().0;
    let s = t.shell.clone();
    t.app(|app| s.set_width(app, 350.0));
    t.wait(100).await;
    t.t.check(
        &["H.widget-width-setting"],
        w_lo == 324 && w_hi == 644 && t.size().0 == 394,
        format!("width clamp 280..600 + 44: 100 -> {w_lo}, 1000 -> {w_hi}, 350 -> {}", t.size().0),
    );

    // hide / show, topmost, minimize / restore
    let s = t.shell.clone();
    t.app(|app| s.set_hidden(app, true));
    let hid = wait_until(&t.cx, 500, || !win32::is_visible(hwnd)).await;
    let s = t.shell.clone();
    t.app(|app| s.set_hidden(app, false));
    let shown = wait_until(&t.cx, 500, || win32::is_visible(hwnd)).await;
    t.t.check(
        &["H.hide-when-idle"],
        hid && shown && !win32::is_foreground(hwnd),
        format!("hidden: {hid}, shown again: {shown}, foreground taken: {}", win32::is_foreground(hwnd)),
    );

    let s = t.shell.clone();
    t.app(|app| s.set_always_on_top(app, false));
    t.wait(100).await;
    let off = win32::ex_style(hwnd) & WS_EX_TOPMOST == 0;
    let s = t.shell.clone();
    t.app(|app| s.set_always_on_top(app, true));
    t.wait(100).await;
    let on = win32::ex_style(hwnd) & WS_EX_TOPMOST != 0;
    t.t.check(&["H.topmost-ipc"], off && on, format!("WS_EX_TOPMOST cleared: {off}, set again: {on}"));

    t.win_resize(500.0, false);
    t.wait(200).await;
    let rp0 = t.shell.trace().replays;
    let s = t.shell.clone();
    t.app(|app| s.minimize(app));
    let iconic = wait_until(&t.cx, 500, || win32::is_minimized(hwnd)).await;
    t.wait(60).await;
    t.t.check(&["H.minimize-button"], iconic, "win:minimize minimizes the widget (IsIconic)");
    // a second launch while minimized: restore + show, no activation in test mode
    let sec = matches!(instance::acquire(tag), instance::Instance::Second);
    let woke = wait_until(&t.cx, 1000, || t.shell.trace().second_instance >= 1).await;
    let restored = wait_until(&t.cx, 1000, || !win32::is_minimized(hwnd)).await;
    t.t.check(&["H.single-instance"], sec, "a second acquire of the same name returns Second (it would exit 0)");
    t.t.check(&["H.second-instance"], woke && restored, format!("first instance woke: {woke}, restored from minimize: {restored}"));
    t.wait(200).await;
    t.t.check(&["H.restore-replay"], t.shell.trace().replays > rp0, format!("restore replayed the wanted height ({} replays)", t.shell.trace().replays - rp0));
    t.win_resize(420.0, false);
    t.wait(500).await;

    // card measure: grow once, shrink after the tween (port.md 7.8)
    t.shell.resize_mode.set(ResizeMode::Shell);
    t.wait(500).await;
    let (_, hs) = t.size();
    let na = t.shell.trace().applied.len();
    with_widget(&t.cx, t.handle, |w, cx| w.start_tween(500.0, cx));
    t.wait(700).await;
    let ap = t.shell.trace().applied;
    let grew = &ap[na..];
    let want = (500.0 + STAGE_PAD * 2.0) as i32;
    t.t.check(
        &["H.widget-height-rules"],
        grew.len() <= 2 && grew.last().map(|g| g.1) == Some(want),
        format!("grow tween from {hs}: window sizes applied {:?} (final {want})", grew.iter().map(|g| g.1).collect::<Vec<_>>()),
    );
    let na = ap.len();
    with_widget(&t.cx, t.handle, |w, cx| w.start_tween(300.0, cx));
    t.wait(450).await;
    let during = t.size().1;
    t.wait(500).await;
    let ap = t.shell.trace().applied;
    let shr = &ap[na..];
    let want = (300.0 + STAGE_PAD * 2.0) as i32;
    t.t.check(
        &["H.widget-height-rules"],
        near(during, 544, 1) && shr.len() == 1 && shr[0].1 == want && near(t.size().1, want, 1),
        format!("shrink tween: window held at {during} until the tween ended, then sizes applied {:?}", shr.iter().map(|g| g.1).collect::<Vec<_>>()),
    );
    t.shell.resize_mode.set(ResizeMode::Off);

    // close quits: GPUI ends the process with the last window, so the table is handed to
    // `last_window_closed` (called from the app's window-closed hook) and printed there
    *PENDING.lock().unwrap() = Some(std::mem::take(&mut t.t));
    win32::post_close(hwnd);
    sleep(&t.cx, 3000).await;
    if let Some(mut tab) = PENDING.lock().unwrap().take() {
        tab.check(&["H.widget-close-quits", "H.close-button"], false, "WM_CLOSE did not close the widget within 3 s");
        t.t = tab;
    }
}

/// Table waiting for the close check, see `body`.
static PENDING: Mutex<Option<Table>> = Mutex::new(None);

/// The app's window-closed hook, when no window is left: closing the widget quit the app.
pub fn last_window_closed() {
    if let Some(mut tab) = PENDING.lock().unwrap().take() {
        tab.check(&["H.widget-close-quits", "H.close-button"], true, "WM_CLOSE on the widget closed the last window and the app quit");
        std::process::exit(print_table(&tab));
    }
}
