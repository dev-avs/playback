//! The lyric sweep (port.md 2.4): `background-image: linear-gradient(<angle>, lit <gp>, dim <gp + soft>)`
//! with `background-clip: text` and a transparent fill (src/App.css:1415-1428 `.word, .letter, .dot`,
//! src/windows/Settings.css:720-738 `.st-pv-line.current`).
//!
//! What Chromium (Skia, software raster) draws, measured (parity/plan.md Phase 4, "Slice C – glow and
//! sweep"): the gradient is painted over the element's border box and kept where the text is, through
//! the text drawn as a mask in black (Blink's text-clip phase). That coverage is the black-text one:
//! a white gradient clipped to `Wild ill` has exactly `color: #000`'s alpha (0/255 apart, mass 75393),
//! where `color: #fff` has 92874; text gamma/contrast depends on the colour, so the lit and dim parts
//! cannot be GPUI glyphs in their own colours (a white glyph is up to 100/255 heavier at its edges).
//! GPUI's per-run HighlightStyle cannot cut a glyph either (gotcha 7.1), and the sweep's edge crosses
//! glyphs on every frame. So the sweep is baked on the CPU, per pixel:
//! - coverage: each glyph's DirectWrite mask (`text_shadow::glyph_masks`, made as gpui makes them)
//!   through gpui's own glyph shader for black text (gpui-0.2.2 alpha_correction.hlsl, shaders.hlsl:1116-1120:
//!   `enhance_contrast` and `apply_alpha_correction` at brightness 0, with the system's gamma and
//!   grayscale contrast as gpui reads them), glyphs combined src-over, as the mask layer does;
//! - × the box: the gradient is painted over the border box only (a glyph's part below it stays
//!   unpainted, src/App.css:495-500), with the box's area coverage at its fractional edges;
//! - × the gradient at the pixel centre: CSS Images 3 §3.1 (direction (sin θ, −cos θ), length
//!   |w sin θ| + |h cos θ| through the box centre, positioning area = the padding box), stops in % of
//!   that length, interpolated premultiplied (as `radial`).
//! The text-shadow layers (`text_shadow`) are painted over it, as Blink paints the text (and its
//! shadows) after the background.

use super::text::{shape, Shaped, Style};
use super::text_shadow::{self, glyph_masks, masks_key, paint_baked, paint_shadows, with_dw, TextShadow};
use gpui::{
    px, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Rgba,
    SharedString, Window,
};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// `linear-gradient(<angle>deg, <lit> <lit_at>%, <dim> <dim_at>%)` over a unit's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub angle: f32,
    pub lit: Rgba,
    pub lit_at: f32,
    pub dim: Rgba,
    pub dim_at: f32,
}

/// src/App.css:1419-1423: lit `hsl(--lit-h --lit-s --lit-l / --grad-lit)` at `--gp`, dim
/// `rgba(255, 255, 255, --grad-dim)` at `--gp + --sweep-soft`.
pub fn lyric(angle: f32, lit: Rgba, gp: f32, soft: f32, dim_alpha: f32) -> Sweep {
    Sweep { angle, lit, lit_at: gp, dim: Rgba { r: 1.0, g: 1.0, b: 1.0, a: dim_alpha }, dim_at: gp + soft }
}

/// gpui's monochrome sprite alpha for a black glyph (alpha_correction.hlsl `apply_contrast_and_gamma_correction`
/// with color (0, 0, 0): brightness 0, `light_on_dark_contrast` = contrast × saturate(4 × .75)).
fn black_coverage(sample: f32, g: [f32; 4], contrast: f32) -> f32 {
    let k = contrast;
    let c = sample * (k + 1.0) / (sample * k + 1.0);
    // `(g.x × b + g.y) × c + (g.z × b + g.w)` at b = 0
    let correction = g[1] * c + g[3];
    (c + c * (1.0 - c) * correction).clamp(0.0, 1.0)
}

/// The gradient's premultiplied colour at `t` (fraction of the gradient line).
fn color_at(s: &Sweep, t: f32) -> [f32; 4] {
    let pm = |c: Rgba| [c.r * c.a, c.g * c.a, c.b * c.a, c.a];
    let (p1, p2) = (s.lit_at / 100.0, (s.dim_at / 100.0).max(s.lit_at / 100.0));
    if t <= p1 {
        return pm(s.lit);
    }
    if t >= p2 {
        return pm(s.dim);
    }
    let f = (t - p1) / (p2 - p1);
    let (a, b) = (pm(s.lit), pm(s.dim));
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * f)
}

