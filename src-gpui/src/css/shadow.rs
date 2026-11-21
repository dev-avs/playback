//! CSS `box-shadow` (port.md 2.4): outer shadows (any number of layers, blur, spread, offset) and
//! inset ones (the 0.5px hairline edges: `inset 0 0 0 0.5px`).
//!
//! GPUI's own `BoxShadow` cannot be used: it paints the whole blurred shape, also under the box, so a
//! translucent card (rgba(20, 20, 22, .72)) shows its shadow through itself; CSS clips an outer shadow
//! to the outside of the border box (CSS Backgrounds 3 §7.1.1) and an inset one to the inside. Its
//! 4-sample gaussian is also not Skia's blur. So each shadow is baked on the CPU, the way Chromium's
//! software raster (Skia) draws it, into one image per box, and painted as a sprite:
//! - shape: the border box offset by (x, y) and grown (outer) / shrunk (inset) by the spread; corner
//!   radii grow by the spread with the spec's adjustment for small radii (Blink
//!   `FloatRoundedRect::OutsetForShapeMargin`/shadow outset) and shrink by it for inset;
//! - coverage: pixel area (exact on straight edges, 16 × 16 samples in the corner squares), in 8 bits
//!   like Skia's A8 mask;
//! - blur: sigma = blur radius / 2 (CSS), drawn as Skia's `SkMaskBlurFilter` does for sigma ≥ 2: three
//!   box blurs of width d = floor(sigma × 3 √(2π) / 4 + 0.5) (both odd: centred; d even: two of width d
//!   offset by half a pixel either way, then one of d + 1), per axis. Measured on shadow-widget: Electron's
//!   16px blur falls off like sigma 7.5, the three-box width-15 filter's (sigma 8 has d = 15);
//! - clip: outer × (1 − box coverage), inset × box coverage, with the same area coverage;
//! - layers: the first listed on top, composited premultiplied.
//! Baked images are cached per (shadows, box size, sub-pixel position).
//!
//! Use: `shadowed(&shadows, radii, the_box)` paints the outer shadows of `the_box` under it (layout is
//! the box's); `inset(&shadows, radii)` is the first child of the box (absolute, inset 0), so it is
//! drawn over the box's background and under its content, as CSS paints it. Radii are CSS's
//! (`rounded::clamp` applies).

use super::rounded::{clamp, Radii};
use gpui::{
    canvas, point, prelude::*, px, size, AnyElement, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Rgba, Window,
};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// One `box-shadow` layer, in CSS px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
    pub inset: bool,
}

/// `<x> <y> <blur> <spread> <color>`
pub const fn outer(x: f32, y: f32, blur: f32, spread: f32, color: Rgba) -> Shadow {
    Shadow { x, y, blur, spread, color, inset: false }
}

/// `inset <x> <y> <blur> <spread> <color>`
pub const fn inset_shadow(x: f32, y: f32, blur: f32, spread: f32, color: Rgba) -> Shadow {
    Shadow { x, y, blur, spread, color, inset: true }
}

// ── geometry ──

#[derive(Clone, Copy, Debug)]
struct RRect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: Radii,
}

impl RRect {
    fn new(x: f32, y: f32, w: f32, h: f32, r: Radii) -> Self {
        let (w, h) = (w.max(0.0), h.max(0.0));
        RRect { x0: x, y0: y, x1: x + w, y1: y + h, r: clamp(r, w, h) }
    }

    fn inside(&self, x: f32, y: f32) -> bool {
        if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
            return false;
        }
        let [tl, tr, br, bl] = self.r;
        let circle = |cx: f32, cy: f32, r: f32| (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r;
        if x < self.x0 + tl && y < self.y0 + tl {
            return circle(self.x0 + tl, self.y0 + tl, tl);
        }
        if x > self.x1 - tr && y < self.y0 + tr {
            return circle(self.x1 - tr, self.y0 + tr, tr);
        }
        if x > self.x1 - br && y > self.y1 - br {
            return circle(self.x1 - br, self.y1 - br, br);
        }
        if x < self.x0 + bl && y > self.y1 - bl {
            return circle(self.x0 + bl, self.y1 - bl, bl);
        }
        true
    }

