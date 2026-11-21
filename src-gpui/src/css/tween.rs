//! CSS `transition` on one property: `Tween`.
//!
//! Semantics are CSS Transitions Level 1 §3 as Chromium implements them
//! (third_party/blink/renderer/core/animation/css/css_animations.cc, CalculateTransitionUpdateForProperty):
//! - a new target starts a transition from the property's *current* (possibly mid-transition) value;
//! - when the new target is the running transition's reversing-adjusted start value (the value it
//!   came from), the new transition is a reversal: its duration (and a negative delay) is multiplied by
//!   the reversing shortening factor S = clamp(|p × S_old + 1 − S_old|, 0, 1), p = the running
//!   transition's current *output* (eased) progress;
//! - a target equal to the current value cancels the running transition;
//! - during the delay the start value shows (transitions fill backwards).
//! Unit tests replay `golden/transition.json` (getComputedStyle every 1/60 s on Electron's virtual clock).
//!
//! Interpolation: numbers linearly; colours in premultiplied sRGB, as Blink interpolates legacy
//! `rgb()/rgba()` colours (a fade from `transparent` keeps the target's hue).
//!
//! Time is milliseconds on whatever clock drives the window (the frame driver's, or a virtual one).

use super::easing::Easing;
use gpui::Rgba;

/// A value a transition / keyframe animation can interpolate.
pub trait Lerp: Copy + PartialEq {
    fn lerp(a: Self, b: Self, p: f64) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: f32, b: f32, p: f64) -> f32 {
        (a as f64 + (b as f64 - a as f64) * p) as f32
    }
}

impl Lerp for Rgba {
    /// Premultiplied interpolation, then back to straight alpha (Blink's InterpolableColor for sRGB).
    fn lerp(a: Rgba, b: Rgba, p: f64) -> Rgba {
        let l = |x: f64, y: f64| x + (y - x) * p;
        let (aa, ba) = (a.a as f64, b.a as f64);
        let alpha = l(aa, ba).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
        }
        let c = |x: f32, y: f32| (l(x as f64 * aa, y as f64 * ba) / alpha).clamp(0.0, 1.0) as f32;
        Rgba { r: c(a.r, b.r), g: c(a.g, b.g), b: c(a.b, b.b), a: alpha as f32 }
    }
}

/// A `transition` declaration for one property: `<property> <duration> <easing> <delay>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub duration_ms: f64,
    pub easing: Easing,
    pub delay_ms: f64,
}

impl Transition {
    pub const fn new(duration_ms: f64, easing: Easing) -> Self {
        Self { duration_ms, easing, delay_ms: 0.0 }
    }
    pub const fn delay(mut self, delay_ms: f64) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

#[derive(Clone, Copy, Debug)]
struct Running<T> {
    from: T,
    to: T,
    start: f64,
    duration: f64,
    delay: f64,
    easing: Easing,
    reversing_adjusted_start: T,
    shortening: f64,
}

impl<T: Lerp> Running<T> {
    /// Output (eased) progress at `now`; before the delay ends it is 0.
    fn progress(&self, now: f64) -> f64 {
        let local = now - self.start - self.delay;
        if local <= 0.0 {
            return 0.0;
        }
        if self.duration <= 0.0 || local >= self.duration {
            return 1.0;
        }
        self.easing.apply(local / self.duration)
    }
    fn done(&self, now: f64) -> bool {
        now - self.start - self.delay >= self.duration
    }
    fn value(&self, now: f64) -> T {
        if self.done(now) {
            return self.to;
        }
        T::lerp(self.from, self.to, self.progress(now))
    }
}

/// One transitioning property. `set` is the style change, `value` the animated value.
#[derive(Clone, Debug)]
pub struct Tween<T> {
    spec: Transition,
    target: T,
    running: Option<Running<T>>,
}

impl<T: Lerp> Tween<T> {
    /// A property at `value`, transitioning per `spec` from now on.
    pub fn new(value: T, spec: Transition) -> Self {
        Self { spec, target: value, running: None }
    }

    /// The style's value (what the transition heads for).
    pub fn target(&self) -> T {
        self.target
    }

    /// The `transition` declaration in force (used for the next change).
    pub fn set_spec(&mut self, spec: Transition) {
        self.spec = spec;
    }

