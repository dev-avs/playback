//! The widget window shell (electron/main.cjs `createWindow` and the window-level helpers around it).
//! It draws no real UI: a placeholder card whose height can tween stands in for the widget, so the
//! window behaviour can be proven before any screen exists (parity/ledger.md, area `shell`).
//!
//! Units: sizes in this file are DIPs (what Electron calls pixels, GPUI calls `px`); the Win32 boundary
//! multiplies by the window's DPI scale. A window's "rect" is its drawn area (the client rect, see
//! `win32::visible_rect`), which is what Electron's getBounds() reports for a frameless window: the
//! invisible resize frame GetWindowRect adds (port.md 7.5) is never part of it.

use crate::frame_driver::FrameDriver;
use crate::win32::{self, Rect};
use gpui::{
    div, point, prelude::*, px, rgb, size, App, Bounds, Context, Entity, Hsla, MouseButton, Task, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{atomic::{AtomicBool, Ordering}, mpsc::Receiver, Arc},
    time::{Duration, Instant},
};

// electron/main.cjs:250. Keep in sync with STAGE_PAD in src/App.tsx:95.
pub const STAGE_PAD: f32 = 22.0;
// electron/main.cjs:251
pub const WIDGET_W: f32 = 350.0 + STAGE_PAD * 2.0;
// electron/main.cjs:1005
pub const WIDGET_H0: f32 = 420.0;
// electron/main.cjs:873
const DRAG_INTERVAL_MS: u64 = 8;
// electron/main.cjs:994
const SHRINK_DEBOUNCE_MS: u64 = 220;
// the cursor poll of port.md 7.6 (one display frame at 60 Hz)
const POLL_MS: u64 = 16;
/// Test and snapshot windows live here, far off every monitor.
pub const TEST_ORIGIN: (i32, i32) = (-20000, -20000);
/// The placeholder card's first height (stands for the widget's idle height).
pub const CARD_H0: f32 = 300.0;

/// Where the cursor comes from: the OS, or a position a test injects (tests never touch the real mouse).
#[derive(Default)]
pub struct Cursor {
    fake: Cell<Option<(i32, i32)>>,
}
impl Cursor {
    pub fn pos(&self) -> (i32, i32) {
        self.fake.get().unwrap_or_else(win32::cursor_pos)
    }
    pub fn set_fake(&self, p: Option<(i32, i32)>) {
        self.fake.set(p);
    }
}