    /// Area of pixel [px, px+1) × [py, py+1) inside the shape.
    fn coverage(&self, px: i32, py: i32) -> f32 {
        let (fx, fy) = (px as f32, py as f32);
        let ox = ((fx + 1.0).min(self.x1) - fx.max(self.x0)).clamp(0.0, 1.0);
        let oy = ((fy + 1.0).min(self.y1) - fy.max(self.y0)).clamp(0.0, 1.0);
        if ox <= 0.0 || oy <= 0.0 {
            return 0.0;
        }
        let [tl, tr, br, bl] = self.r;
        let hits = |cx0: f32, cy0: f32, cx1: f32, cy1: f32| fx < cx1 && fx + 1.0 > cx0 && fy < cy1 && fy + 1.0 > cy0;
        let corner = (tl > 0.0 && hits(self.x0, self.y0, self.x0 + tl, self.y0 + tl))
            || (tr > 0.0 && hits(self.x1 - tr, self.y0, self.x1, self.y0 + tr))
            || (br > 0.0 && hits(self.x1 - br, self.y1 - br, self.x1, self.y1))
            || (bl > 0.0 && hits(self.x0, self.y1 - bl, self.x0 + bl, self.y1));
        if !corner {
            return ox * oy;
        }
        const N: i32 = 16;
        let mut n = 0;
        for j in 0..N {
            for i in 0..N {
                if self.inside(fx + (i as f32 + 0.5) / N as f32, fy + (j as f32 + 0.5) / N as f32) {
                    n += 1;
                }
            }
        }
        n as f32 / (N * N) as f32
    }
}

/// CSS Backgrounds 3 §7.1.1: an outer spread grows each radius by the spread, multiplied by
/// 1 + (r / spread − 1)³ when r < spread (so a square corner stays square).
fn spread_radius(r: f32, spread: f32) -> f32 {
    if spread <= 0.0 {
        return (r + spread).max(0.0);
    }
    if r >= spread {
        r + spread
    } else {
        let k = r / spread - 1.0;
        r + spread * (1.0 + k * k * k)
    }
}

// ── Skia's blur ──

/// W3C / Skia `PlanGauss` box width for `sigma`.
fn box_width(sigma: f32) -> i32 {
    ((sigma as f64 * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor() as i32).max(1)
}

/// The three box passes for `sigma` as (lo, hi) inclusive offsets.
fn passes(sigma: f32) -> [(i32, i32); 3] {
    let d = box_width(sigma);
    if d % 2 == 1 {
        let h = (d - 1) / 2;
        [(-h, h); 3]
    } else {
        let h = d / 2;
        [(-h, h - 1), (-h + 1, h), (-h, h)]
    }
}

fn blur_line(v: &mut [f32], tmp: &mut Vec<f32>, sigma: f32) {
    for (lo, hi) in passes(sigma) {
        let n = v.len() as i32;
        let w = (hi - lo + 1) as f32;
        tmp.clear();
        tmp.resize(v.len(), 0.0);
        // running sum over [i + lo, i + hi]
        let mut sum = 0.0f32;
        for k in lo..=hi {
            if k >= 0 && k < n {
                sum += v[k as usize];
            }
        }
        for i in 0..n {
            tmp[i as usize] = sum / w;
            let out = i + lo;
            let inn = i + hi + 1;
            if out >= 0 && out < n {
                sum -= v[out as usize];
            }
            if inn >= 0 && inn < n {
                sum += v[inn as usize];
            }
        }
        v.copy_from_slice(tmp);
    }
}

/// Gaussian blur of a w × h coverage grid (values 0..255) as Skia's mask blur draws it.
fn blur(grid: &mut [f32], w: usize, h: usize, sigma: f32) {
    assert!(sigma >= 2.0, "css::shadow: sigma {sigma} < 2 uses Skia's small-sigma kernel, not ported");
    let mut tmp = Vec::new();
    for row in grid.chunks_mut(w) {
        blur_line(row, &mut tmp, sigma);
    }
    let mut col = vec![0.0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = grid[y * w + x];
        }
        blur_line(&mut col, &mut tmp, sigma);
        for y in 0..h {
            grid[y * w + x] = col[y];
        }
    }
    for v in grid.iter_mut() {
        *v = v.round();
    }
}

