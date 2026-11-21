//! The async runtime the backend runs on: the host's own (Tauri's tokio
//! runtime, or the one the GPUI build starts), handed over once at launch,
//! so spawning works from any thread.

use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

static HANDLE: OnceLock<Handle> = OnceLock::new();

/// Call once, before anything else in this crate runs.
pub fn init(handle: Handle) {
    let _ = HANDLE.set(handle);
}

fn handle() -> &'static Handle {
    HANDLE.get().expect("playback_core::rt::init was not called")
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle().spawn(future)
}

pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    handle().spawn_blocking(f)
}
