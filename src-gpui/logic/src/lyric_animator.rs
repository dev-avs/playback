//! Port of src/lyrics/lyricAnimator.ts: per-syllable and per-letter lyric
//! motion (springs, curves, letter windows, the line stepper).
//!
//! Design note.
//!
//! * Math is translated 1:1 (same order, same expressions, so results are
//!   bit-identical). `toFixed` is used by the TS, and Rust's `{:.N}` differs
//!   from it on exact ties (Rust rounds half to even, JS picks the larger
//!   magnitude), so `to_fixed` reproduces JS. `Math.min/max` propagate NaN and
//!   order -0 < +0, so `js_min`/`js_max` do too.
//! * Math functions: Node's Math.sin/cos are V8's fdlibm port (Sun's original
//!   kernels), Math.exp and Math.pow its fdlibm ports too. Rust's std calls the
//!   C runtime, which rounds differently in the last bit, so `exp` and `pow` come
//!   from the `libm` crate (FreeBSD/musl fdlibm, bit-identical to V8 for the
//!   arguments used here) and `sin`/`cos` from `ieee754` below, a port of V8's
//!   (the libm crate's k_sin/k_cos use a re-associated polynomial that differs
//!   from V8's by one bit, e.g. sin(0.8484239419430849)).
//! * DOM reads: `collect_line(line_el)` reads, per element, its classes
//!   (`.word`, `.wordGroup`, `.dot`), `dataset.s/e/sweep` and its `.letter`
//!   children. That is the explicit input `LineInput` / `WordInput` /
//!   `LetterInput`: the elements in document order, with `s`/`e` already parsed
//!   as `Number(el.dataset.s ?? 0)` would give.
//! * DOM writes: every element is an `El`, owned by its unit. `El.style` holds
//!   what the TS leaves on `el.style`: the six custom properties it sets with
//!   `setProperty` (`--s --y --gb --go --gp --o`, as the exact strings, `None`
//!   after `removeProperty` or before the first write) plus `willChange` and
//!   `backfaceVisibility` from `promoteToGPU`; `El.gpu` is `dataset.gpu === "1"`.
//!   The unit's `w` (what the TS calls `Written`) keeps the numbers.
//! * Module state (`style`, `motion`, set by `setAnimationStyle` and
//!   `setLyricMotion`) is a `thread_local`, not a struct passed in: the TS
//!   reads it from inside `Spline.at`, `letterFreqScale` and `writeMotion`, so
//!   a thread_local keeps every signature identical to the TS. The module-level
//!   curves (`WORD_SCALE`, ...) are thread_locals too, used as `WORD_SCALE.at(x)`.
//!   Each thread (each test) starts with the TS's initial values.

#![allow(clippy::needless_range_loop)] // index loops kept as in the TS

use crate::cubic_spline::NaturalSpline;
use std::cell::Cell;
use std::thread::LocalKey;

/// V8's `base::ieee754::sin` / `cos` (src/base/ieee754.cc, from Sun's fdlibm:
/// k_sin.c, k_cos.c, e_rem_pio2.c). Not in the TS; here so `Math.sin` and
/// `Math.cos` give the same bits. Argument reduction is written out for
/// |x| <= 5pi/4 (the TS never passes more than ~3: dt is clamped to 0.1 s and
/// the highest spring frequency is 27 rad/s); larger arguments fall back to
/// the `libm` crate.
mod ieee754 {
    #![allow(clippy::excessive_precision)] // fdlibm's constants, as printed there

    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;

    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;

    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;

    fn high_word(x: f64) -> u32 {
        (x.to_bits() >> 32) as u32
    }

    fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
        let ix = high_word(x) & 0x7fffffff;
        if ix < 0x3e400000 && x as i32 == 0 {
            return x;
        }
        let z = x * x;
        let v = z * x;
        let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
        if iy == 0 {
            x + v * (S1 + z * r)
        } else {
            x - ((z * (0.5 * y - v * r) - y) - v * S1)
        }
    }

    fn kernel_cos(x: f64, y: f64) -> f64 {
        let ix = high_word(x) & 0x7fffffff;
        if ix < 0x3e400000 && x as i32 == 0 {
            return 1.0;
        }
        let z = x * x;
        let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
        if ix < 0x3fd33333 {
            1.0 - (0.5 * z - (z * r - x * y))
        } else {
            let qx = if ix > 0x3fe90000 {
                0.28125
            } else {
                f64::from_bits(((ix - 0x00200000) as u64) << 32)
            };
            let hz = 0.5 * z - qx;
            let a = 1.0 - qx;
            a - (hz - (z * r - x * y))
        }
    }

    /// (n, y0, y1) for |x| <= 5pi/4 (e_rem_pio2.c, n = +-1 .. +-2 cases), else None.
    fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
        let neg = x < 0.0;
        let ix = high_word(x) & 0x7fffffff;
        if ix > 0x400f6a7a || (ix & 0xfffff) == 0x921fb {
            return None;
        }
        let (n, m) = if ix <= 0x4002d97c { (1, 1.0) } else { (2, 2.0) };
        if !neg {
            let z = x - m * PIO2_1;
            let y0 = z - m * PIO2_1T;
            let y1 = (z - y0) - m * PIO2_1T;
            Some((n, y0, y1))
        } else {
            let z = x + m * PIO2_1;
            let y0 = z + m * PIO2_1T;
            let y1 = (z - y0) + m * PIO2_1T;
            Some((-n, y0, y1))
        }
    }

    pub fn sin(x: f64) -> f64 {
        let ix = high_word(x) & 0x7fffffff;
        if ix <= 0x3fe921fb {
            return kernel_sin(x, 0.0, 0);
        }
        if ix >= 0x7ff00000 {
            return f64::NAN;
        }
        match rem_pio2(x) {
            Some((n, y0, y1)) => match n & 3 {
                0 => kernel_sin(y0, y1, 1),
                1 => kernel_cos(y0, y1),
                2 => -kernel_sin(y0, y1, 1),
                _ => -kernel_cos(y0, y1),
            },
            None => libm::sin(x),
        }
    }

    pub fn cos(x: f64) -> f64 {
        let ix = high_word(x) & 0x7fffffff;
        if ix <= 0x3fe921fb {
            return kernel_cos(x, 0.0);
        }
        if ix >= 0x7ff00000 {
            return f64::NAN;
        }
        match rem_pio2(x) {
            Some((n, y0, y1)) => match n & 3 {
                0 => kernel_cos(y0, y1),
                1 => -kernel_sin(y0, y1, 1),
                2 => -kernel_cos(y0, y1),
                _ => kernel_sin(y0, y1, 1),
            },
            None => libm::cos(x),
        }
    }
}

// ── JS number helpers (no TS counterpart) ──

/// `Math.min(a, b)`
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b {
        a
    } else {
        b
    }
}

/// `Math.max(a, b)`
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// `Number.prototype.toFixed(digits)` for |v| < 1e21.
pub fn to_fixed(v: f64, digits: usize) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    let neg = v < 0.0;
    let mut m = v.abs();
    // JS picks the larger n on an exact tie; Rust rounds half to even. A tie has
    // a short exact expansion, so step just above it.
    let exact = format!("{:.*}", digits + 30, m);
    if let Some(dot) = exact.find('.') {
        let tail = &exact[dot + 1 + digits..];
        if tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0') {
            m = f64::from_bits(m.to_bits() + 1);
        }
    }
    let s = format!("{:.*}", digits, m);
    if neg {
        format!("-{s}")
    } else {
        s
    }
}

// ── animation style (Settings → Highlight → Word animation) ──
// lyricAnimator.ts:22-30

/// lyricAnimator.ts:32
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationStyle {
    Playback,
    Spicy,
}

thread_local! {
    /// lyricAnimator.ts:34 (`let style`)
    static STYLE: Cell<AnimationStyle> = const { Cell::new(AnimationStyle::Playback) };
    /// lyricAnimator.ts:485 (`let motion`)
    static MOTION: Cell<f64> = const { Cell::new(1.0) };
}

fn style() -> AnimationStyle {
    STYLE.with(|s| s.get())
}

fn motion() -> f64 {
    MOTION.with(|m| m.get())
}

/// Set from the widget whenever the setting changes.
/// lyricAnimator.ts:37-39
pub fn set_animation_style(next: AnimationStyle) {
    STYLE.with(|s| s.set(next));
}

/// lyricAnimator.ts:41
pub type Point = (f64, f64);

/// One keyframed curve, evaluated the way the current style wants: monotone
/// cubic Hermite (Fritsch–Carlson) for Playback, natural cubic for spicy.
/// lyricAnimator.ts:49-118
pub struct Spline {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
    natural: NaturalSpline,
}

impl Spline {
    /// lyricAnimator.ts:55-91
    pub fn new(points: &[Point]) -> Spline {
        let n = points.len();
        let xs: Vec<f64> = points.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
        let natural = NaturalSpline::new(xs.clone(), ys.clone());

        // finite differences
        let mut d = vec![0.0; n - 1];
        for i in 0..n - 1 {
            d[i] = (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]);
        }