// ── baking ──

/// The pixel grid a bake covers: origin and size in device px.
#[derive(Clone, Copy)]
struct Grid {
    x: i32,
    y: i32,
    w: usize,
    h: usize,
}

impl Grid {
    fn mask(&self, s: &RRect) -> Vec<f32> {
        let mut m = vec![0.0f32; self.w * self.h];
        for j in 0..self.h {
            for i in 0..self.w {
                m[j * self.w + i] = (s.coverage(self.x + i as i32, self.y + j as i32) * 255.0).round();
            }
        }
        m
    }
}

/// One shadow layer's shape on `grid`, 0..255 per pixel (blurred if it has a blur).
fn layer(grid: Grid, shape: &RRect, blur_px: f32) -> Vec<f32> {
    if blur_px <= 0.0 {
        return grid.mask(shape);
    }
    let sigma = blur_px / 2.0;
    let m = (3 * (box_width(sigma) / 2 + 1)) as usize;
    // blur on a grid with room for the spread-out tails, then crop to `grid`
    let big = Grid { x: grid.x - m as i32, y: grid.y - m as i32, w: grid.w + 2 * m, h: grid.h + 2 * m };
    let mut b = big.mask(shape);
    blur(&mut b, big.w, big.h, sigma);
    let mut out = vec![0.0f32; grid.w * grid.h];
    for j in 0..grid.h {
        out[j * grid.w..(j + 1) * grid.w].copy_from_slice(&b[(j + m) * big.w + m..(j + m) * big.w + m + grid.w]);
    }
    out
}

