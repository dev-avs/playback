//! Rounded clipping (port.md 2.4, gotcha 7.3): CSS `overflow: hidden` + `border-radius` clips the
//! children to the rounded box; GPUI's content mask is a rectangle. So the clip stays rectangular
//! (`overflow_hidden()`), and every child that reaches into a rounded corner carries its own radii,
//! drawn on the part of it that is visible.
//!
//! Beyond that, a child the curve would cut anywhere but at its own corner (flush on one side only, or
//! thinner than the radius) is not representable: `clip_child` returns None and the caller must not
//! draw it as if it were.
//!
//! `clip_child` works that out for one child, in the parent's coordinates: the visible rectangle
//! (child ∩ parent) and its radii. A corner whose visible edges are both the parent's edges gets the
//! parent's radius (a fill or image flush to the corner, or reaching past it like `.widget-bg`'s
//! `inset: -60px`); a corner that lies inside the parent's curve gets 0 (`.top` inside the widget's
//! 10px padding). A child flush on one side only but still inside a corner's curve (the curve would
//! cut a notch no corner radius can draw) is not representable and returns None.
//!
//! `clamp` is CSS's radius scaling (CSS Backgrounds 3 §5.5, Blink `FloatRoundedRect::ConstrainRadii`):
//! when adjacent radii add up to more than a side, all radii shrink by the same factor (`999px` pills).

use gpui::{px, Bounds, Corners, Pixels};

/// Corner radii in px: top-left, top-right, bottom-right, bottom-left (the CSS order).
pub type Radii = [f32; 4];

pub const fn uniform(r: f32) -> Radii {
    [r, r, r, r]
}

/// CSS Backgrounds 3 §5.5: f = min(L / S) over the four sides; scale every radius by f if f < 1.
pub fn clamp(r: Radii, w: f32, h: f32) -> Radii {
    let [tl, tr, br, bl] = r;
    let mut f = 1.0f32;
    for (len, sum) in [(w, tl + tr), (h, tr + br), (w, br + bl), (h, bl + tl)] {
        if sum > 0.0 {
            f = f.min(len / sum);
        }
    }
    if f < 1.0 {
        [tl * f, tr * f, br * f, bl * f]
    } else {
        r
    }
}

/// The per-pixel coverage of a `w` × `h` box with `radii` (device px) as Chromium's software raster
/// clips to it (`overflow: hidden` + `border-radius`): `ceil(w)` × `ceil(h)` values, row-major. For a
/// clip baked into an image that is then drawn without radii. Skia fills the rounded rect as a path
/// (SkPath::addRRect: one conic per corner, weight √2/2), the conic chopped into two quads, each quad
/// flattened into 2^shift lines by its curvature (SkEdge.cpp `diff_to_shift`, accuracy 2), coverage the
/// line polygon's area (analytic AA). So the curve runs along chords, inside the circle; GPUI's
/// saturate(0.5 − sdf) follows the circle itself. Measured against Chromium's corners of a card fill
/// alone: within 20/255 (r 10) and 15/255 (r 13) on the top corners, up to 32/255 on the bottom ones (Skia's
/// scanline order; not modelled), where the circle is off by up to 52/255.
pub fn clip_mask(w: f32, h: f32, radii: Radii) -> Vec<f32> {
    let r = clamp(radii, w, h);
    let corners = r.map(chords);
    let (cw, ch) = (w.ceil() as usize, h.ceil() as usize);
    // per corner: whether (x, y) in box coordinates is inside it, in that corner's own frame (x, y from its sides)
    let local = |k: usize, x: f32, y: f32| match k {
        0 => (x, y),
        1 => (w - x, y),
        2 => (w - x, h - y),
        _ => (x, h - y),
    };
    let mut out = Vec::with_capacity(cw * ch);
    for j in 0..ch {
        for i in 0..cw {
            let (fx, fy) = (i as f32, j as f32);
            let area = ((fx + 1.0).min(w) - fx).clamp(0.0, 1.0) * ((fy + 1.0).min(h) - fy).clamp(0.0, 1.0);
            let near = (0..4).any(|k| {
                let (a, b) = local(k, fx + 0.5, fy + 0.5);
                r[k] > 0.0 && a < r[k] + 1.0 && b < r[k] + 1.0
            });
            if !near {
                out.push(area);
                continue;
            }
            const N: usize = 16;
            let mut hit = 0;
            for sy in 0..N {
                for sx in 0..N {
                    let (x, y) = (fx + (sx as f32 + 0.5) / N as f32, fy + (sy as f32 + 0.5) / N as f32);
                    if x > w || y > h {
                        continue;
                    }
                    let inside = (0..4).all(|k| {
                        let (a, b) = local(k, x, y);
                        a >= r[k] || b >= r[k] || a >= chord_x(&corners[k], b)
                    });
                    hit += inside as usize;
                }
            }
            out.push(hit as f32 / (N * N) as f32);
        }
    }
    out
}

