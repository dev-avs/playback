//! Text as Chromium lays it out and paints it (port.md 2.4, gotcha 7.12): the app's font stacks with
//! their `@font-face` faces, per-character font fallback through the stack, line boxes with Blink's
//! ascent/descent rounding and half-leading, `letter-spacing`, and glyphs placed at Skia's rounded
//! subpixel positions. The text-shadow and lyric-sweep helpers paint through `Shaped` too.
//!
//! What Chromium (Electron 43, Windows, 1 px per DIP) does, measured on scripts/primitives/pages/text-*.html
//! (`CSS.getPlatformFontsForNode`, per-character `Range` rects):
//! - advances are DirectWrite's natural (design-unit) advances plus GPOS kerning, unrounded: subpixel
//!   positioning is on, so a run's width is the float sum, stored as a LayoutUnit rounded up
//!   (`LayoutUnit::FromFloatCeil`). GPUI differed twice, which made the reset pill 101px wide against
//!   100 (scale-pill-text): it sets an `IDWriteTypography` on every range (gpui-0.2.2
//!   src/platform/windows/direct_write.rs:602, :642) that is empty for default features
//!   (`apply_font_features` returns early, :1730-1732), and an empty one leaves DirectWrite's default
//!   kerning off ("Keep holding…" 11px SF Pro Text Semibold: 80.2817px, Blink 79.8359 with the K-e
//!   pair's -0.4458), and its text element rounds a line's width up to a whole pixel
//!   (src/elements/text.rs:419 `.ceil()`: 81). Here every face asks for `kern` (see `faces`) and
//!   `Text` takes Blink's width (79.84375);
//! - a family is taken per character: the first family in `font-family` that has the glyph, the rest
//!   from the system fallback (Blink's FontFallbackIterator); a family that is not installed is skipped;
//! - the line box is the primary font's (the first available family's) rounded ascent and descent
//!   (`FontMetrics::AscentDescentWithHacks`: `SkScalarRoundToScalar`), with `line-height: L` adding the
//!   half-leading `floor((L - (A + D)) / 2)` above (LayoutNG `CalculateLeadingSpace`); `normal` is
//!   `round(A) + round(D) + round(lineGap)`. GPUI's default line height is 1.618 em, and its
//!   `ShapedLine::paint` centres the unrounded metrics instead;
//! - `letter-spacing` is added after every character, the last one included (`ShapeResultSpacing`);
//! - Skia draws a glyph at x rounded to a quarter pixel (the position + 1/8, floored to 1/4) and y
//!   rounded to a whole pixel (axis-aligned subpixel text), where `Window::paint_glyph` floors both: the
//!   paint adds 1/8 px and 1/2 px before handing GPUI the origin.

use gpui::{
    point, px, App, Bounds, Element, ElementId, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, TextRun, Window,
};
use std::{borrow::Cow, cell::RefCell, collections::HashSet};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory5, DWRITE_CONTAINER_TYPE_WOFF2, DWRITE_FACTORY_TYPE_SHARED,
};

// src/App.css:3-22 @font-face "SpicyLyrics" (src/fonts/spicy-lyrics, bundled): 400, 500, 600, 700.
// The files name their family "BlinkMacSystemFont" (name ID 1/16), with OS/2 weights 400..700.
const SPICY_FILES: [&[u8]; 4] = [
    include_bytes!("../../../src/fonts/spicy-lyrics/LyricsRegular.woff2"),
    include_bytes!("../../../src/fonts/spicy-lyrics/LyricsMedium.woff2"),
    include_bytes!("../../../src/fonts/spicy-lyrics/LyricsSemibold.woff2"),
    include_bytes!("../../../src/fonts/spicy-lyrics/LyricsBold.woff2"),
];

/// `@font-face` families of src/App.css: (CSS name, the faces' own family name, declared weights).
const FACE_FAMILIES: &[(&str, &str, &[u16])] = &[
    // src/App.css:3-22
    ("SpicyLyrics", "BlinkMacSystemFont", &[400, 500, 600, 700]),
    // src/App.css:24-48: local("SF Pro Text Light" … "Bold"), the installed SF Pro Text faces
    ("SF Pro Text", "SF Pro Text", &[300, 400, 500, 600, 700]),
];

