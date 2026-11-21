//! CSS `:hover` + `transition` (port.md 2.4, gotcha 7.6).
//!
//! `Hover` is one element's hover state from two sources:
//! - GPUI's own hover (`on_hover` on the element): immediate, but it misses the leave when the cursor
//!   quits a transparent window quickly;
//! - the cursor poll of the window shell (`shell::widget::Cursor`, every 16 ms, an injected position in
//!   tests): the cursor in window coordinates, or None when it is outside the window. Outside always
//!   clears hover; inside, hover is whether the element's last painted bounds contain it.
//! A change of state is returned so the caller retargets its `HoverProp`s at that moment, which is what
//! a `:hover` rule with a `transition` does.

use super::tween::{Lerp, Transition, Tween};
use gpui::{Bounds, Pixels, Point};

#[derive(Default, Clone, Debug)]
pub struct Hover {
    hovered: bool,
    bounds: Option<Bounds<Pixels>>,
}

impl Hover {
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// The element's bounds (window coordinates), from its prepaint.
    pub fn set_bounds(&mut self, b: Bounds<Pixels>) {
        self.bounds = Some(b);
    }

    fn change(&mut self, h: bool) -> Option<bool> {
        if h == self.hovered {
            None
        } else {
            self.hovered = h;
            Some(h)
        }
    }

    /// GPUI's hover event for the element. Some(new state) when it changed.
    pub fn on_gpui_hover(&mut self, hovered: bool) -> Option<bool> {
        self.change(hovered)
    }

    /// One cursor-poll tick: `cursor` in window coordinates, None when outside the window.
    pub fn poll(&mut self, cursor: Option<Point<Pixels>>) -> Option<bool> {
        let inside = match (cursor, self.bounds) {
            (Some(p), Some(b)) => b.contains(&p),
            _ => false,
        };
        self.change(inside)
    }
}

/// One property with a `:hover` value and a transition between the two.
#[derive(Clone, Debug)]
pub struct HoverProp<T> {
    pub rest: T,
    pub hover: T,
    tween: Tween<T>,
}

impl<T: Lerp> HoverProp<T> {
    pub fn new(rest: T, hover: T, spec: Transition) -> Self {
        Self { rest, hover, tween: Tween::new(rest, spec) }
    }
    /// The hover state changed at `now` (ms): the style flips and the transition starts.
    pub fn follow(&mut self, hovered: bool, now: f64) {
        self.tween.set(if hovered { self.hover } else { self.rest }, now);
    }
    pub fn value(&self, now: f64) -> T {
        self.tween.value(now)
    }
    pub fn active(&self, now: f64) -> bool {
        self.tween.active(now)
    }
}
