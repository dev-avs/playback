//! `radial-gradient` (port.md 2.4: CPU bake). Every radial gradient the app uses is one of the three
//! "black and white glass" backgrounds shown when there is no cover: two white radial glows over a
//! 160deg linear gradient (src/App.css:2193-2200 `.widget-bg-mono`, src/windows/Settings.css:40-48
//! `.st-bg-mono`, src/windows/Changelog.css:54-62 `.cl-bg-mono`). The whole `background` is baked on the
//! CPU once per card size, at the card's device pixels, and painted 1:1 as one image with the card's radii.
//!
//! Not at half size (port.md 7.14), measured: GPUI samples images bilinearly from an atlas without
//! padding (gpui-0.2.2 directx_atlas.rs:255, wrap addressing), so an image scaled up 2× mixes its outer
//! pixel row with its atlas neighbour: a frame 1px wide, up to 19.5/255 off, round every card (1602
//! pixels on radial-st-mono, passing diff.mjs only as tolerated shape edges). At device size the straight
//! edges are within 2/255 and only the corners' AA is left (36 edge pixels).
//!
//! As Chromium (Skia) draws it, measured with the primitive pages:
//! - layers: the first listed on top, composited premultiplied (src-over);
//! - radial-gradient(<rx>% <ry>% at <cx>% <cy>%, c, transparent <stop>): an ellipse with radii of the
//!   box's width / height, the ray t = |((x − cx) / rx, (y − cy) / ry)|, interpolated premultiplied (so
//!   the white fades by alpha only, no grey fringe): alpha = a × (1 − t / stop), 0 past the stop;
//! - linear-gradient(<angle>, from, to): CSS Images 3 §3.1, direction (sin θ, −cos θ), gradient line
//!   length |w sin θ| + |h cos θ| through the box centre;
//! - evaluated at pixel centres; the positioning area is the element's box (no padding).
//! Chromium dithers the result (a ±0.5/255 ordered pattern on the linear gradient, measured); that is
//! below one 8-bit step, so the bake rounds to the nearest value instead (interior within 2/255).

use super::rounded::{clamp, corners, Radii};
use gpui::{canvas, div, prelude::*, px, size, Bounds, Pixels, RenderImage, Rgba, Window};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// One `radial-gradient(<rx>% <ry>% at <cx>% <cy>%, <color>, transparent <stop>%)`, as fractions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radial {
    pub rx: f32,
    pub ry: f32,
    pub cx: f32,
    pub cy: f32,
    pub color: Rgba,
    pub stop: f32,
}

/// `linear-gradient(<angle>deg, <from>, <to>)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linear {
    pub angle: f32,
    pub from: Rgba,
    pub to: Rgba,
}

/// A `background` of radial layers (first listed on top) over a linear gradient.
#[derive(Debug, PartialEq)]
pub struct Background {
    pub radials: &'static [Radial],
    pub linear: Linear,
}

const fn white(a: f32) -> Rgba {
    Rgba { r: 1.0, g: 1.0, b: 1.0, a }
}

const fn hex(v: u32) -> Rgba {
    Rgba { r: ((v >> 16) & 0xff) as f32 / 255.0, g: ((v >> 8) & 0xff) as f32 / 255.0, b: (v & 0xff) as f32 / 255.0, a: 1.0 }
}

// linear-gradient(160deg, #1d1d21, #0b0b0d) (src/App.css:2199, src/windows/Settings.css:47, src/windows/Changelog.css:61)
const GLASS: Linear = Linear { angle: 160.0, from: hex(0x1d1d21), to: hex(0x0b0b0d) };

// src/App.css:2196-2199 .widget-bg.widget-bg-mono
pub const WIDGET_MONO: Background = Background {
    radials: &[
        Radial { rx: 0.45, ry: 0.40, cx: 0.22, cy: 0.18, color: white(0.16), stop: 0.70 },
        Radial { rx: 0.45, ry: 0.45, cx: 0.82, cy: 0.88, color: white(0.08), stop: 0.70 },
    ],
    linear: GLASS,
};
// src/windows/Settings.css:44-47 .st-bg.st-bg-mono
pub const SETTINGS_MONO: Background = Background {
    radials: &[
        Radial { rx: 0.50, ry: 0.35, cx: 0.18, cy: 0.08, color: white(0.14), stop: 0.70 },
        Radial { rx: 0.45, ry: 0.40, cx: 0.88, cy: 0.96, color: white(0.07), stop: 0.70 },
    ],
    linear: GLASS,
};
// src/windows/Changelog.css:58-61 .cl-bg.cl-bg-mono
pub const CHANGELOG_MONO: Background = Background {
    radials: &[
        Radial { rx: 0.55, ry: 0.40, cx: 0.20, cy: 0.10, color: white(0.14), stop: 0.70 },
        Radial { rx: 0.50, ry: 0.45, cx: 0.85, cy: 0.95, color: white(0.07), stop: 0.70 },
    ],
    linear: GLASS,
};

// src/App.css:141 .widget-bg: inset -60px (kept by .widget-bg-mono); src/windows/Settings.css:41
// .st-bg-mono and src/windows/Changelog.css:55 .cl-bg-mono: inset 0
pub const WIDGET_MONO_INSET: f32 = 60.0;