/// Chromium's generic families on Windows (its default font settings): `sans-serif` is Arial.
const GENERIC: &[(&str, &str)] = &[("sans-serif", "Arial")];

// src/App.css:85-91 body
pub const BODY_STACK: &str = r#""SF Pro", "SF Pro Text", "SF Pro Display", -apple-system, "Helvetica Neue", sans-serif"#;
// src/windows/Settings.css:16 .st-card
pub const SETTINGS_STACK: &str = r#""SF Pro", "SF Pro Text", "SF Pro Display", -apple-system, "Segoe UI", sans-serif"#;
// src/App.css:62 --latin-stack
pub const LATIN_STACK: &str = r#""SpicyLyrics", "Noto Sans Georgian", "Vazirmatn", "SF Pro Text""#;
// src/App.css:69 --cjk-stack
pub const CJK_STACK: &str = r#""Yu Gothic UI", "Noto Sans KR", "Malgun Gothic", "Microsoft YaHei UI""#;
// src/App.css:78-80 --cjk-ko, --cjk-ja, --cjk-zh
pub const CJK_KO: &str = r#""Noto Sans KR", "Malgun Gothic""#;
pub const CJK_JA: &str = r#""Yu Gothic UI", "Meiryo""#;
pub const CJK_ZH: &str = r#""Microsoft YaHei UI", "Microsoft JhengHei UI", "Microsoft YaHei""#;

/// `.lyric-line` (src/App.css:338): `var(--latin-stack), <cjk>, sans-serif`, where `<cjk>` is
/// `--cjk-stack` or, under `:lang(ko|ja|zh)` (src/App.css:540-550), that language's stack.
pub fn lyric_stack(latin: &str, cjk: &str) -> String {
    format!("{latin}, {cjk}, sans-serif")
}

/// Adds the bundled faces to GPUI's text system. Call once per `App`, before any text is shaped.
pub fn load_fonts(cx: &App) -> Result<(), String> {
    let fonts = unpacked()?.into_iter().map(Cow::Owned).collect();
    cx.text_system().add_fonts(fonts).map_err(|e| e.to_string())?;
    INSTALLED.with(|i| *i.borrow_mut() = Some(cx.text_system().all_font_names().into_iter().collect()));
    Ok(())
}

/// The bundled faces as OpenType bytes: the WOFF2 files unpacked by DirectWrite
/// (`IDWriteFactory5::UnpackFontFile`). `text_shadow` opens them too, for its glyph masks.
pub(super) fn unpacked() -> Result<Vec<Vec<u8>>, String> {
    let mut fonts = Vec::with_capacity(SPICY_FILES.len());
    unsafe {
        let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(|e| e.to_string())?;
        for file in SPICY_FILES {
            let stream = factory
                .UnpackFontFile(DWRITE_CONTAINER_TYPE_WOFF2, file.as_ptr() as _, file.len() as u32)
                .map_err(|e| e.to_string())?;
            let size = stream.GetFileSize().map_err(|e| e.to_string())?;
            let mut start = std::ptr::null_mut();
            let mut ctx = std::ptr::null_mut();
            stream.ReadFileFragment(&mut start, 0, size, &mut ctx).map_err(|e| e.to_string())?;
            let bytes = std::slice::from_raw_parts(start as *const u8, size as usize).to_vec();
            stream.ReleaseFileFragment(ctx);
            fonts.push(bytes);
        }
    }
    Ok(fonts)
}

thread_local! {
    /// Family names GPUI can open (system and bundled), read once by `load_fonts`.
    static INSTALLED: RefCell<Option<HashSet<String>>> = const { RefCell::new(None) };
}

fn installed(family: &str) -> bool {
    INSTALLED.with(|i| i.borrow().as_ref().is_some_and(|s| s.contains(family)))
}