        // tangents
        let mut ms = vec![0.0; n];
        ms[0] = d[0];
        ms[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            ms[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
        }

        // monotone correction (Fritsch–Carlson)
        for i in 0..n - 1 {
            if d[i] == 0.0 {
                ms[i] = 0.0;
                ms[i + 1] = 0.0;
            } else {
                let a = ms[i] / d[i];
                let b = ms[i + 1] / d[i];
                let h = a * a + b * b;
                if h > 9.0 {
                    let t = 3.0 / h.sqrt();
                    ms[i] = t * a * d[i];
                    ms[i + 1] = t * b * d[i];
                }
            }
        }

        Spline { xs, ys, ms, natural }
    }

    /// lyricAnimator.ts:93-117
    pub fn at(&self, x: f64) -> f64 {
        let xs = &self.xs;
        if x <= xs[0] {
            return self.ys[0];
        }
        let last = xs.len() - 1;
        if x >= xs[last] {
            return self.ys[last];
        }
        if style() == AnimationStyle::Spicy {
            return self.natural.at(x);
        }

        let mut i = 0;
        while i < last {
            if x < xs[i + 1] {
                break;
            }
            i += 1;
        }

        let h = xs[i + 1] - xs[i];
        let t = (x - xs[i]) / h;
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * self.ys[i] + h10 * h * self.ms[i] + h01 * self.ys[i + 1] + h11 * h * self.ms[i + 1]
    }
}

