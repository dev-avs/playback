//! CSS `text-shadow` (port.md 2.4), as Chromium's software raster (Skia) draws it, measured on
//! Electron 43 with `Wild ill` / `WW` 17px SpicyLyrics Bold, one glyph per page as well as whole runs
//! (parity/plan.md Phase 4, "Slice C – glow and sweep"):
//! - a shadow with a blur is the text drawn through a blur mask filter (sigma = blur / 2). The filter
//!   works on each glyph's own coverage mask, before Skia's text gamma/contrast (the mask filter turns
//!   the preblend off), so a shadow is lighter than the fill: `text-shadow: 0 0 .5px #fff` (sigma .25,
//!   Skia's "no blur" case, which keeps the mask) has 77018 of coverage where `color: #fff` has 92874;
//! - each glyph is blurred on its own and drawn src-over in the shadow colour (max 2.8/255 off, against
//!   26/255 for one blur of the whole run's coverage: overlapping halos add up, they do not union);
//! - the blur is `SkMaskBlurFilter`'s: sigma ≥ 2 its three box blurs (`shadow::blur`), sigma < 2 the
//!   small-sigma kernel ported below (`small_blur`), sigma < 1/3 none (`hasNoBlur`). One glyph against
//!   one blur of its own mask: 0/255 (blur 1, 2.2, 3px) and 1/255 (4, 8, 14px);
//! - a shadow without a blur is the text drawn normally in the shadow colour (no mask filter);
//! - layers: the last listed at the bottom, then the text on top (also over a `background-clip: text`
//!   fill, which is the element's background: `sweep`).
//! GPUI's glyph rasterizer is crate-private (gpui-0.2.2 text_system.rs:309), so the glyph masks come
//! from DirectWrite here, made exactly as gpui makes them (direct_write.rs:700-799: the same font
//! faces, rendering mode, grid fit, grayscale antialiasing and quarter-pixel subpixel variants), at the
//! positions `Shaped::paint` puts the glyphs. A shadow is baked on the CPU into one image and painted
//! as a sprite, cached per (glyphs, sub-pixel positions, shadow).

use super::shadow;
use super::text::{shape, Shaped, Style};
use gpui::{
    point, px, size, App, Bounds, Element, ElementId, FontStyle, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, RenderImage, Rgba, SharedString, Window,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use windows::core::{Interface, BOOL, HSTRING};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::DirectWrite::*;

/// One `text-shadow` layer, in CSS px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub color: Rgba,
}

/// `<x> <y> <blur> <color>`
pub const fn text_shadow(x: f32, y: f32, blur: f32, color: Rgba) -> TextShadow {
    TextShadow { x, y, blur, color }
}

// ── glyph masks: DirectWrite, as gpui rasterizes ──

/// A glyph's coverage (0..255) at device pixels [x, x + w) × [y, y + h).
#[derive(Clone, Debug)]
pub(super) struct Mask {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub a: Vec<u8>,
}

pub(super) struct Dw {
    factory: IDWriteFactory5,
    /// The bundled faces (`text::unpacked`), then the system's: the collections gpui looks fonts up in.
    sets: [IDWriteFontSet; 2],
    faces: HashMap<usize, IDWriteFontFace3>,
    /// (font id, glyph id, size bits, scale bits, subpixel variant) → mask relative to the floored origin
    masks: HashMap<(usize, u32, u32, u32, u8), Option<Rc<Mask>>>,
    /// gpui's text gamma ratios and grayscale contrast (directx_renderer.rs:608-618), for `sweep`
    pub(super) gamma_ratios: [f32; 4],
    pub(super) contrast: f32,
}