/// Which monitor the fullscreen widget covers: the one matching the window, or a rect a test injects.
#[derive(Default)]
pub struct Display {
    fake: Cell<Option<Rect>>,
}
impl Display {
    fn bounds(&self, hwnd: isize) -> Rect {
        self.fake.get().unwrap_or_else(|| win32::monitor_of(hwnd).0)
    }
    pub fn set_fake(&self, r: Option<Rect>) {
        self.fake.set(r);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ResizeMode {
    /// The card-measure resize of port.md 7.8 through Electron's rules (`win_resize`).
    Shell,
    /// No window resize at all (the frame proof's "without a resize" run).
    Off,
    /// The window follows the card on every frame (the frame proof's stress run).
    EveryFrame,
}

struct DragOrigin {
    cursor_x: i32,
    cursor_y: i32,
    win_x: i32,
    win_y: i32,
}

/// What the self-test reads back, beside what Win32 reports.
#[derive(Default, Clone)]
pub struct Trace {
    pub initial_rect: Option<Rect>,
    pub strip: [bool; 3],
    pub shown_cloaked: bool,
    /// Frames the card kept its size before the window was uncloaked.
    pub uncloak_frames: Option<u32>,
    pub uncloaked_at: Option<Instant>,
    /// Every size handed to the window, in DIPs.
    pub applied: Vec<(i32, i32)>,
    pub replays: u32,
    pub drag_ticks: u32,
    pub second_instance: u32,
    pub focus_calls: u32,
}

#[derive(Default)]
struct State {
    // electron/main.cjs:913
    wanted_height: Option<i32>,
    wanted_hovered: bool,
    shrink_task: Option<Task<()>>,
    drag: Option<DragOrigin>,
    drag_task: Option<Task<()>>,
    drag_stop: Option<Arc<AtomicBool>>,
    // electron/main.cjs:930
    fullscreen_from: Option<Rect>,
    // electron/main.cjs:253
    widget_outer_w: i32,
    // electron/main.cjs:922: the size the window last asked for, for a display-scale replay
    window_size: Option<(i32, i32)>,
    trace: Trace,
}

/// The window-level state and rules of the widget: what electron/main.cjs keeps in module variables.
pub struct Shell {
    pub hwnd: isize,
    pub test: bool,
    pub cursor: Cursor,
    pub display: Display,
    pub hovered: Cell<bool>,
    pub resize_mode: Cell<ResizeMode>,
    st: RefCell<State>,
}

impl Shell {
    fn new(hwnd: isize, test: bool, mode: ResizeMode) -> Rc<Self> {
        let st = State { widget_outer_w: WIDGET_W as i32, ..Default::default() };
        Rc::new(Self {
            hwnd,
            test,
            cursor: Cursor::default(),
            display: Display::default(),
            hovered: Cell::new(false),
            resize_mode: Cell::new(mode),
            st: RefCell::new(st),
        })
    }

    pub fn trace(&self) -> Trace {
        self.st.borrow().trace.clone()
    }
    pub fn is_fullscreen(&self) -> bool {
        self.st.borrow().fullscreen_from.is_some()
    }
    pub fn is_dragging(&self) -> bool {
        self.st.borrow().drag_task.is_some()
    }
    pub fn wanted(&self) -> (Option<i32>, bool) {
        let s = self.st.borrow();
        (s.wanted_height, s.wanted_hovered)
    }

    pub fn scale(&self) -> f32 {
        win32::dpi(self.hwnd).max(1) as f32 / 96.0
    }
    fn to_px(&self, dip: i32) -> i32 {
        (dip as f32 * self.scale()).ceil() as i32
    }
    /// win.getSize(), in DIPs.
    pub fn size_dip(&self) -> (i32, i32) {
        let v = win32::visible_rect(self.hwnd);
        let s = self.scale();
        ((v.w as f32 / s).round() as i32, (v.h as f32 / s).round() as i32)
    }

    /// Win32 geometry calls are sent through the executor, like GPUI's own `Window::resize`: the
    /// callers include prepaint and event handlers, where an immediate SetWindowPos would re-enter
    /// GPUI's window procedure while the app is borrowed.
    fn defer(self: &Rc<Self>, cx: &App, f: impl FnOnce(&Shell) + 'static) {
        let me = self.clone();
        cx.spawn(async move |_| f(&me)).detach();
    }

    // electron/main.cjs:952
    pub fn set_window_size(self: &Rc<Self>, cx: &App, width: i32, height: i32) {
        {
            let mut s = self.st.borrow_mut();
            s.window_size = Some((width, height));
            s.trace.applied.push((width, height));
        }
        self.defer(cx, move |me| {
            let r = Rect { x: 0, y: 0, w: me.to_px(width), h: me.to_px(height) };
            win32::set_visible_rect(me.hwnd, r, false, true);
        });
    }

    // electron/main.cjs:1795 (win:resize)
    pub fn win_resize(self: &Rc<Self>, cx: &App, height: f32, hovered: bool) {
        let h = (height.ceil() as i32).clamp(80, 1400);
        {
            let mut s = self.st.borrow_mut();
            s.wanted_height = Some(h);
            s.wanted_hovered = hovered;
        }
        self.apply_window_height(cx, h, hovered);
    }

    // electron/main.cjs:961
    fn apply_window_height(self: &Rc<Self>, cx: &App, h: i32, hovered: bool) {
        if self.is_fullscreen() {
            return;
        }
        // cancel any pending shrink first (clearTimeout): a task dropped never runs
        self.st.borrow_mut().shrink_task = None;
        // never resize mid-drag; stop_drag replays it
        if self.is_dragging() {
            return;
        }
        let (cw, current) = self.size_dip();
        // sub-pixel layout jitter would otherwise be a constant resize churn
        if (current - h).abs() < 3 {
            return;
        }
        if h > current {
            // grow at once, so content is never left clipped
            self.set_window_size(cx, cw, h);
            return;
        }
        // never shrink while hovered (the window pulling out from under the cursor flickers the widget),
        // and debounce it otherwise
        if hovered {
            return;
        }
        let me = self.clone();
        let task = cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(SHRINK_DEBOUNCE_MS)).await;
            // setTimeout's callback: `win.getSize()[0]` is read when it fires
            let (w, _) = me.size_dip();
            let cx_app = cx.clone();
            let _ = cx_app.update(|app| me.set_window_size(app, w, h));
        });
        self.st.borrow_mut().shrink_task = Some(task);
    }

    // electron/main.cjs:997
    pub fn replay_wanted_height(self: &Rc<Self>, cx: &App) {
        let (h, hovered) = self.wanted();
        if let Some(h) = h {
            self.st.borrow_mut().trace.replays += 1;
            self.apply_window_height(cx, h, hovered);
        }
    }

    /// The `EveryFrame` stress mode: the window height follows the card on every frame.
    fn force_height(self: &Rc<Self>, cx: &App, height: f32) {
        let h = height.ceil() as i32;
        let (w, cur) = self.size_dip();
        if h != cur {
            self.set_window_size(cx, w, h);
        }
    }

    // electron/main.cjs:876
    pub fn start_drag(self: &Rc<Self>, cx: &App) {
        if self.is_fullscreen() {
            return;
        }
        self.stop_drag(cx);
        let (cursor_x, cursor_y) = self.cursor.pos();
        let v = win32::visible_rect(self.hwnd);
        self.st.borrow_mut().drag = Some(DragOrigin { cursor_x, cursor_y, win_x: v.x, win_y: v.y });
        // main polls the cursor every 8 ms; the ticks come from a high-resolution timer thread (see
        // `win32::spawn_ticker`) and wake this task, which moves the window on the UI thread
        let (tx, rx) = async_channel::bounded::<()>(1);
        let stop = Arc::new(AtomicBool::new(false));
        win32::spawn_ticker(DRAG_INTERVAL_MS as i32, stop.clone(), move || !matches!(tx.try_send(()), Err(async_channel::TrySendError::Closed(_))));
        let me = self.clone();
        let task = cx.spawn(async move |_| {
            while rx.recv().await.is_ok() {
                if !me.drag_tick() {
                    break;
                }
            }
        });
        let mut s = self.st.borrow_mut();
        s.drag_task = Some(task);
        s.drag_stop = Some(stop);
    }

    /// One tick of the drag poll: window position = origin + cursor delta. False once the drag is over.
    fn drag_tick(&self) -> bool {
        let r = {
            let mut s = self.st.borrow_mut();
            let Some(o) = s.drag.as_ref() else { return false };
            let now = self.cursor.pos();
            let r = Rect { x: o.win_x + (now.0 - o.cursor_x), y: o.win_y + (now.1 - o.cursor_y), w: 0, h: 0 };
            s.trace.drag_ticks += 1;
            r
        };
        win32::set_visible_rect(self.hwnd, r, true, false);
        true
    }

    // electron/main.cjs:894
    pub fn stop_drag(self: &Rc<Self>, cx: &App) {
        {
            let mut s = self.st.borrow_mut();
            s.drag_task = None;
            s.drag = None;
            if let Some(stop) = s.drag_stop.take() {
                stop.store(true, Ordering::SeqCst);
            }
        }
        // a resize that arrived while the drag ran was dropped; replay the last request
        self.replay_wanted_height(cx);
    }

    // electron/main.cjs:932
    pub fn set_fullscreen(self: &Rc<Self>, cx: &App, on: bool) -> bool {
        let from = self.st.borrow().fullscreen_from;
        if on && from.is_none() {
            let bounds = win32::visible_rect(self.hwnd);
            let area = self.display.bounds(self.hwnd);
            self.st.borrow_mut().fullscreen_from = Some(bounds);
            self.defer(cx, move |me| win32::set_visible_rect(me.hwnd, area, true, true));
        } else if !on {
            if let Some(back) = from {
                self.st.borrow_mut().fullscreen_from = None;
                self.defer(cx, move |me| win32::set_visible_rect(me.hwnd, back, true, true));
                // the widget may have grown or shrunk while fullscreen
                self.replay_wanted_height(cx);
            }
        }
        self.is_fullscreen()
    }

    // electron/main.cjs:1643 (win:setWidth)
    pub fn set_width(self: &Rc<Self>, cx: &App, width: f32) {
        let w = (width.round() as i32).clamp(280, 600) + STAGE_PAD as i32 * 2;
        self.st.borrow_mut().widget_outer_w = w;
        // applied on the way back from fullscreen instead
        {
            let mut s = self.st.borrow_mut();
            if let Some(fs) = s.fullscreen_from.as_mut() {
                fs.w = (w as f32 * self.scale()).ceil() as i32;
                return;
            }
        }
        let (cw, ch) = self.size_dip();
        if cw == w {
            return;
        }
        self.set_window_size(cx, w, ch);
    }

    // electron/main.cjs:1656 (win:setHidden)
    pub fn set_hidden(self: &Rc<Self>, cx: &App, hidden: bool) {
        self.defer(cx, move |me| {
            if hidden {
                win32::hide(me.hwnd);
            } else if !win32::is_visible(me.hwnd) {
                // showInactive: never takes focus from what you are doing
                win32::show_no_activate(me.hwnd);
            }
        });
    }

    // electron/main.cjs:1801 (win:setAlwaysOnTop; Electron's "screen-saver" level is HWND_TOPMOST on Windows)
    pub fn set_always_on_top(self: &Rc<Self>, cx: &App, on: bool) {
        self.defer(cx, move |me| win32::set_topmost(me.hwnd, on));
    }

    // electron/main.cjs:1855 (win:minimize)
    pub fn minimize(self: &Rc<Self>, cx: &App) {
        self.defer(cx, move |me| if me.test { win32::minimize_inactive(me.hwnd) } else { win32::minimize(me.hwnd) });
    }
}

