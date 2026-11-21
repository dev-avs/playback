//! Win32 helpers for the window shell: the DWM strip, cloak, z-order, rects, cursor, capture.
//! Everything here takes an `HWND` as `isize` so callers need no `windows` types.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_CLOAK,
    DWMWA_CLOAKED, DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_MINIMIZE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, WM_CLOSE,
};

/// A plain integer rect (physical pixels, screen coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

fn h(hwnd: isize) -> HWND {
    HWND(hwnd as *mut _)
}

/// The native handle of a GPUI window.
pub fn hwnd_of(window: &Window) -> isize {
    match HasWindowHandle::window_handle(window).expect("window handle").as_raw() {
        RawWindowHandle::Win32(w) => w.hwnd.get(),
        _ => panic!("not a Win32 window"),
    }
}

/// port.md 7.5: no DWM corner rounding, no border colour, no non-client rendering.
/// Returns whether each of the three DwmSetWindowAttribute calls succeeded
/// (corner, border, non-client rendering). Only the corner preference can be read back.
pub fn dwm_strip(hwnd: isize) -> [bool; 3] {
    unsafe {
        let corner = DWMWCP_DONOTROUND;
        let r1 = DwmSetWindowAttribute(
            h(hwnd),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _,
            std::mem::size_of_val(&corner) as u32,
        );
        let border: u32 = 0xFFFF_FFFE;
        let r2 = DwmSetWindowAttribute(
            h(hwnd),
            DWMWA_BORDER_COLOR,
            &border as *const _ as _,
            std::mem::size_of_val(&border) as u32,
        );
        let policy = DWMNCRP_DISABLED;
        let r3 = DwmSetWindowAttribute(
            h(hwnd),
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as _,
            std::mem::size_of_val(&policy) as u32,
        );
        [r1.is_ok(), r2.is_ok(), r3.is_ok()]
    }
}

fn dwm_u32(hwnd: isize, attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE) -> u32 {
    let mut v: u32 = 0;
    unsafe {
        let _ = DwmGetWindowAttribute(h(hwnd), attr, &mut v as *mut _ as _, 4);
    }
    v
}

/// DWMWA_WINDOW_CORNER_PREFERENCE read back (DWMWA_BORDER_COLOR and DWMWA_NCRENDERING_POLICY are write-only).
pub fn dwm_corner(hwnd: isize) -> u32 {
    dwm_u32(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE)
}

pub const CORNER_DONOTROUND: u32 = DWMWCP_DONOTROUND.0 as u32;

/// port.md 7.7: a cloaked window is shown (so it paints) but not composed on screen.
pub fn set_cloak(hwnd: isize, on: bool) {
    let v: u32 = on as u32;
    unsafe {
        let _ = DwmSetWindowAttribute(h(hwnd), DWMWA_CLOAK, &v as *const _ as _, 4);
    }
}

pub fn is_cloaked(hwnd: isize) -> bool {
    dwm_u32(hwnd, DWMWA_CLOAKED) != 0
}

pub fn show_no_activate(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(h(hwnd), SW_SHOWNOACTIVATE);
    }
}
pub fn hide(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(h(hwnd), SW_HIDE);
    }
}
pub fn minimize(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(h(hwnd), SW_MINIMIZE);
    }
}
pub fn is_visible(hwnd: isize) -> bool {
    unsafe { IsWindowVisible(h(hwnd)).as_bool() }
}
pub fn is_minimized(hwnd: isize) -> bool {
    unsafe { IsIconic(h(hwnd)).as_bool() }
}
pub fn post_close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(h(hwnd)), WM_CLOSE, Default::default(), Default::default());
    }
}