/// Lets a module-level curve be used as in the TS: `WORD_SCALE.at(x)`.
trait CurveAt {
    fn at(&'static self, x: f64) -> f64;
}

impl CurveAt for LocalKey<Spline> {
    fn at(&'static self, x: f64) -> f64 {
        self.with(|s| s.at(x))
    }
}

// ── curves (LyricsAnimator.ts:39-101, 115-150) ──
// lyricAnimator.ts:120-167
thread_local! {
    static WORD_SCALE: Spline = Spline::new(&[(0.0, 0.95), (0.7, 1.0505), (1.0, 1.0)]);
    static LETTER_SCALE: Spline = Spline::new(&[(0.0, 0.95), (0.7, 1.175), (1.0, 1.0)]);
    static WORD_Y: Spline = Spline::new(&[(0.0, 1.0 / 100.0), (0.9, -(1.0 / 60.0)), (1.0, 0.0)]);
    static LETTER_Y: Spline = Spline::new(&[(0.0, 1.0 / 100.0), (0.9, -(1.0 / 56.0)), (1.0, 0.0)]);
    static GLOW: Spline = Spline::new(&[(0.0, 0.0), (0.15, 1.0), (0.6, 1.0), (1.0, 0.0)]);

    static DOT_SCALE: Spline = Spline::new(&[(0.0, 0.75), (0.7, 1.05), (1.0, 1.0)]);
    static DOT_Y: Spline = Spline::new(&[(0.0, 0.0), (0.9, -0.12), (1.0, 0.0)]);
    static DOT_GLOW: Spline = Spline::new(&[(0.0, 0.0), (0.6, 1.0), (1.0, 1.0)]);
    static DOT_OPACITY: Spline = Spline::new(&[(0.0, 0.35), (0.6, 1.0), (1.0, 1.0)]);
}

// Spring tuning — frequency in Hz, damping ratio in [0,1).
// lyricAnimator.ts:169-177
const SCALE_FREQ: f64 = 0.88;
const SCALE_DAMP: f64 = 0.64;
const Y_FREQ: f64 = 1.45;
const Y_DAMP: f64 = 0.4;
const GLOW_FREQ: f64 = 1.18;
const GLOW_DAMP: f64 = 0.56;

const DOT_SCALE_FREQ: f64 = 0.7;
const DOT_SCALE_DAMP: f64 = 0.6;
const DOT_Y_FREQ: f64 = 1.25;
const DOT_Y_DAMP: f64 = 0.4;
const DOT_GLOW_FREQ: f64 = 1.0;
const DOT_GLOW_DAMP: f64 = 0.5;
const DOT_OPACITY_FREQ: f64 = 1.0;
const DOT_OPACITY_DAMP: f64 = 0.5;

/// lyricAnimator.ts:184
const SUNG_LETTER_GLOW: f64 = 0.2;

/// lyricAnimator.ts:192-195
const GLOW_BLUR_BASE: f64 = 0.09; // em
const LETTER_BLUR_RANGE: f64 = 0.27; // em
const WORD_BLUR_RANGE: f64 = 0.045; // em
const DOT_BLUR_RANGE: f64 = 0.136; // em

/// lyricAnimator.ts:201-203
const LETTER_GLOW_ALPHA: f64 = 1.85;
const WORD_GLOW_ALPHA: f64 = 0.35;
const DOT_GLOW_ALPHA: f64 = 0.9;

/// Gradient sweep endpoints, in % (Mixed.css `--gradient-position`).
/// lyricAnimator.ts:206-208
const GRAD_UNSUNG: f64 = -20.0;
const GRAD_SUNG: f64 = 100.0;
const GRAD_SPAN: f64 = 120.0;

/// lyricAnimator.ts:211
const SPICY_LETTER_MIN_S: f64 = 1.0;

/// lyricAnimator.ts:219-220
const LETTER_TAIL_S: f64 = 0.25;
const LETTER_TAIL_FRACTION: f64 = 0.25;

/// lyricAnimator.ts:223
const MIN_LETTER_MS: f64 = 30.0;

/// lyricAnimator.ts:232-233
const WAVE_REF_LETTER_MS: f64 = 125.0;
const MAX_FREQ_SCALE: f64 = 3.0;

/// lyricAnimator.ts:235-238
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(x > 0)` is also true for NaN, as in the TS
fn letter_freq_scale(window_ms: f64) -> f64 {
    // `!(windowMs > 0)` is also true for NaN
    if style() == AnimationStyle::Spicy || !(window_ms > 0.0) {
        return 1.0;
    }
    js_min(MAX_FREQ_SCALE, js_max(1.0, WAVE_REF_LETTER_MS / window_ms))
}

/// The `{ groupEnd, per }` object letterWindows returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetterWindows {
    pub group_end: f64,
    pub per: f64,
}

/// Letter-wave windows for a syllable, or None if it should stay a single unit.
/// lyricAnimator.ts:246-262
pub fn letter_windows(start: f64, end: f64, count: f64, animation: AnimationStyle) -> Option<LetterWindows> {
    let dur = end - start;
    if count < 2.0 || dur <= 0.0 {
        return None;
    }
    if animation == AnimationStyle::Spicy && dur < SPICY_LETTER_MIN_S {
        return None;
    }

    let group_end = end
        - (if animation == AnimationStyle::Spicy {
            LETTER_TAIL_S
        } else {
            js_min(LETTER_TAIL_S, dur * LETTER_TAIL_FRACTION)
        });
    let per = (group_end - start) / count;
    if per * 1000.0 < MIN_LETTER_MS {
        return None;
    }

    Some(LetterWindows { group_end, per })
}

// Falloff from the Active letter.
// lyricAnimator.ts:266-267
fn pos_falloff(distance: f64) -> f64 {
    1.0 / (1.0 + libm::pow(distance, 2.8))
}
fn glow_falloff(distance: f64) -> f64 {
    1.0 / (1.0 + distance * 0.9)
}

// d3-ease easeSinOut
// lyricAnimator.ts:270
fn ease_sin_out(t: f64) -> f64 {
    ieee754::sin((t * std::f64::consts::PI) / 2.0)
}

// ── spring (Fraktality/spr.lua, underdamped branch) ──
// lyricAnimator.ts:273-321
#[derive(Debug, Clone)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
    pub g: f64,
    pub start: f64,
    f: f64,
    d: f64,
}

impl Spring {
    /// lyricAnimator.ts:281-288
    pub fn new(start: f64, freq_hz: f64, damping: f64) -> Spring {
        Spring {
            x: start,
            v: 0.0,
            g: start,
            start,
            f: freq_hz * 2.0 * std::f64::consts::PI,
            d: damping,
        }
    }

    /// lyricAnimator.ts:290-292
    pub fn set_goal(&mut self, g: f64) {
        self.g = g;
    }

    /// lyricAnimator.ts:294-298
    pub fn reset(&mut self) {
        self.x = self.start;
        self.v = 0.0;
        self.g = self.start;
    }

    /// Advance one step; returns the new position.
    /// lyricAnimator.ts:307-320
    pub fn step(&mut self, dt: f64) -> f64 {
        let dx = self.x - self.g;
        let f = self.f;
        let d = self.d;
        let q = libm::exp(-d * f * dt);
        let c = (1.0 - d * d).sqrt();
        let i = ieee754::cos(dt * f * c);
        let j = ieee754::sin(dt * f * c);
        let z = j / c;
        let y = j / (f * c);
        self.x = (dx * (i + z * d) + self.v * y) * q + self.g;
        self.v = (self.v * (i - z * d) - dx * (z * f)) * q;
        self.x
    }
}

// ── model ──
/// lyricAnimator.ts:324
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    NotSung,
    Active,
    Sung,
}

/// Last values written to the DOM, so unchanged frames skip the write.
/// lyricAnimator.ts:327-343
#[derive(Debug, Clone, Copy)]
pub struct Written {
    pub s: f64,
    pub y: f64,
    pub gb: f64,
    pub go: f64,
    pub gp: f64,
    pub o: f64,
}

fn blank_written() -> Written {
    Written {
        s: f64::NAN,
        y: f64::NAN,
        gb: f64::NAN,
        go: f64::NAN,
        gp: f64::NAN,
        o: f64::NAN,
    }
}

/// What the TS leaves on `el.style`: the custom properties it sets with
/// `setProperty` (`None` = not set / removed) and the two properties
/// `promoteToGPU` assigns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElStyle {
    pub s: Option<String>,
    pub y: Option<String>,
    pub gb: Option<String>,
    pub go: Option<String>,
    pub gp: Option<String>,
    pub o: Option<String>,
    pub will_change: Option<String>,
    pub backface_visibility: Option<String>,
}

/// The DOM element a unit writes to (`el` in the TS).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct El {
    pub style: ElStyle,
    /// `el.dataset.gpu === "1"`
    pub gpu: bool,
}

impl El {
    /// `el.style.setProperty(prop, value)`
    pub fn set_property(&mut self, prop: &str, value: String) {
        *self.slot(prop) = Some(value);
    }

    /// `el.style.removeProperty(prop)`
    pub fn remove_property(&mut self, prop: &str) {
        *self.slot(prop) = None;
    }

    fn slot(&mut self, prop: &str) -> &mut Option<String> {
        match prop {
            "--s" => &mut self.style.s,
            "--y" => &mut self.style.y,
            "--gb" => &mut self.style.gb,
            "--go" => &mut self.style.go,
            "--gp" => &mut self.style.gp,
            "--o" => &mut self.style.o,
            _ => panic!("unknown style property {prop}"),
        }
    }
}

/// lyricAnimator.ts:345-353
#[derive(Debug, Clone)]
pub struct LetterUnit {
    pub el: El,
    pub s: f64,
    pub e: f64,
    pub scale: Spring,
    pub y: Spring,
    pub glow: Spring,
    pub w: Written,
}

/// `kind: "word" | "group" | "dot"`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    Word,
    Group,
    Dot,
}

/// lyricAnimator.ts:355-375
#[derive(Debug, Clone)]
pub struct WordUnit {
    pub el: El,
    pub s: f64,
    pub e: f64,
    pub kind: WordKind,
    /// Fill-only unit: the source has no real word sync, so the line is swept as
    /// a progress indicator and nothing scales, lifts or glows.
    pub sweep: bool,
    pub scale: Spring,
    pub y: Spring,
    pub glow: Spring,
    /// dots only
    pub opacity: Option<Spring>,
    /// populated only when kind === "group"
    pub letters: Vec<LetterUnit>,
    pub w: Written,
}

/// A `.letter` element as `makeLetter` reads it: `Number(el.dataset.s ?? 0)`,
/// `Number(el.dataset.e ?? 0)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetterInput {
    pub s: f64,
    pub e: f64,
}

/// A `.word`, `.wordGroup` or `.dot` element as `makeWord` reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct WordInput {
    /// `el.classList.contains("dot")`
    pub dot: bool,
    /// `el.classList.contains("wordGroup")`
    pub word_group: bool,
    /// `Number(el.dataset.s ?? 0)`
    pub s: f64,
    /// `Number(el.dataset.e ?? 0)`
    pub e: f64,
    /// `el.dataset.sweep === "1"`
    pub sweep: bool,
    /// `el.querySelectorAll(".letter")` (only read for a wordGroup)
    pub letters: Vec<LetterInput>,
}

