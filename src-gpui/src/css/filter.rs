//! `filter: blur() saturate() brightness()` on the blurred cover behind a card (port.md 2.4, gotcha
//! 7.14): `.widget-bg` (src/App.css:139-148), `.st-bg` (src/windows/Settings.css:19-27) and
//! `.st-preview-bg` (src/windows/Settings.css:517-524). GPUI has no filters, so the layer is baked on the
//! CPU once per (cover, filter, card size) and painted, with the card's background, as one image.
//!
//! What Chromium draws (software raster, measured with the primitive pages): the element (the card
//! grown by `inset` on every side) filled with the cover (`background-size: cover`, centred), then the
//! filters in their CSS order:
//! - blur: Skia's three box blurs (`shadow::blur`), sigma = the CSS length, on premultiplied colour,
//!   transparent outside the element (the card's edges are within reach of it: a 50px blur 60px out
//!   leaves the card's edge ~0.88 opaque). Measured on a 100px white square: the three-box filter
//!   matches Chromium within 1/255 at sigma 10, 15, 25 and 50; a true gaussian is off by up to 9/255;
//! - saturate(s): the W3C matrix (Filter Effects 1 §12.2, rows 0.213 / 0.715 / 0.072) on unpremultiplied
//!   colour, clamped to [0, 1];
//! - brightness(b): × b, clamped (a separate Skia colour matrix, so the clamp between them stays).
//!
//! The bake: the cover resized to 192² (`BAKE`), the element on that scale (192 / its longer side), the
//! blur's sigma scaled with it; the part under the card is then resampled (bilinear, premultiplied) to
//! the card's device pixels, so GPUI draws it 1:1. A blur too small for Skia's three-box kernel at that
//! scale (sigma < 2: `blur(0)`, "Cover blur" 0, src/windows/Settings.tsx:1030) is baked at device scale:
//! that is Chromium's own path (it never downscales; it draws the cover at device px and blurs there),
//! the 192² grid being 7.14's shortcut where the blur hides it.
//! The card's own background is baked into the same image, with the card's rounded clip (`overflow:
//! hidden` + `border-radius`) as Chromium rasterizes it (`rounded::clip_mask`), because GPUI's rounded
//! AA follows the circle and Chromium's the chords of a flattened path: drawn as GPUI's card fill + a
//! rounded image, the 10px preview corners were up to 72/255 off. As in Chromium (measured on the
//! preview page, card fill alone against fill + layer: within 6/255; one coverage for both is up to
//! 20/255 off), the clip's coverage c applies to each of the two draws: the fill at c, then the layer
//! at `opacity` × c over it.

use super::rounded::{self, Radii};
use super::shadow;
use gpui::{canvas, prelude::*, px, size, Bounds, Pixels, RenderImage, Rgba, Window};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// port.md gotcha 7.14: the cover is baked at 192 × 192.
const BAKE: f32 = 192.0;

/// One cover image, straight-alpha RGBA (convert SMTC / `RenderImage` BGRA once, gotcha 7.13). `key`
/// names it in the bake cache (one per track).
#[derive(Clone)]
pub struct Cover {
    pub key: u64,
    pub image: Arc<image::RgbaImage>,
}

/// `filter: blur(<blur>px) saturate(<saturate>) brightness(<brightness>)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub blur: f32,
    pub saturate: f32,
    pub brightness: f32,
}

// src/App.css:144 .widget-bg: blur(var(--cover-blur, 50px)) saturate(var(--cover-sat, 1.4))
// brightness(var(--cover-bright, 1)); the variables from src/App.tsx:652-657 (coverBlur px,
// coverSaturation / 100, coverBrightness / 100; defaults src/settings.ts:125, 190, 192)
pub const fn widget(cover_blur: f32, cover_saturation: f32, cover_brightness: f32) -> Filter {
    Filter { blur: cover_blur, saturate: cover_saturation / 100.0, brightness: cover_brightness / 100.0 }
}
pub const WIDGET: Filter = widget(50.0, 140.0, 100.0);
// src/windows/Settings.css:24 .st-bg: blur(50px) saturate(1.4)
pub const SETTINGS: Filter = Filter { blur: 50.0, saturate: 1.4, brightness: 1.0 };
// src/windows/Settings.css:522 .st-preview-bg: var(--pv-cover-filter, ...), set by src/windows/Settings.tsx:366
// to blur(coverBlur × 0.6) saturate(coverSaturation / 100) brightness(coverBrightness / 100)
pub const fn preview(cover_blur: f32, cover_saturation: f32, cover_brightness: f32) -> Filter {
    Filter { blur: cover_blur * 6.0 / 10.0, saturate: cover_saturation / 100.0, brightness: cover_brightness / 100.0 }
}
pub const PREVIEW: Filter = preview(50.0, 140.0, 100.0);

