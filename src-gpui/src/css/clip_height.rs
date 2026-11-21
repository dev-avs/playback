//! The `max-height` transition (port.md 2.4): a box whose `max-height` transitions while its content is
//! laid out at full size, e.g. `.lyrics-panel` (src/App.css:268-293: max-height 0 → var(--panel-h),
//! 350ms cubic-bezier(0.4, 0, 0.2, 1), overflow hidden) and `.controls` (src/App.css:892-910: 0 → 32px,
//! 200ms ease; `overflow: visible` while open).
//!
//! GPUI equivalent: a wrapper whose height is `min(content height, max-height now)` (the CSS used height
//! of a box with `height: auto` under a `max-height`), in Blink's LayoutUnits (`layout_unit`), with the
//! content inside at its own size. `clip` mirrors `overflow`: hidden clips the content to the wrapper
//! (GPUI clips rectangles only, see port.md 7.3), visible lets it spill while the wrapper's height still
//! moves whatever comes after it. GPUI's layout (taffy with rounding) then pixel-snaps the edges the
//! way Blink's paint does (round(y + h) − round(y)).

use super::layout_unit;
use super::tween::{Transition, Tween};
use gpui::{div, prelude::*, px, Div};

pub struct ClipHeight {
    max_height: Tween<f32>,
}

impl ClipHeight {
    /// A box at `max_height` px whose changes transition per `spec`.
    pub fn new(max_height: f32, spec: Transition) -> Self {
        Self { max_height: Tween::new(max_height, spec) }
    }

    /// The style's new max-height (px) at `now` (ms).
    pub fn set(&mut self, max_height: f32, now: f64) {
        self.max_height.set(max_height, now);
    }

    /// The used height of the box at `now` for content of `content_h` px (laid out at full size).
    pub fn height(&self, content_h: f32, now: f64) -> f32 {
        layout_unit(content_h.min(self.max_height.value(now)).max(0.0))
    }

    pub fn active(&self, now: f64) -> bool {
        self.max_height.active(now)
    }

    /// The wrapper div: the used height, the content inside it; `clip` = `overflow: hidden`.
    /// Style the wrapper (flex, alignment, margins) on the returned div; the content keeps its own size.
    pub fn wrap(&self, content_h: f32, now: f64, clip: bool, content: impl IntoElement) -> Div {
        let d = div().flex_shrink_0().h(px(self.height(content_h, now))).child(content);
        if clip {
            d.overflow_hidden()
        } else {
            d
        }
    }
}