/// Bakes the clipped gradient: box `b` in device px; straight BGRA over the masks' bounds.
fn bake(masks: &[text_shadow::Mask], b: Bounds<f32>, s: &Sweep, g: [f32; 4], contrast: f32) -> Option<((i32, i32, usize, usize), Vec<u8>)> {
    let (x0, y0, w, h) = text_shadow::bounds_of(masks)?;
    let mut cov = vec![0.0f32; w * h];
    for m in masks {
        for j in 0..m.h {
            let row = (m.y - y0) as usize + j;
            for i in 0..m.w {
                let c = black_coverage(m.a[j * m.w + i] as f32 / 255.0, g, contrast);
                let d = &mut cov[row * w + (m.x - x0) as usize + i];
                *d = c + *d * (1.0 - c);
            }
        }
    }
    let th = s.angle.to_radians();
    let (dx, dy) = (th.sin(), -th.cos());
    let len = (b.size.width * dx).abs() + (b.size.height * dy).abs();
    let (cx, cy) = (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);
    let (bx1, by1) = (b.origin.x + b.size.width, b.origin.y + b.size.height);
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let mut out = Vec::with_capacity(w * h * 4);
    for j in 0..h {
        let py = (y0 + j as i32) as f32;
        let oy = ((py + 1.0).min(by1) - py.max(b.origin.y)).clamp(0.0, 1.0);
        for i in 0..w {
            let px_ = (x0 + i as i32) as f32;
            let ox = ((px_ + 1.0).min(bx1) - px_.max(b.origin.x)).clamp(0.0, 1.0);
            let t = ((px_ + 0.5 - cx) * dx + (py + 0.5 - cy) * dy) / len + 0.5;
            let c = color_at(s, t);
            let a = c[3] * cov[j * w + i] * ox * oy;
            let un = |v: f32| if c[3] > 0.0 { ch(v / c[3]) } else { 0 };
            out.extend_from_slice(&[un(c[2]), un(c[1]), un(c[0]), ch(a)]);
        }
    }
    Some(((x0, y0, w, h), out))
}

thread_local! {
    static CACHE: RefCell<HashMap<Vec<u32>, Option<((i32, i32, usize, usize), Arc<RenderImage>)>>> = RefCell::new(HashMap::new());
}

/// Paints the sweep of `sh` (line box top-left at the box's origin, `line_box` high) over the unit's
/// border box `bounds` (window px).
pub fn paint(sh: &Shaped, bounds: Bounds<Pixels>, line_box: f32, s: &Sweep, window: &mut Window) {
    let sf = window.scale_factor();
    let masks = glyph_masks(sh, bounds.origin, line_box, 1.0, window);
    let b = Bounds {
        origin: gpui::point(f32::from(bounds.origin.x) * sf, f32::from(bounds.origin.y) * sf),
        size: gpui::size(f32::from(bounds.size.width) * sf, f32::from(bounds.size.height) * sf),
    };
    let (ox, oy) = masks.first().map(|m| (m.x, m.y)).unwrap_or((0, 0));
    let extra: Vec<u32> = [b.origin.x - ox as f32, b.origin.y - oy as f32, b.size.width, b.size.height, s.angle, s.lit.r, s.lit.g, s.lit.b, s.lit.a, s.lit_at, s.dim.r, s.dim.g, s.dim.b, s.dim.a, s.dim_at]
        .iter()
        .map(|v| v.to_bits())
        .collect();
    let (k, ox, oy) = masks_key(&masks, &extra);
    let (g, contrast) = with_dw(|dw| (dw.gamma_ratios, dw.contrast));
    let baked = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 512 {
            c.clear();
        }
        c.entry(k)
            .or_insert_with(|| {
                bake(&masks, b, s, g, contrast).map(|(r, bgra)| {
                    let img = image::RgbaImage::from_raw(r.2 as u32, r.3 as u32, bgra).expect("bake size");
                    ((r.0 - ox, r.1 - oy, r.2, r.3), Arc::new(RenderImage::new(vec![image::Frame::new(img)])))
                })
            })
            .clone()
    });
    if let Some((r, img)) = baked {
        paint_baked((r.0 + ox, r.1 + oy, r.2, r.3), img, window);
    }
}

/// One swept unit (`white-space: nowrap`, transparent fill): its box is the text's line box plus
/// `pad_bottom` px of padding below (src/App.css:501-505, .18em on word units), the sweep painted in it
/// and the `text-shadow` layers over the sweep.
pub fn swept(text: impl Into<SharedString>, style: Style, sweep: Sweep, pad_bottom: f32, shadows: &[TextShadow]) -> Swept {
    Swept { text: text.into(), style, sweep, pad_bottom, shadows: shadows.to_vec(), shaped: None }
}

pub struct Swept {
    text: SharedString,
    style: Style,
    sweep: Sweep,
    pad_bottom: f32,
    shadows: Vec<TextShadow>,
    shaped: Option<Shaped>,
}

impl IntoElement for Swept {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Swept {
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
        st.size.height = px(shaped.line_box(self.style.line_height) + self.pad_bottom).into();
        st.flex_shrink = 0.0;
        self.shaped = Some(shaped);
        (window.request_layout(st, [], cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut Window, _: &mut App) {}
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, _: &mut App) {
        if let Some(sh) = &self.shaped {
            let lb = sh.line_box(self.style.line_height);
            paint(sh, bounds, lb, &self.sweep, window);
            paint_shadows(sh, bounds.origin, lb, &self.shadows, window);
        }
    }
}
