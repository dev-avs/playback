//! CSS `transform: translate*()` (port.md 2.4 "translate"). A transform moves the painted box without
//! moving layout, and it is not pixel-snapped: Chromium draws a box translated by 1.37px with
//! antialiased edges at 1.37px (measured on the primitive pages: kf-vol-pill-in, kf-next-popup-in).
//! GPUI's `relative().top(..)` goes through taffy, which rounds every position to whole pixels, so a
//! relative offset would jump a pixel at a time. This element instead offsets its child in prepaint
//! with `Window::with_element_offset` (what GPUI's own scrolling uses), which keeps the fraction.

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels,
    Point, Window,
};

pub struct Translate {
    offset: Point<Pixels>,
    child: AnyElement,
}

/// `child` painted `offset` away from where layout puts it (layout is unchanged).
pub fn translate(offset: Point<Pixels>, child: impl IntoElement) -> Translate {
    Translate { offset, child: child.into_any_element() }
}

impl IntoElement for Translate {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Translate {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let offset = self.offset;
        let child = &mut self.child;
        window.with_element_offset(offset, |window| {
            child.prepaint(window, cx);
        });
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.paint(window, cx);
    }
}