impl Dw {
    fn new() -> windows::core::Result<Dw> {
        unsafe {
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loader = factory.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let builder = factory.CreateFontSetBuilder()?;
            for bytes in super::text::unpacked().map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e))? {
                let file = loader.CreateInMemoryFontFileReference(&factory, bytes.as_ptr() as _, bytes.len() as u32, None)?;
                builder.AddFontFile(&file)?;
            }
            let custom = builder.CreateFontSet()?;
            let mut system = None;
            factory.GetSystemFontCollection(false, &mut system, true)?;
            let system = system.expect("system font collection").GetFontSet()?;
            let params: IDWriteRenderingParams1 = factory.CreateRenderingParams()?.cast()?;
            Ok(Dw {
                factory,
                sets: [custom, system],
                faces: HashMap::new(),
                masks: HashMap::new(),
                gamma_ratios: gamma_ratios(params.GetGamma()),
                contrast: params.GetGrayscaleEnhancedContrast(),
            })
        }
    }

    /// The face gpui opened for `id` (direct_write.rs:395-440: the family's first match at the font's
    /// weight and style, bundled faces before the system's).
    fn face(&mut self, id: gpui::FontId, window: &Window) -> Option<IDWriteFontFace3> {
        if let Some(f) = self.faces.get(&id.0) {
            return Some(f.clone());
        }
        let font = window.text_system().get_font_for_id(id)?;
        let style = match font.style {
            FontStyle::Normal => DWRITE_FONT_STYLE_NORMAL,
            FontStyle::Italic => DWRITE_FONT_STYLE_ITALIC,
            FontStyle::Oblique => DWRITE_FONT_STYLE_OBLIQUE,
        };
        let family = HSTRING::from(font.family.as_ref());
        let face = self.sets.iter().find_map(|set| unsafe {
            let m = set.GetMatchingFonts(&family, DWRITE_FONT_WEIGHT(font.weight.0 as i32), DWRITE_FONT_STRETCH_NORMAL, style).ok()?;
            (m.GetFontCount() > 0).then(|| m.GetFontFaceReference(0).ok()?.CreateFontFace().ok()).flatten()
        })?;
        self.faces.insert(id.0, face.clone());
        Some(face)
    }

    /// direct_write.rs:700-799 `create_glyph_run_analysis`, `raster_bounds`, `rasterize_monochrome`.
    fn raster(face: &IDWriteFontFace3, factory: &IDWriteFactory5, glyph: u16, size: f32, sf: f32, sub_x: u8) -> Option<Mask> {
        unsafe {
            let ids = [glyph];
            let advance = [0.0f32];
            let offset = [DWRITE_GLYPH_OFFSET::default()];
            let run = DWRITE_GLYPH_RUN {
                fontFace: std::mem::transmute_copy(face),
                fontEmSize: size,
                glyphCount: 1,
                glyphIndices: ids.as_ptr(),
                glyphAdvances: advance.as_ptr(),
                glyphOffsets: offset.as_ptr(),
                isSideways: BOOL(0),
                bidiLevel: 0,
            };
            let transform = DWRITE_MATRIX { m11: sf, m12: 0.0, m21: 0.0, m22: sf, dx: 0.0, dy: 0.0 };
            // SUBPIXEL_VARIANTS_X = 4, SUBPIXEL_VARIANTS_Y = 1 on Windows (text_system.rs:44-51)
            let ox = sub_x as f32 / 4.0 / sf;
            let mut mode = DWRITE_RENDERING_MODE1::default();
            let mut grid = DWRITE_GRID_FIT_MODE::default();
            face.GetRecommendedRenderingMode(
                size,
                96.0,
                96.0,
                Some(&transform),
                false,
                DWRITE_OUTLINE_THRESHOLD_ANTIALIASED,
                DWRITE_MEASURING_MODE_NATURAL,
                None,
                &mut mode,
                &mut grid,
            )
            .ok()?;
            let mode = if mode == DWRITE_RENDERING_MODE1_OUTLINE { DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC } else { mode };
            let analysis = factory
                .CreateGlyphRunAnalysis(&run, Some(&transform), mode, DWRITE_MEASURING_MODE_NATURAL, grid, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE, ox, 0.0)
                .ok()?;
            let b = analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1).ok()?;
            if b.right <= b.left || b.bottom <= b.top {
                return None;
            }
            let (w, h) = ((b.right - b.left) as usize, (b.bottom - b.top) as usize);
            let mut a = vec![0u8; w * h];
            analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &b as *const RECT, &mut a).ok()?;
            Some(Mask { x: b.left, y: b.top, w, h, a })
        }
    }
}

