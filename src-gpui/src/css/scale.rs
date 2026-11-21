//! CSS `scale` / `transform: scale()` (port.md 2.4 "scale"). A scale changes what is painted, not
//! layout: the box keeps its unscaled size in the flow and is drawn scaled around its centre
//! (`transform-origin: 50% 50%`). Blink lays the subtree out unscaled and pixel-snaps it (a 13.75px
//! box becomes 14px, measured on scale-pill-094), then draws every snapped rect through the transform,
//! unsnapped and antialiased by area (a 96px pill at .94 is 90.24px wide, its edges at fractions of a
//! pixel), with radii, shadows and text scaled too.
//!
//! GPUI has no transforms for elements, so the scaled box is a tree of `Node`s that this module paints
//! itself:
//! - layout: the tree is laid out unscaled as ordinary GPUI divs (taffy's whole-pixel rounding there
//!   stands in for Blink's snapping); an absolute canvas in each node records its laid-out bounds; the
//!   divs themselves are never painted;
//! - paint: each node at `c + (bounds − c) × s` (c = the centre of the scaled box's own bounds), in f32,
//!   with no second layout pass, so nested offsets are not rounded again.
//! - straight edges: a GPUI quad without radii or borders takes the shader's fast path and paints a
//!   pixel fully when its centre is inside (gpui-0.2.2 src/platform/windows/shaders.hlsl:528-535
//!   `quad_fragment`); its SDF path (any radius) antialiases only pixels whose centre is inside, since the
//!   quad's own bounds are what is rasterized (shaders.hlsl:94-97 `to_device_position`). So an unrounded
//!   fill is drawn as up to 9 pixel-aligned quads (interior, edge strips, corner pixels) whose alpha is
//!   the pixel's covered area: what Skia draws for an axis-aligned rect;
//! - rounded fills go to the SDF path as GPUI draws any rounded box (port.md 4, edge tolerance);
//! - shadows: `css::shadow`'s bakes at the scaled geometry (offset, blur, spread × s);
//! - text: laid out unscaled by `css::text` (Blink's width and line box), its glyphs drawn at the scaled
//!   font size from the scaled pen positions and baseline.
//! `opacity` (clamped to [0, 1] as CSS does) multiplies every painted colour.
//! Children are not clipped (no scaled box the app has overflows its parent).
//!
//! Limits:
//! - `opacity` fades each primitive, where CSS fades the composited group: where nodes overlap
//!   (the stand-in over the pill) a partly faded frame is lighter in CSS (scale-pill-in f001).

use super::rounded::{clamp, corners, Radii};
use super::shadow::{self, Shadow};
use super::text::{self as css_text, Style};
use gpui::{
    canvas, div, fill, point, prelude::*, px, quad, size, AnyElement, App, Bounds, Div, Element, ElementId,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Rgba, SharedString, Window,
};
use std::{cell::Cell, rc::Rc};

/// What a node paints.
enum Paint {
    Box { bg: Option<Rgba>, radii: Radii, shadows: Vec<Shadow> },
    Text { text: SharedString, style: Style },
}

/// One box or line of text in a scaled subtree: `layout` carries its layout style (size, padding,
/// flex, gap; no colours), the rest is what it paints.
pub struct Node {
    layout: Div,
    paint: Paint,
    children: Vec<Node>,
}

/// A box laid out by `layout`.
pub fn node(layout: Div) -> Node {
    Node { layout, paint: Paint::Box { bg: None, radii: [0.0; 4], shadows: Vec::new() }, children: Vec::new() }
}

/// One line of text (`white-space: nowrap`) set with `style` (`css::text`); `layout` places it.
pub fn text(layout: Div, text: impl Into<SharedString>, style: Style) -> Node {
    Node { layout, paint: Paint::Text { text: text.into(), style }, children: Vec::new() }
}

impl Node {
    pub fn bg(mut self, c: Rgba) -> Self {
        if let Paint::Box { bg, .. } = &mut self.paint {
            *bg = Some(c);
        }
        self
    }
    /// `border-radius` (CSS order; `rounded::clamp` applies).
    pub fn radii(mut self, r: Radii) -> Self {
        if let Paint::Box { radii, .. } = &mut self.paint {
            *radii = r;
        }
        self
    }
    /// `box-shadow` layers, outer and inset.
    pub fn shadows(mut self, s: &[Shadow]) -> Self {
        if let Paint::Box { shadows, .. } = &mut self.paint {
            *shadows = s.to_vec();
        }
        self
    }
    pub fn child(mut self, c: Node) -> Self {
        self.children.push(c);
        self
    }
}

/// `root` drawn at scale `s` around its centre, with `opacity`. Layout is the unscaled root's.
pub fn scale(s: f32, opacity: f32, root: Node) -> Scale {
    let mut nodes = Vec::new();
    let main = build(root, &mut nodes);
    // CSS clamps opacity to [0, 1] (a springy keyframe overshoots it)
    Scale { s, opacity: opacity.clamp(0.0, 1.0), main, nodes }
}