/// One corner of radius `rad` in its own frame (the corner at the origin, the box towards +x, +y): the
/// polyline Skia fills, from (0, rad) to (rad, 0).
fn chords(rad: f32) -> Vec<(f32, f32)> {
    if rad <= 0.0 {
        return Vec::new();
    }
    // SkConic::chop at t = ½ of the conic (0, rad), (0, 0), (rad, 0), weight √2/2 (SkPath::addRRect)
    let w = std::f32::consts::FRAC_1_SQRT_2;
    let (p0, p2) = ((0.0, rad), (rad, 0.0));
    let m = (rad / (2.0 + 2.0 * w), rad / (2.0 + 2.0 * w));
    let (a, b) = ((0.0, rad / (1.0 + w)), (rad / (1.0 + w), 0.0));
    let mut pts = vec![p0];
    for (q0, q1, q2) in [(p0, a, m), (m, b, p2)] {
        // SkQuadraticEdge::setQuadraticWithoutUpdate(pts, shift 2): points in FDot6 << 2
        let fd = |v: f32| (v * 256.0) as i32;
        let dx = ((fd(q1.0) << 1) - fd(q0.0) - fd(q2.0)) >> 2;
        let dy = ((fd(q1.1) << 1) - fd(q0.1) - fd(q2.1)) >> 2;
        let (dx, dy) = (dx.abs(), dy.abs());
        let dist = if dx > dy { dx + (dy >> 1) } else { dy + (dx >> 1) };
        let dist = (dist + (1 << 4)) >> 5;
        let shift = (((32 - dist.leading_zeros()) >> 1) as i32).clamp(1, 6);
        let n = 1 << shift;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            let u = 1.0 - t;
            pts.push((u * u * q0.0 + 2.0 * u * t * q1.0 + t * t * q2.0, u * u * q0.1 + 2.0 * u * t * q1.1 + t * t * q2.1));
        }
    }
    pts
}

/// The polyline's x at height `y` (0 ≤ y < rad): y falls and x rises along it.
fn chord_x(pts: &[(f32, f32)], y: f32) -> f32 {
    for s in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (s[0], s[1]);
        if y <= y0 && y >= y1 {
            return if y0 > y1 { x0 + (x1 - x0) * (y0 - y) / (y0 - y1) } else { x1 };
        }
    }
    0.0
}

pub fn corners(r: Radii) -> Corners<Pixels> {
    Corners { top_left: px(r[0]), top_right: px(r[1]), bottom_right: px(r[2]), bottom_left: px(r[3]) }
}

/// A child rectangle (x, y, w, h) in the parent's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn bounds(&self, origin: gpui::Point<Pixels>) -> Bounds<Pixels> {
        Bounds { origin: gpui::point(origin.x + px(self.x), origin.y + px(self.y)), size: gpui::size(px(self.w), px(self.h)) }
    }
}