thread_local! {
    static DW: RefCell<Option<Dw>> = const { RefCell::new(None) };
}

pub(super) fn with_dw<R>(f: impl FnOnce(&mut Dw) -> R) -> R {
    DW.with(|d| {
        let mut d = d.borrow_mut();
        let dw = d.get_or_insert_with(|| Dw::new().expect("css::text_shadow: DirectWrite"));
        f(dw)
    })
}

/// directx_renderer.rs:621-655 `get_gamma_ratios` (from Windows Terminal's dwrite.cpp).
fn gamma_ratios(gamma: f32) -> [f32; 4] {
    const RATIOS: [[f32; 4]; 13] = [
        [0.0000, 0.0000, 0.0000, 0.0000],
        [0.0166, -0.0807, 0.2227, -0.0751],
        [0.0350, -0.1760, 0.4325, -0.1370],
        [0.0543, -0.2821, 0.6302, -0.1876],
        [0.0739, -0.3963, 0.8167, -0.2287],
        [0.0933, -0.5161, 0.9926, -0.2616],
        [0.1121, -0.6395, 1.1588, -0.2877],
        [0.1300, -0.7649, 1.3159, -0.3080],
        [0.1469, -0.8911, 1.4644, -0.3234],
        [0.1627, -1.0170, 1.6051, -0.3347],
        [0.1773, -1.1420, 1.7385, -0.3426],
        [0.1908, -1.2652, 1.8650, -0.3476],
        [0.2031, -1.3864, 1.9851, -0.3501],
    ];
    const NORM13: f32 = ((0x10000 as f64) / (255.0 * 255.0) * 4.0) as f32;
    const NORM24: f32 = ((0x100 as f64) / (255.0) * 4.0) as f32;
    let r = RATIOS[((gamma * 10.0).round() as usize).clamp(10, 22) - 10];
    [r[0] / 4.0 * NORM13, r[1] / 4.0 * NORM24, r[2] / 4.0 * NORM13, r[3] / 4.0 * NORM24]
}

/// The masks of `sh`'s glyphs as `Shaped::paint(origin, line_box, s, ..)` places them, in device px.
pub(super) fn glyph_masks(sh: &Shaped, origin: Point<Pixels>, line_box: f32, s: f32, window: &Window) -> Vec<Mask> {
    let sf = window.scale_factor();
    let size = sh.size * s;
    // the same arithmetic as Shaped::paint (Skia's rounding), then Window::paint_glyph's floor and
    // subpixel variant (window.rs:2960-2987)
    let base = f32::from(origin.y) + sh.baseline(line_box) * s;
    let y = (base * sf + 0.5) / sf;
    let gy = (y * sf).floor() as i32;
    with_dw(|dw| {
        let mut out = Vec::new();
        for g in &sh.glyphs {
            let x = ((f32::from(origin.x) + g.x * s) * sf + 0.125) / sf;
            let gx = x * sf;
            let sub = (gx.fract() * 4.0).floor() as u8;
            // GlyphId is `#[repr(C)] struct GlyphId(pub(crate) u32)` (gpui-0.2.2 text_system.rs:757-760)
            let glyph: u32 = unsafe { std::mem::transmute(g.id) };
            let key = (g.font_id.0, glyph, size.to_bits(), sf.to_bits(), sub);
            let m = match dw.masks.get(&key) {
                Some(m) => m.clone(),
                None => {
                    let m = dw.face(g.font_id, window).and_then(|f| Dw::raster(&f, &dw.factory, glyph as u16, size, sf, sub)).map(Rc::new);
                    dw.masks.insert(key, m.clone());
                    m
                }
            };
            if let Some(m) = m {
                out.push(Mask { x: gx.floor() as i32 + m.x, y: gy + m.y, w: m.w, h: m.h, a: m.a.clone() });
            }
        }
        out
    })
}

