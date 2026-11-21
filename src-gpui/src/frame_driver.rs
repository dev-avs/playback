//! Frame driver (port.md 2.5): one per window. While any tween, keyframe or spring is active,
//! `render` asks for the next frame with `window.request_animation_frame()`, which gives display-rate
//! frames (spikes/frame-driver: ~180 fps on this display, worst gap < 10 ms, with and without a resize in
//! flight). A background timer loop was rejected there (64 fps, gaps up to 21 ms).
//!
//! The driver also keeps a paint-time log. It is what `--frame-log` writes, and it flags a stall (a gap of
//! more than `STALL_MS` between two paints while something was animating) so a regression of the old
//! "2 frames, then nothing until a 250 ms idle timer" bug shows up on stderr.

use gpui::{canvas, px, App, IntoElement, Styled, Window};
use std::{cell::RefCell, rc::Rc, time::Instant};

/// A gap between two paints longer than this, while animating, is reported as a stall.
pub const STALL_MS: f64 = 50.0;

#[derive(Default)]
pub struct FrameLog {
    /// Paint instants, with whether the driver was asked to animate when it painted.
    pub frames: Vec<(Instant, bool)>,
    /// Number of stalls seen (see `STALL_MS`).
    pub stalls: u32,
    animating: bool,
}

#[derive(Clone, Default)]
pub struct FrameDriver {
    log: Rc<RefCell<FrameLog>>,
}

impl FrameDriver {
    /// Call from `render` with whether any tween / keyframe / spring of this window is active.
    pub fn tick(&self, window: &mut Window, animating: bool) {
        self.log.borrow_mut().animating = animating;
        if animating {
            window.request_animation_frame();
        }
    }

    /// A 1 px element that records the time of every paint. Put it anywhere in the tree.
    pub fn probe(&self) -> impl IntoElement {
        let log = self.log.clone();
        canvas(
            |_, _, _| {},
            move |_, _, _: &mut Window, _: &mut App| {
                let mut l = log.borrow_mut();
                let now = Instant::now();
                let animating = l.animating;
                if animating {
                    if let Some((prev, was)) = l.frames.last().copied() {
                        let gap = now.duration_since(prev).as_secs_f64() * 1000.0;
                        if was && gap > STALL_MS {
                            l.stalls += 1;
                            eprintln!("[frame-driver] stall: {gap:.1} ms between paints while animating");
                        }
                    }
                }
                l.frames.push((now, animating));
            },
        )
        .absolute()
        .size(px(1.0))
    }

    pub fn log(&self) -> Rc<RefCell<FrameLog>> {
        self.log.clone()
    }
}