/// The names in a CSS `font-family` list (quoted or not), in order.
pub fn families(list: &str) -> Vec<String> {
    list.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

/// CSS Fonts 4 §5.2 step 4 (font-weight): the declared weight a face set offers for `want`.
fn match_weight(want: u16, have: &[u16]) -> u16 {
    let mut w: Vec<u16> = have.to_vec();
    w.sort_unstable();
    if w.contains(&want) {
        return want;
    }
    let up = |lo: u16, hi: u16| w.iter().copied().find(|v| *v > lo && *v <= hi);
    let down = || w.iter().rev().copied().find(|v| *v < want);
    let pick = if (400..=500).contains(&want) {
        up(want, 500).or_else(down).or_else(|| up(500, u16::MAX))
    } else if want < 400 {
        down().or_else(|| up(want, u16::MAX))
    } else {
        up(want, u16::MAX).or_else(down)
    };
    pick.unwrap_or(want)
}

/// The GPUI fonts a CSS `font-family` list resolves to at `weight`, in order; families that are not
/// installed are left out (Chromium skips them too).
pub fn faces(list: &str, weight: u16) -> Vec<Font> {
    let mut out: Vec<Font> = Vec::new();
    for name in families(list) {
        let (family, w) = if let Some((_, family, weights)) = FACE_FAMILIES.iter().find(|(css, _, _)| *css == name) {
            (family.to_string(), match_weight(weight, weights))
        } else if let Some((_, family)) = GENERIC.iter().find(|(g, _)| *g == name) {
            (family.to_string(), weight)
        } else {
            (name, weight)
        };
        if !installed(&family) || out.iter().any(|f| f.family.as_ref() == family && f.weight.0 == w as f32) {
            continue;
        }
        // kerning on, as HarfBuzz has it in Blink (see the module doc); with any feature listed gpui
        // also sets liga, clig and calt (direct_write.rs:1758-1762), HarfBuzz's defaults too
        out.push(Font {
            family: family.into(),
            features: FontFeatures(std::sync::Arc::new(vec![("kern".into(), 1)])),
            fallbacks: None,
            weight: FontWeight(w as f32),
            style: FontStyle::Normal,
        });
    }
    out
}

/// CSS `line-height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// `normal`: the primary font's rounded ascent + descent + line gap.
    Normal,
    /// A number: × the font size.
    Number(f32),
    /// A length in px.
    Px(f32),
}

/// The text properties a line is set with.
#[derive(Clone, Debug)]
pub struct Style {
    /// The CSS `font-family` list.
    pub families: SharedString,
    pub weight: u16,
    /// `font-size` in px.
    pub size: f32,
    pub line_height: LineHeight,
    /// `letter-spacing` in px (`em` values × the font size).
    pub letter_spacing: f32,
    pub color: Hsla,
}

impl Style {
    pub fn new(families: impl Into<SharedString>, weight: u16, size: f32, line_height: LineHeight, color: Hsla) -> Self {
        Style { families: families.into(), weight, size, line_height, letter_spacing: 0.0, color }
    }
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.letter_spacing = px;
        self
    }
}

/// One glyph of a shaped line.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub font_id: gpui::FontId,
    pub id: gpui::GlyphId,
    /// Pen position from the line's start, px (advances, kerning and letter-spacing before it).
    pub x: f32,
    /// UTF-8 index of the glyph's cluster in the text.
    pub index: usize,
    pub is_emoji: bool,
}

/// A line shaped as Chromium shapes it (see the module doc).
#[derive(Clone, Debug)]
pub struct Shaped {
    pub glyphs: Vec<Glyph>,
    /// The advance sum with letter-spacing, unrounded.
    pub width: f32,
    /// The primary font's ascent, descent and line gap, rounded to whole px as Blink does.
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub size: f32,
}

/// The family GPUI resolved `id` from (None: a face only the system fallback picked).
fn family_of(id: gpui::FontId, window: &Window) -> Option<SharedString> {
    window.text_system().get_font_for_id(id).map(|f| f.family)
}

fn run(len: usize, font: &Font) -> TextRun {
    TextRun { len, font: font.clone(), color: gpui::black(), background_color: None, underline: None, strikethrough: None }
}