// ── Skia's small-sigma blur (SkMaskBlurFilter.cpp, SkGaussFilter.cpp) ──

/// `SkGaussFilter`: the discrete Gaussian of Lindeberg (modified Bessel functions), taps while they
/// exceed 1%, normalized, as 16-bit fixed point (`small_blur`'s `gaussFactors`). At most 5 taps.
fn bessel_factors(sigma: f64) -> Vec<u32> {
    let var = sigma * sigma;
    // I0 and I1 by their series
    let series = |first: f64, step: &dyn Fn(f64) -> f64| {
        let (mut sum, mut term, mut k) = (first, first, 1.0f64);
        while term > 1e-12 {
            term *= step(k);
            sum += term;
            k += 1.0;
        }
        sum
    };
    let q = var * var / 4.0;
    let i0 = series(1.0, &|k| q / (k * k));
    let i1 = series(var / 2.0, &|k| q / (k * (k + 1.0)));
    let d = var.exp();
    let mut b = vec![i0, i1];
    let mut g = vec![i0 / d, i1 / d];
    let mut n = 1;
    while g[n] > 0.01 {
        let next = -(2.0 * n as f64 / var) * b[n] + b[n - 1];
        b.push(next);
        g.push(next / d);
        n += 1;
    }
    g.truncate(n);
    let mut sum = g[0];
    for v in &g[1..] {
        sum += 2.0 * v;
    }
    for v in g.iter_mut() {
        *v /= sum;
    }
    let tail: f64 = g[1..].iter().map(|v| 2.0 * v).sum();
    g[0] = 1.0 - tail;
    g.iter().map(|v| ((v * 65536.0).round() as i64 & 0xffff) as u32).collect()
}

/// `small_blur`: the y pass then the x pass, each output `(0x80 + Σ mulhi(s << 8, g)) >> 8`; the
/// result grows by the radius on every side. Returns (pixels, radius).
fn small_blur(src: &[u8], w: usize, h: usize, sigma: f64) -> (Vec<u8>, usize) {
    let g = bessel_factors(sigma);
    let r = g.len() - 1;
    let (bw, bh) = (w + 2 * r, h + 2 * r);
    let mul = |s: u8, gi: u32| ((s as u32) << 8) * gi >> 16;
    let mut tmp = vec![0u8; w * bh];
    for x in 0..w {
        for j in 0..bh {
            let mut acc = 0x80u32;
            for t in -(r as i32)..=r as i32 {
                let y = j as i32 - r as i32 + t;
                if y >= 0 && (y as usize) < h {
                    acc += mul(src[y as usize * w + x], g[t.unsigned_abs() as usize]);
                }
            }
            tmp[j * w + x] = (acc >> 8) as u8;
        }
    }
    let mut out = vec![0u8; bw * bh];
    for j in 0..bh {
        for i in 0..bw {
            let mut acc = 0x80u32;
            for t in -(r as i32)..=r as i32 {
                let x = i as i32 - r as i32 + t;
                if x >= 0 && (x as usize) < w {
                    acc += mul(tmp[j * w + x as usize], g[t.unsigned_abs() as usize]);
                }
            }
            out[j * bw + i] = (acc >> 8) as u8;
        }
    }
    (out, r)
}