/// Premultiplied colour of `bg` at (x, y) in a w × h box (CSS px).
fn eval(bg: &Background, w: f32, h: f32, x: f32, y: f32) -> [f32; 4] {
    let l = bg.linear;
    let th = l.angle.to_radians();
    let (dx, dy) = (th.sin(), -th.cos());
    let len = (w * dx).abs() + (h * dy).abs();
    let t = (((x - w / 2.0) * dx + (y - h / 2.0) * dy) / len + 0.5).clamp(0.0, 1.0);
    let pm = |c: Rgba| [c.r * c.a, c.g * c.a, c.b * c.a, c.a];
    let (a, b) = (pm(l.from), pm(l.to));
    let mut acc = [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t);
    for r in bg.radials.iter().rev() {
        let t = (((x - r.cx * w) / (r.rx * w)).powi(2) + ((y - r.cy * h) / (r.ry * h)).powi(2)).sqrt();
        if t >= r.stop {
            continue;
        }
        let src = pm(r.color).map(|v| v * (1.0 - t / r.stop));
        acc = [0, 1, 2, 3].map(|k| src[k] + acc[k] * (1.0 - src[3]));
    }
    acc
}

/// The background under a card of `cw` × `ch` device px (scale `sf`), the element reaching `inset`
/// CSS px past every side: straight-alpha BGRA, one pixel per device pixel.
fn bake(bg: &Background, cw: u32, ch: u32, sf: f32, inset: f32) -> Vec<u8> {
    let (ew, eh) = (cw as f32 / sf + 2.0 * inset, ch as f32 / sf + 2.0 * inset);
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for j in 0..ch {
        let y = (j as f32 + 0.5) / sf + inset;
        for i in 0..cw {
            let x = (i as f32 + 0.5) / sf + inset;
            let p = eval(bg, ew, eh, x, y);
            let a = p[3];
            let un = |v: f32| if a > 0.0 { (v / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            out.extend_from_slice(&[un(p[2]), un(p[1]), un(p[0]), (a * 255.0).round().clamp(0.0, 255.0) as u8]);
        }
    }
    out
}

thread_local! {
    static CACHE: RefCell<HashMap<(usize, [u32; 4]), Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

fn baked(bg: &'static Background, cw: u32, ch: u32, sf: f32, inset: f32) -> Arc<RenderImage> {
    let key = (bg as *const Background as usize, [cw, ch, sf.to_bits(), inset.to_bits()]);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.entry(key)
            .or_insert_with(|| {
                let img = image::RgbaImage::from_raw(cw, ch, bake(bg, cw, ch, sf, inset)).expect("bake size");
                Arc::new(RenderImage::new(vec![image::Frame::new(img)]))
            })
            .clone()
    })
}

/// The mono background of a card: the card's first child (absolute, inset 0), drawn with the card's
/// `radii` at `opacity`. `inset` is the element's CSS `inset` (positive = past the card).
pub fn background(bg: &'static Background, inset: f32, radii: Radii, opacity: f32) -> impl IntoElement {
    div().absolute().top_0().left_0().size_full().opacity(opacity).child(
        canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let sf = window.scale_factor();
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let (cw, ch) = ((w * sf).round().max(1.0), (h * sf).round().max(1.0));
            let img = baked(bg, cw as u32, ch as u32, sf, inset);
            let at = Bounds { origin: bounds.origin, size: size(px(cw / sf), px(ch / sf)) };
            let _ = window.paint_image(at, corners(clamp(radii, w, h)), img, 0, false);
        })
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_ends_and_glow() {
        // a 160deg gradient starts at the top-left-ish corner's projection and ends at the opposite one
        let bg = Background { radials: &[], linear: GLASS };
        let (w, h) = (600.0f32, 200.0f32);
        let th = 160.0f32.to_radians();
        let len = (w * th.sin()).abs() + (h * th.cos()).abs();
        let (dx, dy) = (th.sin(), -th.cos());
        let start = (w / 2.0 - dx * len / 2.0, h / 2.0 - dy * len / 2.0);
        let c = eval(&bg, w, h, start.0, start.1);
        assert!((c[0] - 0x1d as f32 / 255.0).abs() < 1e-4 && c[3] == 1.0);
        // the glow is white at its alpha over the base at its centre, gone at 70%
        const GLOW: [Radial; 1] = [Radial { rx: 0.5, ry: 0.5, cx: 0.5, cy: 0.5, color: white(0.14), stop: 0.7 }];
        let g = Background { radials: &GLOW, linear: GLASS };
        let base = eval(&bg, w, h, 300.0, 100.0);
        let lit = eval(&g, w, h, 300.0, 100.0);
        assert!((lit[0] - (0.14 + base[0] * 0.86)).abs() < 1e-5);
        assert_eq!(eval(&g, w, h, 300.0 + 0.7 * 300.0 + 1.0, 100.0), eval(&bg, w, h, 300.0 + 0.7 * 300.0 + 1.0, 100.0));
    }
}
