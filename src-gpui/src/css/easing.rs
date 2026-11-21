//! CSS easing functions (`transition-timing-function`, `animation-timing-function`).
//!
//! The cubic-bezier solver is Chromium's `gfx::CubicBezier` (ui/gfx/geometry/cubic_bezier.cc), step for
//! step: polynomial coefficients, an 11-point spline table for the first guess, at most 4 Newton
//! iterations, then bisection, epsilon 1e-7; outside [0, 1] the curve continues along its end
//! gradients. Blink's keyword curves are its `CubicBezierTimingFunction` presets; `linear` is the
//! identity. Unit tests compare against `golden/easing.json`, sampled from Electron
//! (`node scripts/primitives/run.mjs --golden`).

/// ui/gfx/geometry/cubic_bezier.cc: kBezierEpsilon
const BEZIER_EPSILON: f64 = 1e-7;
/// ui/gfx/geometry/cubic_bezier.cc: kMaxNewtonIterations
const MAX_NEWTON_ITERATIONS: usize = 4;
/// ui/gfx/geometry/cubic_bezier.h: CUBIC_BEZIER_SPLINE_SAMPLES
const SPLINE_SAMPLES: usize = 11;

/// A CSS easing function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    CubicBezier(f64, f64, f64, f64),
}

impl Easing {
    /// CSS `ease`
    pub const EASE: Easing = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
    /// CSS `ease-in`
    pub const EASE_IN: Easing = Easing::CubicBezier(0.42, 0.0, 1.0, 1.0);
    /// CSS `ease-out`
    pub const EASE_OUT: Easing = Easing::CubicBezier(0.0, 0.0, 0.58, 1.0);
    /// CSS `ease-in-out`
    pub const EASE_IN_OUT: Easing = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);
    /// CSS `linear`
    pub const LINEAR: Easing = Easing::Linear;
    /// The app's standard curve, cubic-bezier(0.4, 0, 0.2, 1) (e.g. src/App.css:273)
    pub const STANDARD: Easing = Easing::CubicBezier(0.4, 0.0, 0.2, 1.0);
    /// The app's springy curve, cubic-bezier(0.3, 1.5, 0.5, 1) (src/windows/Settings.css:273)
    pub const SPRINGY: Easing = Easing::CubicBezier(0.3, 1.5, 0.5, 1.0);
    /// The app's springier curve, cubic-bezier(0.3, 1.6, 0.5, 1) (src/windows/Settings.css:475)
    pub const SPRINGIER: Easing = Easing::CubicBezier(0.3, 1.6, 0.5, 1.0);

    /// The eased progress for input progress `x`.
    pub fn apply(&self, x: f64) -> f64 {
        match *self {
            Easing::Linear => x,
            Easing::CubicBezier(x1, y1, x2, y2) => CubicBezier::new(x1, y1, x2, y2).solve(x),
        }
    }
}

/// Chromium's `gfx::CubicBezier`.
#[derive(Clone, Debug)]
pub struct CubicBezier {
    ax: f64,
    bx: f64,
    cx: f64,
    ay: f64,
    by: f64,
    cy: f64,
    start_gradient: f64,
    end_gradient: f64,
    spline_samples: [f64; SPLINE_SAMPLES],
}

impl CubicBezier {
    pub fn new(p1x: f64, p1y: f64, p2x: f64, p2y: f64) -> Self {
        // InitCoefficients
        let cx = 3.0 * p1x;
        let bx = 3.0 * (p2x - p1x) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * p1y;
        let by = 3.0 * (p2y - p1y) - cy;
        let ay = 1.0 - cy - by;
        // InitGradients
        let start_gradient = if p1x > 0.0 {
            p1y / p1x
        } else if p1y == 0.0 && p2x > 0.0 {
            p2y / p2x
        } else if p1y == 0.0 && p2y == 0.0 {
            1.0
        } else {
            0.0
        };
        let end_gradient = if p2x < 1.0 {
            (p2y - 1.0) / (p2x - 1.0)
        } else if p2y == 1.0 && p1x < 1.0 {
            (p1y - 1.0) / (p1x - 1.0)
        } else if p2y == 1.0 && p1y == 1.0 {
            1.0
        } else {
            0.0
        };
        let mut b = Self { ax, bx, cx, ay, by, cy, start_gradient, end_gradient, spline_samples: [0.0; SPLINE_SAMPLES] };
        // InitSpline
        let delta_t = 1.0 / (SPLINE_SAMPLES - 1) as f64;
        for i in 0..SPLINE_SAMPLES {
            b.spline_samples[i] = b.sample_curve_x(i as f64 * delta_t);
        }
        b
    }

    fn sample_curve_x(&self, t: f64) -> f64 {
        // `ax t^3 + bx t^2 + cx t' expanded using Horner's rule.
        ((self.ax * t + self.bx) * t + self.cx) * t
    }
    fn sample_curve_y(&self, t: f64) -> f64 {
        ((self.ay * t + self.by) * t + self.cy) * t
    }
    fn sample_curve_derivative_x(&self, t: f64) -> f64 {
        (3.0 * self.ax * t + 2.0 * self.bx) * t + self.cx
    }