pub fn set_topmost(hwnd: isize, on: bool) {
    unsafe {
        let _ = SetWindowPos(
            h(hwnd),
            Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn style(hwnd: isize) -> u32 {
    unsafe { GetWindowLongPtrW(h(hwnd), GWL_STYLE) as u32 }
}
pub fn ex_style(hwnd: isize) -> u32 {
    unsafe { GetWindowLongPtrW(h(hwnd), GWL_EXSTYLE) as u32 }
}
pub fn dpi(hwnd: isize) -> u32 {
    unsafe { GetDpiForWindow(h(hwnd)) }
}

/// GetWindowRect: includes the invisible resize frame (port.md 7.5).
pub fn window_rect(hwnd: isize) -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h(hwnd), &mut r);
    }
    Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

/// The drawn window: the client area in screen coordinates. This is what Electron's
/// getBounds() reports for a frameless window, and what every rule in the ledger is about.
pub fn visible_rect(hwnd: isize) -> Rect {
    let mut c = RECT::default();
    let mut p = POINT { x: 0, y: 0 };
    unsafe {
        let _ = GetClientRect(h(hwnd), &mut c);
        let _ = ClientToScreen(h(hwnd), &mut p);
    }
    Rect { x: p.x, y: p.y, w: c.right - c.left, h: c.bottom - c.top }
}

/// Puts the drawn window (client area) at `r`, whatever invisible frame GetWindowRect adds.
pub fn set_visible_rect(hwnd: isize, r: Rect, move_it: bool, size_it: bool) {
    let win = window_rect(hwnd);
    let vis = visible_rect(hwnd);
    let (l, t) = (vis.x - win.x, vis.y - win.y);
    let (rr, bb) = (win.right() - vis.right(), win.bottom() - vis.bottom());
    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE;
    if !move_it {
        flags |= SWP_NOMOVE;
    }
    if !size_it {
        flags |= SWP_NOSIZE;
    }
    unsafe {
        let _ = SetWindowPos(h(hwnd), None, r.x - l, r.y - t, r.w + l + rr, r.h + t + bb, flags);
    }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

fn monitor_rects(mi: &MONITORINFO) -> (Rect, Rect) {
    let f = |r: RECT| Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
    (f(mi.rcMonitor), f(mi.rcWork))
}

/// (bounds, work area) of the primary monitor.
pub fn primary_monitor() -> (Rect, Rect) {
    unsafe {
        let m = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        monitor_rects(&mi)
    }
}

/// (bounds, work area) of the monitor nearest a window.
pub fn monitor_of(hwnd: isize) -> (Rect, Rect) {
    unsafe {
        let m = MonitorFromWindow(h(hwnd), MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        monitor_rects(&mi)
    }
}

/// Opens the timer resolution to 1 ms for the process, so an 8 ms drag poll keeps 8 ms.
pub fn raise_timer_resolution() {
    unsafe {
        windows::Win32::Media::timeBeginPeriod(1);
    }
}

/// Pixels of the whole window (incl. the transparent stage) via PrintWindow, as top-down BGRA.
/// Returns (width, height, bgra) of the window rect (invisible frame included).
pub fn capture_bgra(hwnd: isize) -> Option<(i32, i32, Vec<u8>)> {
    let r = window_rect(hwnd);
    if r.w <= 0 || r.h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: r.w,
            biHeight: -r.h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(dc, HGDIOBJ(bmp.0));
        // PW_RENDERFULLCONTENT (2): ask DWM for the composed content of a DirectComposition window
        let ok = PrintWindow(h(hwnd), dc, PRINT_WINDOW_FLAGS(2)).as_bool();
        let out = if ok {
            Some(std::slice::from_raw_parts(bits as *const u8, (r.w * r.h * 4) as usize).to_vec())
        } else {
            None
        };
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        out.map(|v| (r.w, r.h, v))
    }
}

/// True while the left button is physically down (read only; nothing is sent).
pub fn left_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

pub fn is_foreground(hwnd: isize) -> bool {
    unsafe { GetForegroundWindow() == h(hwnd) }
}

/// Minimize without handing activation to another window (test windows only).
pub fn minimize_inactive(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(h(hwnd), SW_SHOWMINNOACTIVE);
    }
}

/// Runs `f` every `period_ms` on its own thread until it returns false or `stop` is set. The timer is a
/// high-resolution waitable timer: GPUI's own timers (WinRT ThreadPoolTimer) and `Sleep` follow the system
/// tick (15.6 ms) and cannot keep an 8 ms drag poll.
pub fn spawn_ticker(period_ms: i32, stop: std::sync::Arc<std::sync::atomic::AtomicBool>, mut f: impl FnMut() -> bool + Send + 'static) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE,
    };
    std::thread::spawn(move || unsafe {
        // TIMER_ALL_ACCESS
        let Ok(t) = CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, 0x1F0003) else {
            return;
        };
        let due = -(period_ms as i64) * 10_000;
        if SetWaitableTimer(t, &due, period_ms, None, None, false).is_ok() {
            while WaitForSingleObject(t, INFINITE).0 == 0 && !stop.load(std::sync::atomic::Ordering::SeqCst) && f() {}
        }
        let _ = CloseHandle(t);
    });
}

pub fn is_window(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(h(hwnd))).as_bool() }
}

/// (bounds, work area) of the monitor nearest a point: Electron's `screen.getDisplayNearestPoint`.
pub fn monitor_near_point(x: i32, y: i32) -> (Rect, Rect) {
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        monitor_rects(&mi)
    }
}

/// Posts a client-area mouse message to one of our own windows (tests: nothing is sent to the real input
/// queue, the cursor does not move). `msg` is WM_RBUTTONDOWN etc., (x, y) in client pixels.
pub fn post_mouse(hwnd: isize, msg: u32, x: i32, y: i32) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    let lparam = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
    unsafe {
        let _ = PostMessageW(Some(h(hwnd)), msg, WPARAM(0), LPARAM(lparam as isize));
    }
}

pub const WM_RBUTTONDOWN: u32 = 0x0204;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;

/// A global hotkey on its own thread with its own message loop (GPUI owns the main thread's loop and
/// would drop a thread-level WM_HOTKEY). `on_press` runs on that thread.
pub struct Hotkey {
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;

impl Hotkey {
    /// None when the OS refuses (another program has the combination); Electron's `globalShortcut.register`
    /// returns false then and the app goes on without the shortcut.
    pub fn register(mods: u32, vk: u32, on_press: impl Fn() + Send + 'static) -> Option<Hotkey> {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
        use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
        let (tx, rx) = std::sync::mpsc::channel::<Option<u32>>();
        let join = std::thread::spawn(move || unsafe {
            let ok = RegisterHotKey(None, 1, HOT_KEY_MODIFIERS(mods), vk).is_ok();
            let _ = tx.send(if ok { Some(GetCurrentThreadId()) } else { None });
            if !ok {
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY {
                    on_press();
                }
            }
            let _ = UnregisterHotKey(None, 1);
        });
        match rx.recv().ok().flatten() {
            Some(thread_id) => Some(Hotkey { thread_id, join: Some(join) }),
            None => {
                let _ = join.join();
                None
            }
        }
    }

    /// Posts a WM_HOTKEY to the hotkey thread itself (tests: the real keys are never pressed).
    pub fn post_press(&self) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_HOTKEY};
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_HOTKEY, WPARAM(1), LPARAM(0));
        }
    }

    /// Unregisters and waits for the thread to be done (`globalShortcut.unregisterAll`).
    pub fn unregister(mut self) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Whether the window has a menu bar (`GetMenu`).
pub fn has_menu(hwnd: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetMenu;
    unsafe { !GetMenu(h(hwnd)).is_invalid() }
}
