//! CSS `mask-image: linear-gradient(to bottom, …)` (port.md 2.4): the masked box's pixels are
//! multiplied by the gradient's alpha at their row. Every mask the app has is one of these:
//! `.lyrics-panel` (src/App.css:268-289: black 0%, black 82%, transparent 100%) and the fullscreen
//! panel (src/App.css:2781-2782: transparent 0%, black 12%, black 82%, transparent 100%).
//!
//! GPUI has no masks or layers, so the content is painted once per alpha level: the rows where the
//! mask is 1 by the box itself (clipped to them), and each device-pixel row where it is between 0 and
//! 1 by a copy of the box, laid out the same, clipped to that one row and drawn at the row's alpha
//! (the gradient at the row's centre, as Skia shades a gradient at pixel centres). Rows at 0 are not
//! painted; nor is anything outside the box (`mask-clip: border-box`).
//! The row alpha multiplies each primitive the row draws (GPUI's opacity), which equals masking the
//! row's composite as long as the content does not draw translucent shapes over each other there.
//!
//! `masked(stops, build)`: `build` makes the box (its size, its content); it is called once per
//! fading row, each frame. Margins and positioning belong on a wrapper around the `Masked`.

use gpui::{
    div, point, prelude::*, px, size, AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Window,
};
use std::rc::Rc;

/// A `to bottom` gradient's colour stops as (position 0..1, alpha 0..1), in order.
pub type Stops = &'static [(f32, f32)];

/// src/App.css:277-288 `.lyrics-panel`: black 0%, black 82%, transparent 100%
pub const LYRICS_PANEL: Stops = &[(0.0, 1.0), (0.82, 1.0), (1.0, 0.0)];
/// src/App.css:2781-2782 `.widget.fullscreen > .lyrics-panel`
pub const FULLSCREEN_PANEL: Stops = &[(0.0, 0.0), (0.12, 1.0), (0.82, 1.0), (1.0, 0.0)];

/// The gradient's alpha at `t` (0..1 down the box): linear between stops, the end stops' alpha beyond.
pub fn alpha_at(stops: Stops, t: f32) -> f32 {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    for w in stops.windows(2) {
        let ((p0, a0), (p1, a1)) = (w[0], w[1]);
        if t >= p0 && t <= p1 {
            return if p1 > p0 { a0 + (a1 - a0) * (t - p0) / (p1 - p0) } else { a1 };
        }
    }
    last.1
}

pub fn masked(stops: Stops, build: impl Fn() -> AnyElement + 'static) -> Masked {
    let build: Rc<dyn Fn() -> AnyElement> = Rc::new(build);
    Masked { stops, main: build(), build, rows: Vec::new() }
}

pub struct Masked {
    stops: Stops,
    build: Rc<dyn Fn() -> AnyElement>,
    main: AnyElement,
    /// (row in window px, the copy drawn there at the row's alpha)
    rows: Vec<(Bounds<Pixels>, AnyElement)>,
}

impl IntoElement for Masked {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Masked {
    type RequestLayoutState = ();
    /// The span of rows at alpha 1.
    type PrepaintState = Option<Bounds<Pixels>>;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.main.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        self.main.prepaint(window, cx);
        let sf = window.scale_factor();
        let top = f32::from(bounds.origin.y) * sf;
        let h = f32::from(bounds.size.height) * sf;
        self.rows.clear();
        if h <= 0.0 {
            return None;
        }
        let (mut solid0, mut solid1) = (i32::MAX, i32::MIN);
        for y in top.floor() as i32..(top + h).ceil() as i32 {
            let yc = y as f32 + 0.5;
            if yc < top || yc >= top + h {
                continue;
            }
            let a = alpha_at(self.stops, (yc - top) / h);
            if a >= 1.0 {
                solid0 = solid0.min(y);
                solid1 = solid1.max(y + 1);
            } else if a > 0.0 {
                let row = Bounds { origin: point(bounds.origin.x, px(y as f32 / sf)), size: size(bounds.size.width, px(1.0 / sf)) };
                let mut copy = div().w(bounds.size.width).h(bounds.size.height).opacity(a).child((self.build)()).into_any_element();
                copy.layout_as_root(size(AvailableSpace::Definite(bounds.size.width), AvailableSpace::Definite(bounds.size.height)), window, cx);
                copy.prepaint_at(bounds.origin, window, cx);
                self.rows.push((row, copy));
            }
        }
        (solid0 < solid1).then(|| Bounds {
            origin: point(bounds.origin.x, px(solid0 as f32 / sf)),
            size: size(bounds.size.width, px((solid1 - solid0) as f32 / sf)),
        })
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), solid: &mut Option<Bounds<Pixels>>, window: &mut Window, cx: &mut App) {
        if let Some(s) = *solid {
            let main = &mut self.main;
            window.with_content_mask(Some(ContentMask { bounds: s }), |window| main.paint(window, cx));
        }
        for (row, copy) in &mut self.rows {
            window.with_content_mask(Some(ContentMask { bounds: *row }), |window| copy.paint(window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops() {
        assert_eq!(alpha_at(LYRICS_PANEL, 0.5), 1.0);
        assert!((alpha_at(LYRICS_PANEL, 0.91) - 0.5).abs() < 1e-5);
        assert_eq!(alpha_at(LYRICS_PANEL, 1.0), 0.0);
        assert!((alpha_at(FULLSCREEN_PANEL, 0.06) - 0.5).abs() < 1e-5);
    }
}