/// Shapes `text` (one line, no newlines) with `style`.
pub fn shape(text: &str, style: &Style, window: &mut Window) -> Shaped {
    let mut stack = faces(&style.families, style.weight);
    if stack.is_empty() {
        stack.push(gpui::font(".SystemUIFont"));
    }
    let size = px(style.size);
    let ts = window.text_system().clone();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    // per character: the first family in the stack that has its glyph (unassigned ones stay with the
    // primary font, whose layout takes them from the system fallback)
    let mut pick: Vec<Option<usize>> = vec![None; chars.len()];
    for (k, f) in stack.iter().enumerate() {
        if pick.iter().all(Option::is_some) {
            break;
        }
        let line = ts.shape_line(SharedString::from(text.to_string()), size, &[run(text.len(), f)], None);
        // cluster starts, each with whether this family drew it
        let mut starts: Vec<(usize, bool)> = Vec::new();
        for r in &line.runs {
            let own = family_of(r.font_id, window).is_some_and(|fam| fam == f.family);
            starts.extend(r.glyphs.iter().map(|g| (g.index, own)));
        }
        starts.sort_by_key(|s| s.0);
        for (i, (b, _)) in chars.iter().enumerate() {
            if pick[i].is_some() {
                continue;
            }
            if let Some(&(_, own)) = starts.iter().rev().find(|s| s.0 <= *b) {
                if own {
                    pick[i] = Some(k);
                }
            }
        }
    }
    // runs of one face: (face, start byte, end byte)
    let mut segs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, (b, c)) in chars.iter().enumerate() {
        let k = pick[i].unwrap_or(0);
        match segs.last_mut() {
            Some(last) if last.0 == k => last.2 = b + c.len_utf8(),
            _ => segs.push((k, *b, b + c.len_utf8())),
        }
    }
    // Each run is shaped on its own, as HarfBuzz shapes Blink's font runs (no kerning across a font
    // change). It is also needed: in one GPUI line a bundled face after a system face's run is drawn in
    // the system face (gpui-0.2.2 direct_write.rs:636-642 set the custom collection per range, which
    // DirectWrite does not honour there; measured: "夜 走" set Yu Gothic UI / SpicyLyrics / Yu Gothic UI
    // drew the space in Yu Gothic UI, 4.67px against BlinkMacSystemFont's 3.36px).
    let ls = style.letter_spacing;
    let before = |index: usize| chars.iter().take_while(|(b, _)| *b < index).count() as f32;
    let mut glyphs = Vec::new();
    let mut pen = 0.0;
    for &(k, a, b) in &segs {
        let line = ts.shape_line(SharedString::from(text[a..b].to_string()), size, &[run(b - a, &stack[k])], None);
        for r in &line.runs {
            for g in &r.glyphs {
                glyphs.push(Glyph {
                    font_id: r.font_id,
                    id: g.id,
                    x: pen + f32::from(g.position.x) + ls * before(a + g.index),
                    index: a + g.index,
                    is_emoji: g.is_emoji,
                });
            }
        }
        pen += f32::from(line.width);
    }
    let primary = ts.resolve_font(&stack[0]);
    let ascent = f32::from(ts.ascent(primary, size));
    let descent = -f32::from(ts.descent(primary, size));
    // DirectWrite's line metrics (gpui LineLayout ascent/descent) are the font's ascent + descent +
    // line gap; the font metrics are ascent and descent alone
    let line_gap = {
        let one = ts.shape_line(SharedString::from(" "), size, &[run(1, &stack[0])], None);
        (f32::from(one.ascent) + f32::from(one.descent) - ascent - descent).max(0.0)
    };
    Shaped {
        glyphs,
        width: pen + ls * chars.len() as f32,
        ascent: ascent.round(),
        descent: descent.round(),
        line_gap: line_gap.round(),
        size: style.size,
    }
}

/// `LayoutUnit::FromFloatCeil`: up to the next 1/64 px.
fn layout_unit_ceil(v: f32) -> f32 {
    (v as f64 * 64.0).ceil() as f32 / 64.0
}