/// Composite the layers (first on top) and return straight-alpha BGRA bytes.
fn compose(grid: Grid, layers: &[(Rgba, Vec<f32>)]) -> Vec<u8> {
    let n = grid.w * grid.h;
    let mut acc = vec![[0.0f32; 4]; n];
    for (c, a) in layers.iter().rev() {
        for i in 0..n {
            let sa = c.a * a[i];
            let d = &mut acc[i];
            d[0] = c.r * sa + d[0] * (1.0 - sa);
            d[1] = c.g * sa + d[1] * (1.0 - sa);
            d[2] = c.b * sa + d[2] * (1.0 - sa);
            d[3] = sa + d[3] * (1.0 - sa);
        }
    }
    let mut out = Vec::with_capacity(n * 4);
    for p in acc {
        let a = p[3];
        let un = |v: f32| if a > 0.0 { ((v / a) * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
        out.extend_from_slice(&[un(p[2]), un(p[1]), un(p[0]), (a * 255.0).round().clamp(0.0, 255.0) as u8]);
    }
    out
}

/// Outer layers of a box at `b` (device px) with radii `r` (device px): the grid and its BGRA pixels.
fn bake_outer(b: Bounds<f32>, r: Radii, shadows: &[Shadow]) -> Option<(Grid, Vec<u8>)> {
    let bx = RRect::new(b.origin.x, b.origin.y, b.size.width, b.size.height, r);
    let shapes: Vec<(Shadow, RRect)> = shadows
        .iter()
        .filter(|s| !s.inset && s.color.a > 0.0)
        .map(|s| {
            let rr = bx.r.map(|v| spread_radius(v, s.spread));
            (*s, RRect::new(bx.x0 + s.x - s.spread, bx.y0 + s.y - s.spread, b.size.width + 2.0 * s.spread, b.size.height + 2.0 * s.spread, rr))
        })
        .collect();
    if shapes.is_empty() {
        return None;
    }
    let reach = |s: &Shadow| if s.blur > 0.0 { (3 * (box_width(s.blur / 2.0) / 2 + 1)) as f32 } else { 0.0 } + 1.0;
    let x0 = shapes.iter().map(|(s, k)| k.x0 - reach(s)).fold(f32::INFINITY, f32::min).floor() as i32;
    let y0 = shapes.iter().map(|(s, k)| k.y0 - reach(s)).fold(f32::INFINITY, f32::min).floor() as i32;
    let x1 = shapes.iter().map(|(s, k)| k.x1 + reach(s)).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let y1 = shapes.iter().map(|(s, k)| k.y1 + reach(s)).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let grid = Grid { x: x0, y: y0, w: (x1 - x0) as usize, h: (y1 - y0) as usize };
    let hole = grid.mask(&bx);
    let layers: Vec<(Rgba, Vec<f32>)> = shapes
        .iter()
        .map(|(s, k)| {
            let mut a = layer(grid, k, s.blur);
            for (v, c) in a.iter_mut().zip(&hole) {
                *v = *v / 255.0 * (1.0 - c / 255.0);
            }
            (s.color, a)
        })
        .collect();
    Some((grid, compose(grid, &layers)))
}

/// Inset layers of a box at `b` (device px): drawn inside the box only.
fn bake_inset(b: Bounds<f32>, r: Radii, shadows: &[Shadow]) -> Option<(Grid, Vec<u8>)> {
    let bx = RRect::new(b.origin.x, b.origin.y, b.size.width, b.size.height, r);
    let list: Vec<&Shadow> = shadows.iter().filter(|s| s.inset && s.color.a > 0.0).collect();
    if list.is_empty() {
        return None;
    }
    let x0 = bx.x0.floor() as i32;
    let y0 = bx.y0.floor() as i32;
    let grid = Grid { x: x0, y: y0, w: (bx.x1.ceil() as i32 - x0) as usize, h: (bx.y1.ceil() as i32 - y0) as usize };
    let inside = grid.mask(&bx);
    let layers: Vec<(Rgba, Vec<f32>)> = list
        .iter()
        .map(|s| {
            let rr = bx.r.map(|v| (v - s.spread).max(0.0));
            let hole = RRect::new(bx.x0 + s.x + s.spread, bx.y0 + s.y + s.spread, b.size.width - 2.0 * s.spread, b.size.height - 2.0 * s.spread, rr);
            let mut a = layer(grid, &hole, s.blur);
            for (v, c) in a.iter_mut().zip(&inside) {
                *v = (1.0 - *v / 255.0) * (c / 255.0);
            }
            (s.color, a)
        })
        .collect();
    Some((grid, compose(grid, &layers)))
}

thread_local! {
    static CACHE: RefCell<HashMap<Vec<u32>, Option<(Grid, Arc<RenderImage>)>>> = RefCell::new(HashMap::new());
}

fn key(kind: u32, b: Bounds<f32>, r: Radii, shadows: &[Shadow]) -> Vec<u32> {
    // the position only matters within a pixel; the grid follows the whole-pixel part
    let fx = b.origin.x - b.origin.x.floor();
    let fy = b.origin.y - b.origin.y.floor();
    let mut k = vec![kind, fx.to_bits(), fy.to_bits(), b.size.width.to_bits(), b.size.height.to_bits()];
    k.extend(r.iter().map(|v| v.to_bits()));
    for s in shadows {
        k.extend([s.x, s.y, s.blur, s.spread, s.color.r, s.color.g, s.color.b, s.color.a].map(f32::to_bits));
        k.push(s.inset as u32);
    }
    k
}

/// Paint the bake for a box at `bounds` (window px). Also used by `css::scale` for its scaled boxes.
pub(super) fn paint(inset: bool, bounds: Bounds<Pixels>, radii: Radii, shadows: &[Shadow], window: &mut Window) {
    let sf = window.scale_factor();
    let b = Bounds {
        origin: point(f32::from(bounds.origin.x) * sf, f32::from(bounds.origin.y) * sf),
        size: size(f32::from(bounds.size.width) * sf, f32::from(bounds.size.height) * sf),
    };
    let r = radii.map(|v| v * sf);
    let scaled: Vec<Shadow> =
        shadows.iter().map(|s| Shadow { x: s.x * sf, y: s.y * sf, blur: s.blur * sf, spread: s.spread * sf, ..*s }).collect();
    let k = key(inset as u32, b, r, &scaled);
    let (ix, iy) = (b.origin.x.floor() as i32, b.origin.y.floor() as i32);
    // bake at the box's sub-pixel position on a grid relative to its whole-pixel origin
    let local = Bounds { origin: point(b.origin.x - ix as f32, b.origin.y - iy as f32), size: b.size };
    let baked = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 512 {
            c.clear();
        }
        c.entry(k)
            .or_insert_with(|| {
                let out = if inset { bake_inset(local, r, &scaled) } else { bake_outer(local, r, &scaled) };
                out.map(|(g, bgra)| {
                    let img = image::RgbaImage::from_raw(g.w as u32, g.h as u32, bgra).expect("bake size");
                    (g, Arc::new(RenderImage::new(vec![image::Frame::new(img)])))
                })
            })
            .clone()
    });
    if let Some((g, img)) = baked {
        let at = Bounds {
            origin: point(px((ix + g.x) as f32 / sf), px((iy + g.y) as f32 / sf)),
            size: size(px(g.w as f32 / sf), px(g.h as f32 / sf)),
        };
        let _ = window.paint_image(at, Default::default(), img, 0, false);
    }
}

