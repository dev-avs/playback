//! The card windows (settings, changelog, past changelog, setup, update prompt, Version control): the
//! window shell around a placeholder card whose height can tween, so the window behaviour can be proven
//! before any screen exists (parity/ledger.md, area `shell`). They open the way the widget does (hidden,
//! cloaked, DWM strip, SetWindowPos, SW_SHOWNOACTIVATE, uncloak after 2 stable frames) and size themselves
//! to their card through `card_resize`, the `settings:resize` / `changelog:resize` IPC of electron/main.cjs.

use super::placement::CardKind;
use super::widget::{Card, ResizeMode, Shell, STAGE_PAD};
use super::windows::Windows;
use crate::frame_driver::FrameDriver;
use crate::win32::{self, Rect};
use gpui::{
    div, point, prelude::*, px, rgb, size, App, Bounds, Context, Entity, MouseButton, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{rc::Rc, time::{Duration, Instant}};

// the poll of port.md 7.6 (one display frame at 60 Hz), as the widget's
const POLL_MS: u64 = 16;

/// What the self-test reads back, beside what Win32 reports.
#[derive(Default, Clone)]
pub struct CardTrace {
    pub strip: [bool; 3],
    pub initial_rect: Option<Rect>,
    pub shown_cloaked: bool,
    /// Frames the card kept its size before the window was uncloaked.
    pub uncloak_frames: Option<u32>,
    pub focus_calls: u32,
    /// What the settings window was last told about What's new (`changelog:openChanged`).
    pub changelog_open: Option<bool>,
    /// The hash query the page was loaded with (`?from=<version>`).
    pub query: Option<String>,
}

pub struct CardView {
    pub kind: CardKind,
    pub shell: Rc<Shell>,
    pub card: Card,
    pub driver: FrameDriver,
    pub trace: CardTrace,
    windows: Rc<Windows>,
    /// Where the window goes, given the display scale of this window (set once, used by `start`).
    place: Option<Box<dyn FnOnce(f32) -> (i32, i32)>>,
    topmost: bool,
    cloaked: bool,
    /// The window has been put where it opens: before that GPUI's own first size is not the card's.
    placed: bool,
    last_card_h: Option<i32>,
    same_frames: u32,
    frames_cloaked: u32,
}

// electron/main.cjs:1622 (changelog:resize) and 1662 (settings:resize): the height is clamped to 120..1000
// (ceil first) and a change under 2 px is ignored. Settings keeps its constant width, the others the width
// they have.
pub fn card_resize(shell: &Rc<Shell>, cx: &App, kind: CardKind, height: f32) {
    let h = (height.ceil() as i32).clamp(120, 1000);
    let (width, current) = shell.size_dip();
    if (current - h).abs() < 2 {
        return;
    }
    let w = if kind == CardKind::Settings { kind.width() } else { width };
    shell.set_window_size(cx, w, h);
}

impl CardView {
    pub fn start_tween(&mut self, to: f32, cx: &mut Context<Self>) {
        self.card.tween_to(to, Instant::now());
        cx.notify();
    }

    // electron/main.cjs:1091 and its siblings: opening an open card only focuses it
    pub fn focus(&mut self, window: &mut Window) {
        self.trace.focus_calls += 1;
        if !self.shell.test {
            window.activate_window();
        }
    }

    /// The card box was measured in prepaint (the ResizeObserver of src/windows/Changelog.tsx:137).
    fn on_measure(&mut self, visible_h: f32, window: &mut Window, cx: &mut App) {
        if !self.placed {
            window.request_animation_frame();
            return;
        }
        card_resize(&self.shell, cx, self.kind, visible_h + STAGE_PAD * 2.0);
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
                self.trace.uncloak_frames = Some(self.frames_cloaked);
                // BrowserWindow.show() takes focus (ready-to-show handlers, electron/main.cjs:1123 and siblings)
                if !self.shell.test {
                    window.activate_window();
                }
            } else {
                // a window that is not animating draws no further frames by itself
                window.request_animation_frame();
            }
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let shell = self.shell.clone();
        shell.poll_scale(cx);
        // src/windows/Settings.tsx:605: the renderer ends a drag on pointerup / blur; GPUI does not capture
        // the pointer, so the button is polled
        if !shell.test && shell.is_dragging() && !win32::left_button_down() {
            shell.stop_drag(cx);
        }
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = self.shell.clone();
        let hwnd = shell.hwnd;
        let (kind, topmost) = (self.kind, self.topmost);
        let pos = self.place.take().map_or((100, 100), |p| p(shell.scale()));
        // a close that does not come from `Windows` (WM_CLOSE): the handlers of its `closed` event that have to
        // run before the window is gone
        let windows = self.windows.clone();
        window.on_window_should_close(cx, move |_, cx| {
            windows.before_close(cx, kind);
            true
        });
        // port.md 7.7: shown to paint, not seen until the card is laid out (electron/main.cjs:1112 `show: false`
        // then `ready-to-show`)
        win32::set_cloak(hwnd, true);
        // electron/main.cjs:1109, 1122 (settings), 1169, 1182 (the others): alwaysOnTop, "screen-saver" level
        win32::set_topmost(hwnd, topmost);
        self.trace.strip = win32::dwm_strip(hwnd);
        self.cloaked = true;
        cx.spawn_in(window, async move |this, cx| {
            let (w, h) = (shell.to_px(kind.width()), shell.to_px(kind.height0()));
            win32::set_visible_rect(hwnd, Rect { x: pos.0, y: pos.1, w, h }, true, true);
            let _ = this.update(cx, |me, _| {
                me.placed = true;
                me.trace.initial_rect = Some(win32::visible_rect(hwnd));
                me.trace.shown_cloaked = win32::is_cloaked(hwnd);
            });
            shell.set_initial_size(kind.width(), kind.height0());
            win32::show_no_activate(hwnd);
            loop {
                cx.background_executor().timer(Duration::from_millis(POLL_MS)).await;
                if this.update_in(cx, |me, _window, cx| me.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for CardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let h = self.card.height_at(now);
        self.driver.tick(window, self.card.active(now));

        let me: Entity<CardView> = cx.entity();
        let kind = self.kind;
        let windows = self.windows.clone();
        let down = self.shell.clone();
        let up = self.shell.clone();
        let up2 = self.shell.clone();
        let settings = kind == CardKind::Settings;
        // .stage (src/index.css .stage, src/App.css:101): the transparent padding the card sits in
        div()
            .size_full()
            .p(px(STAGE_PAD))
            // src/windows/Settings.tsx:1343: right-click anywhere in Settings closes it; the other cards
            // only swallow the context menu (Changelog.tsx:148, Setup.tsx:161, Update.tsx:75, Versions.tsx:239).
            // stop_propagation (port.md 7.9): one right-click is one close.
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                cx.stop_propagation();
                if settings {
                    windows.right_click_close(cx, kind);
                }
            })
            .child(
                div()
                    .on_children_prepainted(move |bounds, window, cx| {
                        let visible_h = bounds.iter().map(|b| f32::from(b.size.height)).fold(0.0, f32::max);
                        me.update(cx, |this, cx| this.on_measure(visible_h, window, cx));
                    })
                    .w_full()
                    // src/windows/Settings.tsx:1349: Settings drags from anywhere on its card (pointerdown)
                    .when(settings, |d| {
                        d.on_mouse_down(MouseButton::Left, move |_, _, cx| down.start_drag(cx))
                            .on_mouse_up(MouseButton::Left, move |_, _, cx| up.stop_drag(cx))
                            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| up2.stop_drag(cx))
                    })
                    // placeholder card: not a ledger value, the real card replaces it (phases 6 and 7)
                    .child(div().w_full().h(px(h)).rounded(px(18.0)).bg(rgb(0x1d1d21)).border_1().border_color(rgb(0x3a3a42))),
            )
            .child(self.driver.probe())
    }
}

/// Opens a card window, hidden and cloaked, at `place(scale)` (physical px, top-left of the drawn window).
pub fn open_card_window(
    cx: &mut App,
    windows: &Rc<Windows>,
    kind: CardKind,
    query: Option<String>,
    place: Box<dyn FnOnce(f32) -> (i32, i32)>,
    topmost: bool,
    test: bool,
) -> (WindowHandle<CardView>, Rc<Shell>) {
    let out: Rc<std::cell::RefCell<Option<Rc<Shell>>>> = Rc::default();
    let out2 = out.clone();
    let windows = windows.clone();
    let windows2 = windows.clone();
    // GPUI refuses bounds that are not on a monitor; the real placement is done with Win32 in `start`
    let bounds = Bounds { origin: point(px(100.0), px(100.0)), size: size(px(kind.width() as f32), px(kind.height0() as f32)) };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                focus: false,
                show: false,
                // skipTaskbar: true (electron/main.cjs:1110): a popup window has no taskbar button
                kind: WindowKind::PopUp,
                ..Default::default()
            },
            move |window, cx| {
                let hwnd = win32::hwnd_of(window);
                // the card's own sizing rules are card_resize's; the shell is the widget's, for its size, drag
                // and display-scale replay
                let shell = Shell::new(hwnd, test, ResizeMode::Off);
                *out2.borrow_mut() = Some(shell.clone());
                let view = cx.new(|_| CardView {
                    kind,
                    shell,
                    // the card inside the window: the window's first height less the stage on both sides
                    card: Card::new(kind.height0() as f32 - STAGE_PAD * 2.0),
                    driver: FrameDriver::default(),
                    trace: CardTrace { query, ..Default::default() },
                    windows,
                    place: Some(place),
                    topmost,
                    cloaked: false,
                    placed: false,
                    last_card_h: None,
                    same_frames: 0,
                    frames_cloaked: 0,
                });
                // electron/main.cjs `closed`: the registry entry goes, whoever closed the window
                let (w2, hwnd) = (windows2.clone(), hwnd);
                cx.observe_release(&view, move |_, cx| w2.on_card_closed(cx, kind, hwnd)).detach();
                view.update(cx, |v, cx| v.start(window, cx));
                view
            },
        )
        .expect("open card window");
    let shell = out.borrow_mut().take().unwrap();
    (handle, shell)
}