impl Shaped {
    /// The text's inline size, as Blink stores it.
    pub fn box_width(&self) -> f32 {
        layout_unit_ceil(self.width)
    }

    /// The line box height for `lh`.
    pub fn line_box(&self, lh: LineHeight) -> f32 {
        match lh {
            LineHeight::Normal => self.ascent + self.descent + self.line_gap,
            LineHeight::Number(k) => super::layout_unit(self.size * k),
            LineHeight::Px(v) => super::layout_unit(v),
        }
    }

    /// The baseline's offset from the top of a line box `line_box` px high.
    pub fn baseline(&self, line_box: f32) -> f32 {
        ((line_box - (self.ascent + self.descent)) / 2.0).floor() + self.ascent
    }

    /// Paints the line with its line box's top-left at `origin` (window px), scaled by `s` about that
    /// point (1 = unscaled; a `css::scale` maps the box and passes its factor).
    pub fn paint(&self, origin: Point<Pixels>, line_box: f32, s: f32, color: Hsla, window: &mut Window) {
        let sf = window.scale_factor();
        let size = px(self.size * s);
        let base = f32::from(origin.y) + self.baseline(line_box) * s;
        // Skia's rounding (module doc), in device px; paint_glyph floors
        let y = ((base * sf + 0.5) / sf).into();
        for g in &self.glyphs {
            let x = ((f32::from(origin.x) + g.x * s) * sf + 0.125) / sf;
            let at = point(px(x), px(y));
            let _ = if g.is_emoji {
                window.paint_emoji(at, g.font_id, g.id, size)
            } else {
                window.paint_glyph(at, g.font_id, g.id, size, color)
            };
        }
    }
}

/// One line of text (`white-space: nowrap`) as an element: laid out at its Blink size (inline size by
/// `box_width`, block size by `line_box`) and painted with `Shaped::paint`.
pub fn text(text: impl Into<SharedString>, style: Style) -> Text {
    Text { text: text.into(), style, shaped: None }
}

pub struct Text {
    text: SharedString,
    style: Style,
    shaped: Option<Shaped>,
}

impl IntoElement for Text {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Text {
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
        // nowrap text never shrinks below its width (flex min-width: auto)
        st.flex_shrink = 0.0;
        self.shaped = Some(shaped);
        (window.request_layout(st, [], cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut Window, _: &mut App) {}
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, _: &mut App) {
        if let Some(sh) = &self.shaped {
            sh.paint(bounds.origin, sh.line_box(self.style.line_height), 1.0, self.style.color, window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{families, layout_unit_ceil, match_weight};

    #[test]
    fn family_lists() {
        assert_eq!(families(super::LATIN_STACK), ["SpicyLyrics", "Noto Sans Georgian", "Vazirmatn", "SF Pro Text"]);
        assert_eq!(families(r#""SF Pro", -apple-system, sans-serif"#), ["SF Pro", "-apple-system", "sans-serif"]);
    }

    #[test]
    fn css_weight_matching() {
        let sf = [300, 400, 500, 600, 700];
        assert_eq!(match_weight(600, &sf), 600);
        assert_eq!(match_weight(900, &sf), 700);
        assert_eq!(match_weight(800, &sf), 700);
        assert_eq!(match_weight(200, &sf), 300);
        let spicy = [400, 500, 600, 700];
        assert_eq!(match_weight(300, &spicy), 400);
        assert_eq!(match_weight(450, &[400, 700]), 400);
        assert_eq!(match_weight(450, &[300, 700]), 300);
        assert_eq!(match_weight(450, &[700]), 700);
    }

    #[test]
    fn ceil_to_layout_unit() {
        // "Keep holding…" 11px SF Pro Text Semibold: 79.8359 → 79.84375 (Chromium's Range width)
        assert_eq!(layout_unit_ceil(79.8359375), 79.84375);
        assert_eq!(layout_unit_ceil(79.836), 79.84375);
        assert_eq!(layout_unit_ceil(80.0), 80.0);
    }
}