/// A glyph mask through Skia's blur mask filter at `sigma` (device px).
fn blur_mask(m: &Mask, sigma: f32) -> Mask {
    // SkMaskBlurFilter::hasNoBlur: kNoWindowSigma 1/3 (the mask is kept as it is)
    if sigma < 1.0 / 3.0 {
        return m.clone();
    }
    if sigma < 2.0 {
        let (a, r) = small_blur(&m.a, m.w, m.h, sigma as f64);
        return Mask { x: m.x - r as i32, y: m.y - r as i32, w: m.w + 2 * r, h: m.h + 2 * r, a };
    }
    let r = (3 * (shadow::box_width(sigma) / 2 + 1)) as usize;
    let (bw, bh) = (m.w + 2 * r, m.h + 2 * r);
    let mut grid = vec![0.0f32; bw * bh];
    for j in 0..m.h {
        for i in 0..m.w {
            grid[(j + r) * bw + i + r] = m.a[j * m.w + i] as f32;
        }
    }
    shadow::blur(&mut grid, bw, bh, sigma);
    Mask { x: m.x - r as i32, y: m.y - r as i32, w: bw, h: bh, a: grid.iter().map(|v| v.clamp(0.0, 255.0) as u8).collect() }
}

/// The pixel box masks cover: (x, y, w, h).
pub(super) fn bounds_of(masks: &[Mask]) -> Option<(i32, i32, usize, usize)> {
    let x0 = masks.iter().map(|m| m.x).min()?;
    let y0 = masks.iter().map(|m| m.y).min()?;
    let x1 = masks.iter().map(|m| m.x + m.w as i32).max()?;
    let y1 = masks.iter().map(|m| m.y + m.h as i32).max()?;
    Some((x0, y0, (x1 - x0) as usize, (y1 - y0) as usize))
}

/// One blurred shadow layer: every glyph's blurred mask drawn src-over in `color`; straight BGRA.
fn bake(masks: &[Mask], sigma: f32, color: Rgba) -> Option<((i32, i32, usize, usize), Vec<u8>)> {
    let blurred: Vec<Mask> = masks.iter().map(|m| blur_mask(m, sigma)).collect();
    let (x0, y0, w, h) = bounds_of(&blurred)?;
    let mut acc = vec![0.0f32; w * h];
    for m in &blurred {
        for j in 0..m.h {
            let row = (m.y - y0) as usize + j;
            for i in 0..m.w {
                let s = color.a * m.a[j * m.w + i] as f32 / 255.0;
                let d = &mut acc[row * w + (m.x - x0) as usize + i];
                *d = s + *d * (1.0 - s);
            }
        }
    }
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let (r, g, b) = (ch(color.r), ch(color.g), ch(color.b));
    let mut out = Vec::with_capacity(w * h * 4);
    for a in acc {
        out.extend_from_slice(&[b, g, r, ch(a)]);
    }
    Some(((x0, y0, w, h), out))
}

thread_local! {
    static CACHE: RefCell<HashMap<Vec<u32>, Option<((i32, i32, usize, usize), Arc<RenderImage>)>>> = RefCell::new(HashMap::new());
}

/// Paints `img` (device px box `b`) 1:1.
pub(super) fn paint_baked(b: (i32, i32, usize, usize), img: Arc<RenderImage>, window: &mut Window) {
    let sf = window.scale_factor();
    let at = Bounds {
        origin: point(px(b.0 as f32 / sf), px(b.1 as f32 / sf)),
        size: size(px(b.2 as f32 / sf), px(b.3 as f32 / sf)),
    };
    let _ = window.paint_image(at, Default::default(), img, 0, false);
}

/// A cache key for masks placed relative to their first glyph, plus `extra`.
pub(super) fn masks_key(masks: &[Mask], extra: &[u32]) -> (Vec<u32>, i32, i32) {
    let (ox, oy) = masks.first().map(|m| (m.x, m.y)).unwrap_or((0, 0));
    let mut k = extra.to_vec();
    for m in masks {
        // a mask is fixed by its glyph raster: position, size and a cheap content hash
        let sum = m.a.iter().enumerate().fold(0u32, |h, (i, v)| h.wrapping_mul(31).wrapping_add(*v as u32 ^ i as u32));
        k.extend([(m.x - ox) as u32, (m.y - oy) as u32, m.w as u32, m.h as u32, sum]);
    }
    (k, ox, oy)
}