/// `lineEl`: the `.word, .wordGroup, .dot` elements below it, in document order.
pub type LineInput = Vec<WordInput>;

/// lyricAnimator.ts:377-381
pub fn state_of(t: f64, s: f64, e: f64) -> UnitState {
    if t < s {
        return UnitState::NotSung;
    }
    if t >= e {
        return UnitState::Sung;
    }
    UnitState::Active
}

/// lyricAnimator.ts:383-387
fn progress(t: f64, s: f64, e: f64) -> f64 {
    if t <= s {
        return 0.0;
    }
    if t >= e {
        return 1.0;
    }
    (t - s) / (e - s)
}

/// lyricAnimator.ts:389-394
fn promote_to_gpu(el: &mut El) {
    if el.gpu {
        return;
    }
    el.style.will_change = Some("transform, opacity, text-shadow, scale".to_string());
    el.style.backface_visibility = Some("hidden".to_string());
    el.gpu = true;
}

/// lyricAnimator.ts:396-412
fn make_letter(input: &LetterInput) -> LetterUnit {
    let mut el = El::default();
    promote_to_gpu(&mut el);
    let s = input.s;
    let e = input.e;
    // Only the letter springs are rescaled (Playback style); word-level motion
    // runs at the reference's tuning against the syllable's own window.
    let k = letter_freq_scale((e - s) * 1000.0);
    LetterUnit {
        el,
        s,
        e,
        scale: Spring::new(LETTER_SCALE.at(0.0), SCALE_FREQ * k, SCALE_DAMP),
        y: Spring::new(LETTER_Y.at(0.0), Y_FREQ * k, Y_DAMP),
        glow: Spring::new(GLOW.at(0.0), GLOW_FREQ * k, GLOW_DAMP),
        w: blank_written(),
    }
}

