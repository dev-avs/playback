// The widget window: its height follows the page, it is dragged by hand, and
// it can stay on top. Ported from electron/main.cjs (applyWindowHeight,
// startDrag), which explains each rule at length.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Room around the card for its shadow, on every side (STAGE_PAD in main.cjs).
pub const STAGE_PAD: f64 = 22.0;

#[derive(Default)]
pub struct Widget {
    /// The last height the page asked for, and whether it was hovered: a
    /// request refused mid-drag is replayed once the drag ends.
    wanted: Mutex<Option<(f64, bool)>>,
    /// The running drag's flag; its thread stops when the flag clears. One
    /// flag per drag, so a new drag cannot keep an old thread going.
    drag: Mutex<Option<Arc<AtomicBool>>>,
    /// Bumped by every request, so a pending shrink knows it was overtaken.
    generation: Arc<AtomicU64>,
}

fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

fn logical_size(win: &WebviewWindow) -> (f64, f64) {
    let scale = win.scale_factor().unwrap_or(1.0);
    let s = win.inner_size().map(|s| s.to_logical::<f64>(scale)).unwrap_or(LogicalSize::new(0.0, 0.0));
    (s.width, s.height)
}

fn set_size(win: &WebviewWindow, width: f64, height: f64) {
    let _ = win.set_size(LogicalSize::new(width, height));
}

impl Widget {
    pub fn resize(&self, win: &WebviewWindow, height: f64, hovered: bool) {
        let h = height.ceil().clamp(80.0, 1400.0);
        *self.wanted.lock().unwrap() = Some((h, hovered));
        self.apply(win, h, hovered);
    }

    fn apply(&self, win: &WebviewWindow, h: f64, hovered: bool) {
        // any pending shrink is overtaken by this request
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if self.drag.lock().unwrap().is_some() {
            return;
        }
        let (w, current) = logical_size(win);
        if (current - h).abs() < 3.0 {
            return;
        }
        if h > current {
            set_size(win, w, h); // grow at once, nothing left clipped
            return;
        }
        // Shrinking under a hovering pointer is what made the widget flicker.
        if hovered {
            return;
        }
        let win = win.clone();
        let generation = self.generation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(220));
            if generation.load(Ordering::SeqCst) == gen {
                let (w, _) = logical_size(&win);
                set_size(&win, w, h);
            }
        });
    }

    fn replay(&self, win: &WebviewWindow) {
        if let Some((h, hovered)) = *self.wanted.lock().unwrap() {
            self.apply(win, h, hovered);
        }
    }

    pub fn set_width(&self, win: &WebviewWindow, width: f64) {
        let w = width.round().clamp(280.0, 600.0) + STAGE_PAD * 2.0;
        let (cw, ch) = logical_size(win);
        if cw != w {
            set_size(win, w, ch);
        }
    }

    /// Follows the cursor until drag_end, ~120 times a second.
    pub fn drag_start(&self, win: &WebviewWindow) {
        self.stop_drag();
        let Ok(origin) = win.outer_position() else { return };
        let start = cursor();
        let on = Arc::new(AtomicBool::new(true));
        *self.drag.lock().unwrap() = Some(on.clone());
        let win = win.clone();
        std::thread::spawn(move || {
            while on.load(Ordering::SeqCst) {
                let (x, y) = cursor();
                let _ = win.set_position(PhysicalPosition::new(origin.x + x - start.0, origin.y + y - start.1));
                std::thread::sleep(Duration::from_millis(8));
            }
        });
    }

    fn stop_drag(&self) {
        if let Some(on) = self.drag.lock().unwrap().take() {
            on.store(false, Ordering::SeqCst);
        }
    }

    pub fn drag_end(&self, win: &WebviewWindow) {
        self.stop_drag();
        self.replay(win);
    }
}