    /// Given an x value, find a parametric value it came from. x must be in [0, 1].
    fn solve_curve_x(&self, x: f64, epsilon: f64) -> f64 {
        let mut t0 = 0.0;
        let mut t1 = 0.0;
        let mut t2 = x;
        let mut x2 = 0.0;

        // Linear interpolation of spline curve for initial guess.
        let delta_t = 1.0 / (SPLINE_SAMPLES - 1) as f64;
        for i in 1..SPLINE_SAMPLES {
            if x <= self.spline_samples[i] {
                t1 = delta_t * i as f64;
                t0 = t1 - delta_t;
                t2 = t0 + (t1 - t0) * (x - self.spline_samples[i - 1]) / (self.spline_samples[i] - self.spline_samples[i - 1]);
                break;
            }
        }

        // Perform a few iterations of Newton's method -- normally very fast.
        let newton_epsilon = epsilon.min(BEZIER_EPSILON);
        for _ in 0..MAX_NEWTON_ITERATIONS {
            x2 = self.sample_curve_x(t2) - x;
            if x2.abs() < newton_epsilon {
                return t2;
            }
            let d2 = self.sample_curve_derivative_x(t2);
            if d2.abs() < BEZIER_EPSILON {
                break;
            }
            t2 -= x2 / d2;
        }
        if x2.abs() < epsilon {
            return t2;
        }

        // Fall back to the bisection method for reliability.
        while t0 < t1 {
            x2 = self.sample_curve_x(t2);
            if (x2 - x).abs() < epsilon {
                return t2;
            }
            if x > x2 {
                t0 = t2;
            } else {
                t1 = t2;
            }
            t2 = (t1 + t0) * 0.5;
        }

        // Failure.
        t2
    }

    /// CubicBezier::SolveWithEpsilon
    pub fn solve_with_epsilon(&self, x: f64, epsilon: f64) -> f64 {
        if x < 0.0 {
            return 0.0 + self.start_gradient * x;
        }
        if x > 1.0 {
            return 1.0 + self.end_gradient * (x - 1.0);
        }
        self.sample_curve_y(self.solve_curve_x(x, epsilon))
    }

    /// CubicBezier::Solve
    pub fn solve(&self, x: f64) -> f64 {
        self.solve_with_epsilon(x, BEZIER_EPSILON)
    }
}

/// The curve a golden curve name stands for (keys of `golden/easing.json` "curves").
#[cfg(test)]
pub(crate) fn by_golden_name(name: &str) -> Easing {
    match name {
        "ease" => Easing::EASE,
        "ease-in" => Easing::EASE_IN,
        "ease-out" => Easing::EASE_OUT,
        "ease-in-out" => Easing::EASE_IN_OUT,
        "linear" => Easing::LINEAR,
        "0.4,0,0.2,1" => Easing::STANDARD,
        "0.3,1.5,0.5,1" => Easing::SPRINGY,
        "0.3,1.6,0.5,1" => Easing::SPRINGIER,
        other => {
            let v: Vec<f64> = other.split(',').map(|s| s.parse().unwrap()).collect();
            Easing::CubicBezier(v[0], v[1], v[2], v[3])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Golden {
        waapi: Vec<Waapi>,
        css: Vec<Css>,
    }
    #[derive(serde::Deserialize)]
    struct Waapi {
        curve: String,
        x: f64,
        y: f64,
    }
    #[derive(serde::Deserialize)]
    struct Css {
        curve: String,
        t: f64,
        opacity: f64,
    }

    fn golden() -> Golden {
        serde_json::from_str(include_str!("golden/easing.json")).unwrap()
    }

    /// WAAPI progress is Blink's double: the solver must give the same number.
    #[test]
    fn matches_chromium_waapi_progress() {
        let g = golden();
        let mut worst = 0.0f64;
        for s in &g.waapi {
            let y = by_golden_name(&s.curve).apply(s.x);
            let d = (y - s.y).abs();
            worst = worst.max(d);
            assert!(d < 1e-12, "{} at {}: {} vs chromium {}", s.curve, s.x, y, s.y);
        }
        assert!(g.waapi.len() > 1000, "golden too small");
        eprintln!("easing: {} samples, worst |d| = {worst:e}", g.waapi.len());
    }

    /// The transition path (getComputedStyle opacity, a float) uses the same curve. Opacity's computed
    /// value is clamped to [0, 1], so an overshooting curve reads 1 past its peak.
    #[test]
    fn matches_chromium_css_transitions() {
        let g = golden();
        for s in &g.css {
            let y = (by_golden_name(&s.curve).apply(s.t / 1000.0) as f32).clamp(0.0, 1.0);
            assert!((y as f64 - s.opacity).abs() < 2e-6, "{} at {} ms: {} vs chromium {}", s.curve, s.t, y, s.opacity);
        }
    }

    #[test]
    fn ends_and_overshoot() {
        for e in [Easing::EASE, Easing::STANDARD, Easing::SPRINGY, Easing::SPRINGIER] {
            assert_eq!(e.apply(0.0), 0.0);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-12);
        }
        // the springy curves overshoot 1 on the way
        assert!(Easing::SPRINGY.apply(0.5) > 1.0);
        assert!(Easing::SPRINGIER.apply(0.5) > Easing::SPRINGY.apply(0.5));
    }
}