    /// The property's new style value at time `now` (ms).
    pub fn set(&mut self, to: T, now: f64) {
        if to == self.target {
            return;
        }
        let current = self.value(now);
        let old = self.running.filter(|r| !r.done(now));
        self.target = to;
        // css_animations.cc: a target equal to the current value cancels the running transition
        if current == to {
            self.running = None;
            return;
        }
        if self.spec.duration_ms + self.spec.delay_ms <= 0.0 {
            self.running = None;
            return;
        }
        let mut duration = self.spec.duration_ms;
        let mut delay = self.spec.delay_ms;
        let mut reversing_adjusted_start = current;
        let mut shortening = 1.0;
        if let Some(r) = old {
            if to == r.reversing_adjusted_start {
                // reversing shortening factor (CSS Transitions §3, step 4 of "start a transition")
                reversing_adjusted_start = r.to;
                shortening = (r.progress(now) * r.shortening + (1.0 - r.shortening)).abs().clamp(0.0, 1.0);
                duration *= shortening;
                if delay < 0.0 {
                    delay *= shortening;
                }
            }
        }
        self.running = Some(Running {
            from: current,
            to,
            start: now,
            duration,
            delay,
            easing: self.spec.easing,
            reversing_adjusted_start,
            shortening,
        });
    }

    /// Jump to `value` with no transition (the property set while `transition: none`).
    pub fn jump(&mut self, value: T) {
        self.target = value;
        self.running = None;
    }

    /// The animated value at `now`.
    pub fn value(&self, now: f64) -> T {
        match &self.running {
            Some(r) => r.value(now),
            None => self.target,
        }
    }

    /// True while a transition is running at `now`: the window must keep requesting frames.
    pub fn active(&self, now: f64) -> bool {
        self.running.as_ref().is_some_and(|r| !r.done(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Golden {
        reversals: Vec<Case>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        a: String,
        b: String,
        c: Option<String>,
        transition: String,
        flips: Vec<f64>,
        samples: Vec<Sample>,
    }
    #[derive(serde::Deserialize)]
    struct Sample {
        t: f64,
        v: f64,
    }

    fn num(s: &str) -> f32 {
        s.trim_end_matches("px").parse().unwrap()
    }
    /// "opacity 300ms ease" / "max-height 350ms cubic-bezier(0.4, 0, 0.2, 1)"
    fn spec(s: &str) -> Transition {
        let mut it = s.splitn(3, ' ');
        let _prop = it.next();
        let dur: f64 = it.next().unwrap().trim_end_matches("ms").parse().unwrap();
        let e = it.next().unwrap();
        let easing = match e {
            "ease" => Easing::EASE,
            "linear" => Easing::LINEAR,
            _ => {
                let v: Vec<f64> = e.trim_start_matches("cubic-bezier(").trim_end_matches(')').split(',').map(|x| x.trim().parse().unwrap()).collect();
                Easing::CubicBezier(v[0], v[1], v[2], v[3])
            }
        };
        Transition::new(dur, easing)
    }

    /// Replays each golden case: the same style changes at the same virtual times must give
    /// Chromium's getComputedStyle values (float, so 1e-5 relative to the property's range).
    #[test]
    fn reversal_matches_chromium() {
        let g: Golden = serde_json::from_str(include_str!("golden/transition.json")).unwrap();
        assert!(g.reversals.len() >= 5);
        for c in &g.reversals {
            let (a, b) = (num(&c.a), num(&c.b));
            let mut tw = Tween::new(a, spec(&c.transition));
            tw.set(b, 0.0);
            let mut flips = c.flips.clone();
            let mut flip = 0;
            let mut target = b;
            let range = (a - b).abs().max(c.c.as_deref().map_or(0.0, |x| (num(x) - a).abs())) as f64;
            for s in &c.samples {
                if !flips.is_empty() && s.t + 1e-6 >= flips[0] {
                    flips.remove(0);
                    flip += 1;
                    target = match (&c.c, flip) {
                        (Some(x), 1) => num(x),
                        _ => {
                            if target == b {
                                a
                            } else {
                                b
                            }
                        }
                    };
                    tw.set(target, s.t);
                }
                let v = tw.value(s.t) as f64;
                assert!((v - s.v).abs() <= 1e-5 * range.max(1.0), "{} at {} ms: {} vs chromium {}", c.name, s.t, v, s.v);
            }
        }
    }

    #[test]
    fn premultiplied_colour() {
        let from = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
        let to = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.08 };
        let mid = Rgba::lerp(from, to, 0.5);
        assert_eq!((mid.r, mid.g, mid.b), (1.0, 1.0, 1.0));
        assert!((mid.a - 0.04).abs() < 1e-6);
    }

    #[test]
    fn active_until_done() {
        let mut tw = Tween::new(1.0f32, Transition::new(300.0, Easing::EASE));
        assert!(!tw.active(0.0));
        tw.set(0.2, 10.0);
        assert!(tw.active(10.0) && tw.active(309.0));
        assert!(!tw.active(310.0));
        assert_eq!(tw.value(400.0), 0.2);
    }
}