/// lyricAnimator.ts:414-447
fn make_word(input: &WordInput) -> WordUnit {
    let mut el = El::default();
    promote_to_gpu(&mut el);
    let kind = if input.dot {
        WordKind::Dot
    } else if input.word_group {
        WordKind::Group
    } else {
        WordKind::Word
    };

    let is_dot = kind == WordKind::Dot;
    WordUnit {
        el,
        s: input.s,
        e: input.e,
        kind,
        sweep: input.sweep,
        scale: if is_dot {
            Spring::new(DOT_SCALE.at(0.0), DOT_SCALE_FREQ, DOT_SCALE_DAMP)
        } else {
            Spring::new(WORD_SCALE.at(0.0), SCALE_FREQ, SCALE_DAMP)
        },
        y: if is_dot {
            Spring::new(DOT_Y.at(0.0), DOT_Y_FREQ, DOT_Y_DAMP)
        } else {
            Spring::new(WORD_Y.at(0.0), Y_FREQ, Y_DAMP)
        },
        glow: if is_dot {
            Spring::new(DOT_GLOW.at(0.0), DOT_GLOW_FREQ, DOT_GLOW_DAMP)
        } else {
            Spring::new(GLOW.at(0.0), GLOW_FREQ, GLOW_DAMP)
        },
        opacity: if is_dot {
            Some(Spring::new(DOT_OPACITY.at(0.0), DOT_OPACITY_FREQ, DOT_OPACITY_DAMP))
        } else {
            None
        },
        letters: if kind == WordKind::Group {
            input.letters.iter().map(make_letter).collect()
        } else {
            Vec::new()
        },
        w: blank_written(),
    }
}

/// Build the animatable model for one rendered line.
/// lyricAnimator.ts:450-455
pub fn collect_line(line_el: &LineInput) -> Vec<WordUnit> {
    line_el.iter().map(make_word).collect()
}

// ── DOM writes ──
/// lyricAnimator.ts:458-469
fn write(el: &mut El, prop: &str, value: f64, last: f64, format: fn(f64) -> String, epsilon: f64) -> f64 {
    // `last === last` is false only for NaN
    if !last.is_nan() && (value - last).abs() < epsilon {
        return last;
    }
    el.set_property(prop, format(value));
    value
}

/// lyricAnimator.ts:471-473
pub fn fmt4(v: f64) -> String {
    to_fixed(v, 4)
}
pub fn fmt3(v: f64) -> String {
    to_fixed(v, 3)
}
pub fn fmt_pct(v: f64) -> String {
    format!("{}%", to_fixed(v, 2))
}

/// lyricAnimator.ts:475-477
const EPS_TRANSFORM: f64 = 1e-3;
const EPS_GLOW: f64 = 2e-3;
const EPS_GRAD: f64 = 0.05;

/// `WordUnit | LetterUnit`, as far as the writers and `clearUnit` touch them.
trait Unit {
    fn el_w(&mut self) -> (&mut El, &mut Written);
    fn springs(&mut self) -> [&mut Spring; 3];
}

impl Unit for WordUnit {
    fn el_w(&mut self) -> (&mut El, &mut Written) {
        (&mut self.el, &mut self.w)
    }
    fn springs(&mut self) -> [&mut Spring; 3] {
        [&mut self.scale, &mut self.y, &mut self.glow]
    }
}

impl Unit for LetterUnit {
    fn el_w(&mut self) -> (&mut El, &mut Written) {
        (&mut self.el, &mut self.w)
    }
    fn springs(&mut self) -> [&mut Spring; 3] {
        [&mut self.scale, &mut self.y, &mut self.glow]
    }
}