// ── the placeholder card ─────────────────────────────────────────────────────────────────────

/// Exact CSS cubic-bezier(x1,y1,x2,y2), from spikes/frame-driver. Phase 4's `css::Tween` replaces this;
/// it is here only so the placeholder card has a height that moves the way a CSS transition does.
struct Bezier {
    ax: f64,
    bx: f64,
    cx: f64,
    ay: f64,
    by: f64,
    cy: f64,
}
impl Bezier {
    fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;
        Self { ax, bx, cx, ay, by, cy }
    }
    fn x(&self, t: f64) -> f64 {
        ((self.ax * t + self.bx) * t + self.cx) * t
    }
    fn dx(&self, t: f64) -> f64 {
        (3.0 * self.ax * t + 2.0 * self.bx) * t + self.cx
    }
    fn y(&self, t: f64) -> f64 {
        ((self.ay * t + self.by) * t + self.cy) * t
    }
    fn solve_t(&self, x: f64) -> f64 {
        let mut t = x;
        for _ in 0..8 {
            let e = self.x(t) - x;
            if e.abs() < 1e-9 {
                return t;
            }
            let d = self.dx(t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= e / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        t = x;
        for _ in 0..60 {
            let e = self.x(t) - x;
            if e.abs() < 1e-9 {
                break;
            }
            if e > 0.0 {
                hi = t
            } else {
                lo = t
            }
            t = (lo + hi) / 2.0;
        }
        t
    }
    fn ease(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else if x >= 1.0 {
            1.0
        } else {
            self.y(self.solve_t(x))
        }
    }
}

pub const TWEEN: Duration = Duration::from_millis(350);

pub struct Card {
    from: f32,
    to: f32,
    start: Option<Instant>,
}
impl Card {
    fn new(h: f32) -> Self {
        Self { from: h, to: h, start: None }
    }
    /// The height the card is heading for (its final laid-out height).
    pub fn target(&self) -> f32 {
        self.to
    }
    pub fn active(&self, now: Instant) -> bool {
        self.start.map_or(false, |s| now.duration_since(s) < TWEEN)
    }
    pub fn height_at(&self, now: Instant) -> f32 {
        match self.start {
            Some(s) => {
                let p = (now.duration_since(s).as_secs_f64() / TWEEN.as_secs_f64()).min(1.0);
                // `ease` = cubic-bezier(.25,.1,.25,1)
                self.from + (self.to - self.from) * Bezier::new(0.25, 0.1, 0.25, 1.0).ease(p) as f32
            }
            None => self.to,
        }
    }
    /// A new tween from where the card is now (a CSS transition retargets from its current value).
    pub fn tween_to(&mut self, to: f32, now: Instant) {
        self.from = self.height_at(now);
        self.to = to;
        self.start = Some(now);
    }
}

// ── the view ─────────────────────────────────────────────────────────────────────────────────

pub struct Widget {
    pub shell: Rc<Shell>,
    pub card: Card,
    pub driver: FrameDriver,
    /// Paint the window with this colour behind everything (snapshot alpha recovery).
    pub backdrop: Option<Hsla>,
    wake: Option<Receiver<()>>,
    /// Cloak lifts once the card has kept its size for two frames (port.md 7.7).
    cloaked: bool,
    last_card_h: Option<i32>,
    same_frames: u32,
    frames_cloaked: u32,
    was_iconic: bool,
    /// Test hook: marks (label, time) the tween starts, for the frame log.
    pub marks: Vec<(usize, Instant)>,
}

impl Widget {
    pub fn start_tween(&mut self, to: f32, cx: &mut Context<Self>) {
        let now = Instant::now();
        let id = self.marks.len();
        self.marks.push((id, now));
        self.card.tween_to(to, now);
        cx.notify();
    }

    /// The card box was measured in prepaint (the ResizeObserver of src/App.tsx:1558).
    fn on_measure(&mut self, visible_h: f32, window: &mut Window, cx: &mut App) {
        let now = Instant::now();
        let animating = self.card.active(now);
        // port.md 7.8: while the card tweens, ask for the larger of what it is and what it is heading
        // for: a grow is applied once (to the final height), a shrink only repeats a request that
        // Electron's own 220 ms debounce keeps pushing back until the tween has ended.
        let req = (if animating { visible_h.max(self.card.target()) } else { visible_h }) + STAGE_PAD * 2.0;
        match self.shell.resize_mode.get() {
            ResizeMode::Shell => self.shell.win_resize(cx, req, self.shell.hovered.get()),
            ResizeMode::EveryFrame => self.shell.force_height(cx, visible_h + STAGE_PAD * 2.0),
            ResizeMode::Off => {}
        }
        if self.cloaked {
            self.frames_cloaked += 1;
            let h = visible_h.round() as i32;
            if self.last_card_h == Some(h) {
                self.same_frames += 1;
            } else {
                self.last_card_h = Some(h);
                self.same_frames = 1;
            }
            if self.same_frames >= 2 {
                self.cloaked = false;
                win32::set_cloak(self.shell.hwnd, false);
                {
                    let mut s = self.shell.st.borrow_mut();
                    s.trace.uncloak_frames = Some(self.frames_cloaked);
                    s.trace.uncloaked_at = Some(Instant::now());
                }
                if !self.shell.test {
                    window.activate_window();
                }
            } else {
                // a window that is not animating draws no further frames by itself
                window.request_animation_frame();
            }
        }
    }

    /// One tick of the poll loop: second launch, restore-from-minimize, cursor hover, drag release.
    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = self.shell.clone();
        let hwnd = shell.hwnd;

        let iconic_before = win32::is_minimized(hwnd);
        // electron/main.cjs:59 (second-instance)
        let mut woke = 0;
        if let Some(rx) = &self.wake {
            while rx.try_recv().is_ok() {
                woke += 1;
            }
        }
        for _ in 0..woke {
            let mut s = shell.st.borrow_mut();
            s.trace.second_instance += 1;
            if win32::is_minimized(hwnd) {
                win32::show_no_activate(hwnd); // restore()
            }
            if !win32::is_visible(hwnd) {
                win32::show_no_activate(hwnd); // show()
            }
            if !shell.test {
                window.activate_window(); // focus()
            }
            s.trace.focus_calls += 1;
        }

        // electron/main.cjs:1060 (restore): re-apply the last height request
        let iconic = win32::is_minimized(hwnd);
        if (self.was_iconic || iconic_before) && !iconic {
            shell.replay_wanted_height(cx);
        }
        self.was_iconic = iconic;

        // port.md 7.6: GPUI misses the leave event when the cursor leaves a transparent window quickly,
        // so hover comes from the cursor position against the window's drawn rect
        let (x, y) = shell.cursor.pos();
        let inside = !iconic && win32::is_visible(hwnd) && win32::visible_rect(hwnd).contains(x, y);
        if inside != shell.hovered.get() {
            shell.hovered.set(inside);
            cx.notify();
        }

        // src/App.tsx:954: the renderer ends the drag on pointerup / blur; the button is polled here
        // because GPUI does not capture the pointer
        if !shell.test && shell.is_dragging() && !win32::left_button_down() {
            shell.stop_drag(cx);
        }
    }

    /// Placement, show-cloaked and the poll loop. Called once with the window just created.
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = self.shell.clone();
        let hwnd = shell.hwnd;
        // port.md 7.7: the window must be shown to paint, but not seen until the card is laid out
        win32::set_cloak(hwnd, true);
        // electron/main.cjs:1009,1045: alwaysOnTop, raised to the "screen-saver" level (HWND_TOPMOST on Windows)
        win32::set_topmost(hwnd, true);
        shell.st.borrow_mut().trace.strip = win32::dwm_strip(hwnd);
        self.cloaked = true;

        cx.spawn_in(window, async move |this, cx| {
            // frameless + non-resizable: size = the card's window, position = test origin or centred
            // (Electron centres a window created without x/y)
            let (w, h) = (shell.to_px(WIDGET_W as i32), shell.to_px(WIDGET_H0 as i32));
            let (x, y) = if shell.test {
                TEST_ORIGIN
            } else {
                let (_, wa) = win32::primary_monitor();
                (wa.x + (wa.w - w) / 2, wa.y + (wa.h - h) / 2)
            };
            win32::set_visible_rect(hwnd, Rect { x, y, w, h }, true, true);
            {
                let mut s = shell.st.borrow_mut();
                s.trace.initial_rect = Some(win32::visible_rect(hwnd));
                s.trace.shown_cloaked = win32::is_cloaked(hwnd);
                s.window_size = Some((WIDGET_W as i32, WIDGET_H0 as i32));
            }
            win32::show_no_activate(hwnd);
            // the poll loop
            loop {
                cx.background_executor().timer(Duration::from_millis(POLL_MS)).await;
                if this.update_in(cx, |w, window, cx| w.poll(window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for Widget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let h = self.card.height_at(now);
        let animating = self.card.active(now);
        self.driver.tick(window, animating);

        let me: Entity<Widget> = cx.entity();
        let down = self.shell.clone();
        let up = self.shell.clone();
        let up2 = self.shell.clone();
        // .stage (src/App.css:101): the transparent padding the card sits in
        div()
            .size_full()
            .when_some(self.backdrop, |d, c| d.bg(c))
            .p(px(STAGE_PAD))
            // src/App.tsx:940: the drag starts on pointerdown (placeholder: anywhere on the card)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| down.start_drag(cx))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| up.stop_drag(cx))
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| up2.stop_drag(cx))
            .child(
                div()
                    .on_children_prepainted(move |bounds, window, cx| {
                        let visible_h = bounds.iter().map(|b| f32::from(b.size.height)).fold(0.0, f32::max);
                        me.update(cx, |this, cx| this.on_measure(visible_h, window, cx));
                    })
                    .w_full()
                    // placeholder card: not a ledger value, the real widget replaces it (phase 5)
                    .child(div().w_full().h(px(h)).rounded(px(18.0)).bg(rgb(0x1d1d21)).border_1().border_color(rgb(0x3a3a42))),
            )
            .child(self.driver.probe())
    }
}

/// Everything `open_widget` needs.
pub struct Opts {
    /// Test / snapshot / self-test: off-screen origin, no activation, no real cursor.
    pub test: bool,
    pub wake: Option<Receiver<()>>,
    pub resize_mode: ResizeMode,
}

pub fn open_widget(cx: &mut App, opts: Opts) -> (WindowHandle<Widget>, Rc<Shell>, FrameDriver) {
    let out: Rc<RefCell<Option<(Rc<Shell>, FrameDriver)>>> = Rc::new(RefCell::new(None));
    let out2 = out.clone();
    let test = opts.test;
    let mode = opts.resize_mode;
    let wake = RefCell::new(opts.wake);
    // GPUI refuses bounds that are not on a monitor and falls back to a default; the real placement is
    // done by `Widget::start` with Win32, while the window is hidden and cloaked.
    let bounds = Bounds { origin: point(px(100.0), px(100.0)), size: size(px(WIDGET_W), px(WIDGET_H0)) };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                is_minimizable: true,
                focus: false,
                show: false,
                // skipTaskbar: false (electron/main.cjs:1010): a normal window keeps its taskbar button
                kind: WindowKind::Normal,
                ..Default::default()
            },
            move |window, cx| {
                let hwnd = win32::hwnd_of(window);
                let shell = Shell::new(hwnd, test, mode);
                let driver = FrameDriver::default();
                *out2.borrow_mut() = Some((shell.clone(), driver.clone()));
                let view = cx.new(|_| Widget {
                    shell,
                    card: Card::new(CARD_H0),
                    driver,
                    backdrop: None,
                    wake: wake.borrow_mut().take(),
                    cloaked: false,
                    last_card_h: None,
                    same_frames: 0,
                    frames_cloaked: 0,
                    was_iconic: false,
                    marks: Vec::new(),
                });
                view.update(cx, |w, cx| w.start(window, cx));
                view
            },
        )
        .expect("open widget window");
    let (shell, driver) = out.borrow_mut().take().unwrap();
    (handle, shell, driver)
}