// src/App.css:141 .widget-bg and src/windows/Settings.css:21 .st-bg: inset -60px;
// src/windows/Settings.css:519 .st-preview-bg: inset -40px
pub const WIDGET_INSET: f32 = 60.0;
pub const SETTINGS_INSET: f32 = 60.0;
pub const PREVIEW_INSET: f32 = 40.0;

/// Bilinear sample of a w × h grid of `N`-channel pixels at (x, y) in pixel-centre coordinates
/// (pixel i's centre is i), clamped to the edge.
fn bilinear<const N: usize>(g: &[[f32; N]], w: usize, h: usize, x: f32, y: f32) -> [f32; N] {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; N];
    for c in 0..N {
        let top = g[y0 * w + x0][c] * (1.0 - fx) + g[y0 * w + x1][c] * fx;
        let bot = g[y1 * w + x0][c] * (1.0 - fx) + g[y1 * w + x1][c] * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    out
}

/// Filter Effects 1 §12.2 saturate, then brightness, each clamped, on unpremultiplied 0..1 colour.
fn color_filter(f: Filter, [r, g, b]: [f32; 3]) -> [f32; 3] {
    let s = f.saturate;
    let sat = [
        (0.213 + 0.787 * s) * r + (0.715 - 0.715 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 + 0.285 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 - 0.715 * s) * g + (0.072 + 0.928 * s) * b,
    ]
    .map(|v| v.clamp(0.0, 1.0));
    sat.map(|v| (v * f.brightness).clamp(0.0, 1.0))
}