/// The Lyric motion setting: scales how far each unit grows and lifts away from
/// rest (1 = the tuned motion above, 0 = still, more = bouncier).
/// lyricAnimator.ts:485-488 (`let motion` is the MOTION thread_local above)
pub fn set_lyric_motion(k: f64) {
    MOTION.with(|m| m.set(k));
}

/// lyricAnimator.ts:490-493
fn write_motion<U: Unit>(u: &mut U, scale: f64, y: f64, y_mul: f64) {
    let (el, w) = u.el_w();
    w.s = write(el, "--s", 1.0 + (scale - 1.0) * motion(), w.s, fmt4, EPS_TRANSFORM);
    w.y = write(el, "--y", y * y_mul * motion(), w.y, fmt4, EPS_TRANSFORM);
}

/// lyricAnimator.ts:495-504
fn write_glow<U: Unit>(u: &mut U, glow: f64, blur_range: f64, alpha_mul: f64) {
    let g = js_max(0.0, glow);
    let (el, w) = u.el_w();
    w.gb = write(el, "--gb", GLOW_BLUR_BASE + g * blur_range, w.gb, fmt3, EPS_GLOW);
    w.go = write(el, "--go", js_min(g * alpha_mul, 1.0), w.go, fmt3, EPS_GLOW);
}

/// lyricAnimator.ts:506-508
fn write_gradient<U: Unit>(u: &mut U, pct: f64) {
    let (el, w) = u.el_w();
    w.gp = write(el, "--gp", pct, w.gp, fmt_pct, EPS_GRAD);
}

// ── per-frame stepping ──

/// lyricAnimator.ts:512-599
fn step_letters(group: &mut WordUnit, t: f64, dt: f64) {
    let word_state = state_of(t, group.s, group.e);
    let letters = &mut group.letters;

    // Exactly one letter drives the whole group.
    let mut active_idx: isize = -1;
    let mut active_p = 0.0;
    if word_state == UnitState::Active {
        for i in 0..letters.len() {
            if state_of(t, letters[i].s, letters[i].e) == UnitState::Active {
                active_idx = i as isize;
                active_p = progress(t, letters[i].s, letters[i].e);
                break;
            }
        }
    }

    let rest_scale = LETTER_SCALE.at(0.0);
    let rest_y = LETTER_Y.at(0.0);
    let rest_glow = GLOW.at(0.0);

    for k in 0..letters.len() {
        let l = &mut letters[k];
        let letter_state = state_of(t, l.s, l.e);

        let mut target_scale = rest_scale;
        let mut target_y = rest_y;
        let mut target_glow = rest_glow;
        let target_grad: f64;

        if word_state == UnitState::Active {
            if active_idx != -1 {
                let base_scale = LETTER_SCALE.at(active_p);
                let base_y = LETTER_Y.at(active_p);
                let base_glow = GLOW.at(active_p);

                let distance = (k as f64 - active_idx as f64).abs();
                let f = pos_falloff(distance);
                let gf = glow_falloff(distance);

                target_scale = rest_scale + (base_scale - rest_scale) * f;
                target_y = rest_y + (base_y - rest_y) * f;
                target_glow = rest_glow + (base_glow - rest_glow) * gf;
            }

            // Letters the bump has not reached yet stay flat regardless of proximity.
            if letter_state == UnitState::NotSung {
                target_scale = rest_scale;
                target_y = rest_y;
                target_glow = rest_glow;
            } else if letter_state == UnitState::Sung && active_idx == -1 {
                target_glow = GLOW.at(SUNG_LETTER_GLOW);
            }

            // Only the one Active letter has a moving fill. Everything else is
            // binary: fully painted or not painted at all.
            target_grad = if letter_state == UnitState::NotSung {
                GRAD_UNSUNG
            } else if letter_state == UnitState::Sung {
                GRAD_SUNG
            } else if k as isize == active_idx {
                GRAD_UNSUNG + GRAD_SPAN * ease_sin_out(active_p)
            } else {
                GRAD_UNSUNG
            };
        } else if word_state == UnitState::NotSung {
            target_grad = GRAD_UNSUNG;
        } else {
            target_scale = LETTER_SCALE.at(1.0);
            target_y = LETTER_Y.at(1.0);
            target_glow = GLOW.at(1.0);
            target_grad = GRAD_SUNG;
        }

        l.scale.set_goal(target_scale);
        l.y.set_goal(target_y);
        l.glow.set_goal(target_glow);

        let c_scale = l.scale.step(dt);
        let c_y = l.y.step(dt);
        let c_glow = l.glow.step(dt);

        // ×2: the reference doubles the letter lift so it clears the word-level
        // lift it is riding on top of.
        write_motion(l, c_scale, c_y, 2.0);
        write_glow(l, c_glow, LETTER_BLUR_RANGE, LETTER_GLOW_ALPHA);
        write_gradient(l, target_grad);
    }
}