/// The node's unscaled layout element (with a bounds-recording canvas), its paint list entries in
/// paint order (a parent before its children).
fn build(n: Node, out: &mut Vec<(Paint, Rc<Cell<Option<Bounds<Pixels>>>>)>) -> AnyElement {
    let at = Rc::new(Cell::new(None));
    let rec = at.clone();
    let probe = canvas(move |b, _, _| rec.set(Some(b)), |_, _, _, _| {}).absolute().top_0().left_0().size_full();
    let mut el = n.layout;
    if let Paint::Text { text, style } = &n.paint {
        el = el.child(css_text::text(text.clone(), style.clone()));
    }
    out.push((n.paint, at));
    for c in n.children {
        el = el.child(build(c, out));
    }
    el.child(probe).into_any_element()
}

pub struct Scale {
    s: f32,
    opacity: f32,
    /// The unscaled tree: laid out and prepainted (its canvases record the bounds), never painted.
    main: AnyElement,
    nodes: Vec<(Paint, Rc<Cell<Option<Bounds<Pixels>>>>)>,
}

impl IntoElement for Scale {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Scale {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.main.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.main.prepaint(window, cx);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, _: &mut App) {
        let (s, o) = (self.s, self.opacity);
        // transform-origin 50% 50%
        let c = point(f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0, f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0);
        let map = |b: Bounds<Pixels>| Bounds {
            origin: point(px(c.x + (f32::from(b.origin.x) - c.x) * s), px(c.y + (f32::from(b.origin.y) - c.y) * s)),
            size: size(px(f32::from(b.size.width) * s), px(f32::from(b.size.height) * s)),
        };
        let fade = |c: Rgba| Rgba { a: c.a * o, ..c };
        for (p, at) in &self.nodes {
            let Some(b) = at.get() else { continue };
            let sb = map(b);
            match p {
                Paint::Box { bg, radii, shadows } => {
                    let r = clamp(radii.map(|v| v * s), f32::from(sb.size.width), f32::from(sb.size.height));
                    let scaled: Vec<Shadow> = shadows
                        .iter()
                        .map(|sh| Shadow { x: sh.x * s, y: sh.y * s, blur: sh.blur * s, spread: sh.spread * s, color: fade(sh.color), ..*sh })
                        .collect();
                    // CSS paint order: outer shadows, background, inset shadows
                    shadow::paint(false, sb, r, &scaled, window);
                    if let Some(bg) = bg {
                        if r.iter().all(|v| *v == 0.0) {
                            area_rect(sb, fade(*bg), window);
                        } else {
                            window.paint_quad(quad(sb, corners(r), Hsla::from(fade(*bg)), px(0.0), gpui::transparent_black(), Default::default()));
                        }
                    }
                    shadow::paint(true, sb, r, &scaled, window);
                }
                Paint::Text { text, style } => {
                    let line = css_text::shape(text, style, window);
                    let color = Hsla { a: style.color.a * o, ..style.color };
                    line.paint(sb.origin, line.line_box(style.line_height), s, color, window);
                }
            }
        }
    }
}

/// [start, end) device-pixel spans of [a, b) with the covered fraction of each.
fn spans(a: f32, b: f32) -> Vec<(f32, f32, f32)> {
    let (fa, fb) = (a.floor(), b.floor());
    if b <= a {
        return Vec::new();
    }
    if fa == fb {
        return vec![(fa, fa + 1.0, b - a)];
    }
    let mut v = Vec::with_capacity(3);
    if a > fa {
        v.push((fa, fa + 1.0, fa + 1.0 - a));
    }
    if a.ceil() < fb {
        v.push((a.ceil(), fb, 1.0));
    }
    if b > fb {
        v.push((fb, fb + 1.0, b - fb));
    }
    v
}

/// An unrounded fill at `b` (window px) with each pixel's alpha × its covered area.
fn area_rect(b: Bounds<Pixels>, c: Rgba, window: &mut Window) {
    let sf = window.scale_factor();
    let x0 = f32::from(b.origin.x) * sf;
    let y0 = f32::from(b.origin.y) * sf;
    let xs = spans(x0, x0 + f32::from(b.size.width) * sf);
    let ys = spans(y0, y0 + f32::from(b.size.height) * sf);
    for &(ya, yb, cy) in &ys {
        for &(xa, xb, cx) in &xs {
            let r = Bounds { origin: point(px(xa / sf), px(ya / sf)), size: size(px((xb - xa) / sf), px((yb - ya) / sf)) };
            window.paint_quad(fill(r, Hsla::from(Rgba { a: c.a * cx * cy, ..c })));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::spans;

    #[test]
    fn pixel_spans() {
        // the 0.94 stand-in's rows: 24.42 .. 37.58 (a 14px box at y 24 around centre 31)
        assert_eq!(spans(24.42, 37.58).len(), 3);
        let s = spans(24.42, 37.58);
        assert!((s[0].2 - 0.58).abs() < 1e-4 && s[1] == (25.0, 37.0, 1.0) && (s[2].2 - 0.58).abs() < 1e-4);
        assert_eq!(spans(3.0, 5.0), vec![(3.0, 5.0, 1.0)]);
        assert!((spans(3.2, 3.7)[0].2 - 0.5).abs() < 1e-4);
    }
}