/// A card of `cw` × `ch` device px with radii `radii` (device px) and background `bg`, the filtered
/// layer at `opacity` over it, the element reaching `inset` device px past every side: straight-alpha
/// BGRA, `ceil(cw)` × `ceil(ch)`, clipped to the card's rounded box.
#[allow(clippy::too_many_arguments)]
fn bake(cover: &image::RgbaImage, f: Filter, cw: f32, ch: f32, inset: f32, blur: f32, radii: Radii, opacity: f32, bg: Rgba) -> (u32, u32, Vec<u8>) {
    let (ew, eh) = (cw + 2.0 * inset, ch + 2.0 * inset);
    let span = ew.max(eh);
    let mut s = BAKE / span;
    if blur * s < 2.0 {
        s = 1.0;
    }
    // the cover square (background-size: cover, background-position: center) at the bake scale
    let q = (span * s).round().max(1.0) as u32;
    let sq = image::imageops::resize(cover, q, q, image::imageops::FilterType::Triangle);
    let sq: Vec<[f32; 4]> = sq.pixels().map(|p| p.0.map(|v| v as f32 / 255.0)).collect();
    let (ox, oy) = ((ew - span) / 2.0 * s, (eh - span) / 2.0 * s);
    // the element on the bake grid, premultiplied 0..255, area coverage on its far edges
    let (gw, gh) = ((ew * s).ceil() as usize, (eh * s).ceil() as usize);
    let (xe, ye) = (ew * s, eh * s);
    let mut chan = vec![vec![0.0f32; gw * gh]; 4];
    for j in 0..gh {
        let cy = ((j + 1) as f32).min(ye) - j as f32;
        for i in 0..gw {
            let cov = (((i + 1) as f32).min(xe) - i as f32) * cy;
            let [r, g, b, a] = bilinear(&sq, q as usize, q as usize, i as f32 - ox, j as f32 - oy);
            let a = a * cov;
            for (c, v) in [r * a, g * a, b * a, a].into_iter().enumerate() {
                chan[c][j * gw + i] = (v * 255.0).round();
            }
        }
    }
    if blur > 0.0 {
        for c in chan.iter_mut() {
            shadow::blur(c, gw, gh, blur * s);
        }
    }
    let grid: Vec<[f32; 4]> = (0..gw * gh)
        .map(|k| {
            let a = chan[3][k] / 255.0;
            if a <= 0.0 {
                return [0.0; 4];
            }
            let rgb = color_filter(f, [0, 1, 2].map(|c| (chan[c][k] / 255.0 / a).min(1.0)));
            [rgb[0] * a, rgb[1] * a, rgb[2] * a, a]
        })
        .collect();
    // the part under the card, at its device pixels: the layer at `opacity` over the card's background
    // (src-over, premultiplied), then the card's clip
    let (ow, oh) = (cw.ceil() as u32, ch.ceil() as u32);
    let b = [bg.r * bg.a, bg.g * bg.a, bg.b * bg.a, bg.a];
    let mask = rounded::clip_mask(cw, ch, radii);
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for y in 0..oh {
        for x in 0..ow {
            let g = bilinear(&grid, gw, gh, (x as f32 + 0.5 + inset) * s - 0.5, (y as f32 + 0.5 + inset) * s - 0.5);
            let c = mask[(y * ow + x) as usize];
            let p = [0, 1, 2, 3].map(|k| g[k] * opacity * c + b[k] * c * (1.0 - g[3] * opacity * c));
            let a = p[3];
            let un = |v: f32| if a > 0.0 { (v / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            out.extend_from_slice(&[un(p[2]), un(p[1]), un(p[0]), (a * 255.0).round().clamp(0.0, 255.0) as u8]);
        }
    }
    (ow, oh, out)
}

thread_local! {
    static CACHE: RefCell<HashMap<Vec<u64>, Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

#[allow(clippy::too_many_arguments)]
fn baked(cover: &Cover, f: Filter, cw: f32, ch: f32, inset: f32, blur: f32, radii: Radii, opacity: f32, bg: Rgba) -> Arc<RenderImage> {
    let key = [f.blur, f.saturate, f.brightness, cw, ch, inset, blur, radii[0], radii[1], radii[2], radii[3], opacity, bg.r, bg.g, bg.b, bg.a]
        .map(|v| v.to_bits() as u64);
    let key: Vec<u64> = std::iter::once(cover.key).chain(key).collect();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.entry(key)
            .or_insert_with(|| {
                let (w, h, bgra) = bake(&cover.image, f, cw, ch, inset, blur, radii, opacity, bg);
                let img = image::RgbaImage::from_raw(w, h, bgra).expect("bake size");
                Arc::new(RenderImage::new(vec![image::Frame::new(img)]))
            })
            .clone()
    })
}

/// A card's background `bg` with its cover layer (the card's first child, absolute, inset 0: the padding
/// box of a card without borders) at the element's `opacity`, clipped to the card's `radii`: the card's
/// first child, in place of the card's own background (the card itself paints none). `inset` is the CSS
/// `inset` (positive = past the card, `WIDGET_INSET` etc.).
pub fn cover(cover: Cover, f: Filter, inset: f32, radii: Radii, opacity: f32, bg: Rgba) -> impl IntoElement {
    canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
        let sf = window.scale_factor();
        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let r = rounded::clamp(radii, w, h).map(|v| v * sf);
        let img = baked(&cover, f, w * sf, h * sf, inset * sf, f.blur * sf, r, opacity, bg);
        let at = Bounds { origin: bounds.origin, size: size(px(img.size(0).width.0 as f32 / sf), px(img.size(0).height.0 as f32 / sf)) };
        let _ = window.paint_image(at, Default::default(), img, 0, false);
    })
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturate_matrix() {
        // saturate(1) and brightness(1) are the identity; saturate(0) is the luminance grey
        let id = Filter { blur: 0.0, saturate: 1.0, brightness: 1.0 };
        let c = color_filter(id, [0.2, 0.5, 0.9]);
        assert!(c.iter().zip([0.2, 0.5, 0.9]).all(|(a, b)| (a - b).abs() < 1e-6));
        let grey = color_filter(Filter { saturate: 0.0, ..id }, [1.0, 0.0, 0.0]);
        assert!(grey.iter().all(|v| (v - 0.213).abs() < 1e-6));
        // the clamp after saturate stays before brightness
        let s = color_filter(Filter { saturate: 2.5, brightness: 0.5, ..id }, [1.0, 0.0, 0.0]);
        assert_eq!(s[0], 0.5);
    }

    #[test]
    fn defaults() {
        assert_eq!(WIDGET, Filter { blur: 50.0, saturate: 1.4, brightness: 1.0 });
        assert_eq!(PREVIEW, Filter { blur: 30.0, saturate: 1.4, brightness: 1.0 });
    }
}