/// `the_box` with its outer `box-shadow` layers painted under it. Layout is the box's own.
pub fn shadowed(shadows: &[Shadow], radii: Radii, the_box: impl IntoElement) -> Shadowed {
    Shadowed { shadows: shadows.to_vec(), radii, child: the_box.into_any_element() }
}

pub struct Shadowed {
    shadows: Vec<Shadow>,
    radii: Radii,
    child: AnyElement,
}

impl IntoElement for Shadowed {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Shadowed {
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
        self.child.prepaint(window, cx);
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        paint(false, bounds, self.radii, &self.shadows, window);
        self.child.paint(window, cx);
    }
}

/// The inset `box-shadow` layers of the box this is the first child of (absolute, inset 0: the
/// padding box of a box without borders, which is every box the app gives an inset shadow).
pub fn inset(shadows: &[Shadow], radii: Radii) -> impl IntoElement {
    let shadows = shadows.to_vec();
    canvas(|_, _, _| (), move |bounds, _, window, _| paint(true, bounds, radii, &shadows, window)).absolute().top_0().left_0().size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skia_box_widths() {
        // blur 16px (src/App.css:126-128) → sigma 8 → d 15; blur 10px (Settings.css:512-515) → sigma 5 → d 9;
        // blur 30px (Settings.css:270-272) → sigma 15 → d 28 (even)
        assert_eq!(box_width(8.0), 15);
        assert_eq!(box_width(5.0), 9);
        assert_eq!(box_width(15.0), 28);
        assert_eq!(passes(15.0), [(-14, 13), (-13, 14), (-14, 14)]);
    }

    #[test]
    fn spread_radius_rule() {
        assert_eq!(spread_radius(13.0, 0.5), 13.5);
        assert_eq!(spread_radius(0.0, 0.5), 0.0);
    }
}