/// The visible part of `child` inside a parent of size `w` × `h` with radii `radii` and
/// `overflow: hidden`, and the radii it must be drawn with. None: entirely outside, or cut by a
/// corner's curve in a way no corner radius reproduces (see the module doc).
pub fn clip_child(w: f32, h: f32, radii: Radii, child: Rect) -> Option<(Rect, Radii)> {
    let r = clamp(radii, w, h);
    let x0 = child.x.max(0.0);
    let y0 = child.y.max(0.0);
    let x1 = (child.x + child.w).min(w);
    let y1 = (child.y + child.h).min(h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // per parent corner: the parent's corner point and the direction into the box
    let at = [(0.0, 0.0, 1.0, 1.0), (w, 0.0, -1.0, 1.0), (w, h, -1.0, -1.0), (0.0, h, 1.0, -1.0)];
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        let rad = r[i];
        if rad <= 0.0 {
            continue;
        }
        let (px_, py_, sx, sy) = at[i];
        // the visible point nearest that corner: if it is outside the corner's circle, the curve cuts the child
        let (nx, ny) = (px_.clamp(x0, x1), py_.clamp(y0, y1));
        let (dx, dy) = ((nx - px_) * sx, (ny - py_) * sy);
        if dx >= rad || dy >= rad {
            continue;
        }
        let (cx, cy) = (rad - dx, rad - dy);
        if cx * cx + cy * cy <= rad * rad {
            continue;
        }
        // representable only as the child's own corner: flush on both sides and covering the corner square
        if dx > 0.0 || dy > 0.0 || x1 - x0 < rad || y1 - y0 < rad {
            return None;
        }
        out[i] = rad;
    }
    if clamp(out, x1 - x0, y1 - y0) != out {
        return None;
    }
    Some((Rect::new(x0, y0, x1 - x0, y1 - y0), out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_mask_flattens_like_skia() {
        // diff_to_shift: two lines per quad at r 10, four at r 13 (fitted on the Chromium corners)
        assert_eq!(chords(10.0).len(), 1 + 2 * 2);
        assert_eq!(chords(13.0).len(), 1 + 2 * 4);
        // straight edges and the interior are whole; a fractional far edge is its area
        let m = clip_mask(300.0, 176.5, uniform(10.0));
        assert_eq!(m.len(), 300 * 177);
        assert_eq!(m[50 * 300], 1.0);
        assert_eq!(m[176 * 300 + 150], 0.5);
        // the chords run inside the circle: less than the circle's area at the curve
        assert!(m[6] < 0.37 && m[6] > 0.2);
    }

    #[test]
    fn pill_radius_clamps_to_half_height() {
        // src/windows/Settings.css:429-444 .st-reset-pill: border-radius 999px on a 96 × 17.75 box
        assert_eq!(clamp(uniform(999.0), 96.0, 17.75), uniform(999.0 * (17.75 / 1998.0)));
        assert_eq!(clamp(uniform(13.0), 200.0, 80.0), uniform(13.0));
    }

    #[test]
    fn flush_inset_and_notched_children() {
        // reaching past every edge (.widget-bg inset -60px): the parent's rect and radii
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(-60.0, -60.0, 320.0, 200.0)), Some((Rect::new(0.0, 0.0, 200.0, 80.0), uniform(13.0))));
        // inside the padding (10px > 13 × (1 − 1/√2)): square corners
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(10.0, 10.0, 44.0, 44.0)), Some((Rect::new(10.0, 10.0, 44.0, 44.0), [0.0; 4])));
        // flush left and top only at the top-left corner
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(0.0, 0.0, 50.0, 20.0)), Some((Rect::new(0.0, 0.0, 50.0, 20.0), [13.0, 0.0, 0.0, 0.0])));
        // flush left, 5px down: the curve notches it
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(0.0, 5.0, 50.0, 20.0)), None);
        // a 6px strip along the bottom: the bottom corners' curves cut it above its own corners
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(0.0, 74.0, 200.0, 6.0)), None);
        // flush along the whole bottom and tall enough: the bottom radii
        assert_eq!(clip_child(200.0, 80.0, uniform(13.0), Rect::new(0.0, 50.0, 200.0, 30.0)), Some((Rect::new(0.0, 50.0, 200.0, 30.0), [0.0, 0.0, 13.0, 13.0])));
    }
}