/// Paints `shadows` of `sh` (line box top-left at `origin`, window px), the last listed at the bottom.
pub fn paint_shadows(sh: &Shaped, origin: Point<Pixels>, line_box: f32, shadows: &[TextShadow], window: &mut Window) {
    let sf = window.scale_factor();
    for s in shadows.iter().rev() {
        if s.color.a <= 0.0 {
            continue;
        }
        let at = point(origin.x + px(s.x), origin.y + px(s.y));
        if s.blur <= 0.0 {
            sh.paint(at, line_box, 1.0, s.color.into(), window);
            continue;
        }
        let sigma = s.blur * sf / 2.0;
        let masks = glyph_masks(sh, at, line_box, 1.0, window);
        let (k, ox, oy) = masks_key(&masks, &[1, sigma.to_bits(), s.color.r.to_bits(), s.color.g.to_bits(), s.color.b.to_bits(), s.color.a.to_bits()]);
        let baked = CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 512 {
                c.clear();
            }
            c.entry(k)
                .or_insert_with(|| {
                    bake(&masks, sigma, s.color).map(|(b, bgra)| {
                        let img = image::RgbaImage::from_raw(b.2 as u32, b.3 as u32, bgra).expect("bake size");
                        ((b.0 - ox, b.1 - oy, b.2, b.3), Arc::new(RenderImage::new(vec![image::Frame::new(img)])))
                    })
                })
                .clone()
        });
        if let Some((b, img)) = baked {
            paint_baked((b.0 + ox, b.1 + oy, b.2, b.3), img, window);
        }
    }
}

/// One line of text (`white-space: nowrap`) with `text-shadow` layers: laid out as `text::text`, the
/// shadows painted under the text.
pub fn shadowed_text(text: impl Into<SharedString>, style: Style, shadows: &[TextShadow]) -> ShadowedText {
    ShadowedText { text: text.into(), style, shadows: shadows.to_vec(), shaped: None }
}

pub struct ShadowedText {
    text: SharedString,
    style: Style,
    shadows: Vec<TextShadow>,
    shaped: Option<Shaped>,
}

impl IntoElement for ShadowedText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ShadowedText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let shaped = shape(&self.text, &self.style, window);
        let mut st = gpui::Style::default();
        st.size.width = px(shaped.box_width()).into();
        st.size.height = px(shaped.line_box(self.style.line_height)).into();
        st.flex_shrink = 0.0;
        self.shaped = Some(shaped);
        (window.request_layout(st, [], cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut Window, _: &mut App) {}
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, _: &mut App) {
        if let Some(sh) = &self.shaped {
            let lb = sh.line_box(self.style.line_height);
            paint_shadows(sh, bounds.origin, lb, &self.shadows, window);
            if self.style.color.a > 0.0 {
                sh.paint(bounds.origin, lb, 1.0, self.style.color, window);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skia_bessel_kernel() {
        // blur .09em of 17px (src/App.css:1428 --gb .09): sigma .765, three taps (radius 2)
        let g = bessel_factors(0.765);
        assert_eq!(g, vec![39899, 11202, 1616]);
        // blur 3px (src/App.css:1456): sigma 1.5, four taps (radius 3: the next factor is under 1%)
        assert_eq!(bessel_factors(1.5), vec![19276, 14165, 6685, 2280]);
        // sigma just under 2: five taps (radius 4, small_blur's largest)
        assert_eq!(bessel_factors(1.99).len(), 5);
    }

    #[test]
    fn small_blur_keeps_mass() {
        let mut src = vec![0u8; 9 * 9];
        src[4 * 9 + 4] = 255;
        let (out, r) = small_blur(&src, 9, 9, 0.765);
        assert_eq!(r, 2);
        let sum: u32 = out.iter().map(|v| *v as u32).sum();
        assert!((250..=260).contains(&sum), "{sum}");
    }
}