/// lyricAnimator.ts:601-618
fn step_dot(dot: &mut WordUnit, t: f64, dt: f64) {
    let state = state_of(t, dot.s, dot.e);
    let p = if state == UnitState::Active {
        progress(t, dot.s, dot.e)
    } else if state == UnitState::Sung {
        1.0
    } else {
        0.0
    };

    dot.scale.set_goal(DOT_SCALE.at(p));
    dot.y.set_goal(DOT_Y.at(p));
    dot.glow.set_goal(DOT_GLOW.at(p));
    dot.opacity.as_mut().unwrap().set_goal(DOT_OPACITY.at(p));

    let c_scale = dot.scale.step(dt);
    let c_y = dot.y.step(dt);
    let c_glow = dot.glow.step(dt);
    let c_opacity = dot.opacity.as_mut().unwrap().step(dt);

    write_motion(dot, c_scale, c_y, 1.0);
    write_glow(dot, c_glow, DOT_BLUR_RANGE, DOT_GLOW_ALPHA);
    dot.w.o = write(&mut dot.el, "--o", c_opacity, dot.w.o, fmt3, EPS_TRANSFORM);
}

/// Advance one frame for a whole line.
///
/// `words` the line's units, in reading order; `t` playback position in seconds
/// (already offset-corrected); `dt` seconds since the previous frame.
/// lyricAnimator.ts:627-687
pub fn step_line(words: &mut [WordUnit], t: f64, dt: f64) {
    if words.is_empty() {
        return;
    }
    // A backgrounded window can hand us a multi-second dt. The springs converge
    // rather than explode at that size, but clamping keeps the first frame after
    // a stall from skipping the settle entirely.
    let d = js_min(js_max(dt, 0.0), 0.1);

    for w in words.iter_mut() {
        if w.kind == WordKind::Dot {
            step_dot(w, t, d);
            continue;
        }

        let state = state_of(t, w.s, w.e);
        let p = if state == UnitState::Active {
            progress(t, w.s, w.e)
        } else if state == UnitState::Sung {
            1.0
        } else {
            0.0
        };

        if w.sweep {
            // Fill only. Motion and glow are left untouched, so the CSS resting
            // values hold and nothing pops on a timing we merely guessed.
            write_gradient(
                w,
                if state == UnitState::Active {
                    GRAD_UNSUNG + GRAD_SPAN * p
                } else if state == UnitState::Sung {
                    GRAD_SUNG
                } else {
                    GRAD_UNSUNG
                },
            );
            continue;
        }

        w.scale.set_goal(WORD_SCALE.at(p));
        w.y.set_goal(WORD_Y.at(p));
        w.glow.set_goal(GLOW.at(p));

        let c_scale = w.scale.step(d);
        let c_y = w.y.step(d);
        let c_glow = w.glow.step(d);

        // Word-level motion applies to plain words *and* to letter groups — the
        // group carries its syllable's own rise while its letters ripple inside it.
        write_motion(w, c_scale, c_y, 1.0);

        if w.kind == WordKind::Group {
            step_letters(w, t, d);
        } else {
            // Whole-syllable fill: linear, no easing (the reference eases only the
            // per-letter sweep).
            write_gradient(
                w,
                if state == UnitState::Active {
                    GRAD_UNSUNG + GRAD_SPAN * p
                } else if state == UnitState::Sung {
                    GRAD_SUNG
                } else {
                    GRAD_UNSUNG
                },
            );
            write_glow(w, c_glow, WORD_BLUR_RANGE, WORD_GLOW_ALPHA);
        }
    }
}

/// lyricAnimator.ts:689
const CUSTOM_PROPS: [&str; 6] = ["--s", "--y", "--gb", "--go", "--gp", "--o"];

/// lyricAnimator.ts:691-697
fn clear_unit<U: Unit>(u: &mut U) {
    {
        let (el, w) = u.el_w();
        for prop in CUSTOM_PROPS {
            el.remove_property(prop);
        }
        *w = blank_written();
    }
    for sp in u.springs() {
        sp.reset();
    }
}

/// Return a line's units to idle, e.g. when it stops being current.
/// lyricAnimator.ts:700-706
pub fn reset_line(words: &mut [WordUnit]) {
    for w in words {
        clear_unit(w);
        if let Some(o) = w.opacity.as_mut() {
            o.reset();
        }
        for l in &mut w.letters {
            clear_unit(l);
        }
    }
}
