//! Slice 3a: the widget window shell and the proofs that run against it (`--selftest-shell`,
//! `--frame-log`, `--snapshot`). All of them open the widget off-screen (`widget::TEST_ORIGIN`).

pub mod cards;
pub mod framelog;
pub mod placement;
pub mod primitive;
pub mod selftest;
pub mod selftest_cards;
pub mod snapshot;
pub mod widget;
pub mod windows;

use gpui::{AsyncApp, Context, WindowHandle};
use std::time::Duration;
use widget::Widget;

pub async fn sleep(cx: &AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

/// Poll `f` every 5 ms until it is true or `max_ms` has passed.
pub async fn wait_until(cx: &AsyncApp, max_ms: u64, f: impl Fn() -> bool) -> bool {
    let mut waited = 0;
    while waited <= max_ms {
        if f() {
            return true;
        }
        sleep(cx, 5).await;
        waited += 5;
    }
    f()
}

/// Run `f` on the widget view from an async task.
pub fn with_widget<R>(
    cx: &AsyncApp,
    handle: WindowHandle<Widget>,
    f: impl FnOnce(&mut Widget, &mut Context<Widget>) -> R,
) -> Option<R> {
    cx.update(|app| handle.update(app, |w, _window, cx| f(w, cx)).ok()).ok().flatten()
}
