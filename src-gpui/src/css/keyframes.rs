//! CSS `@keyframes` + `animation`: `Keyframes` (one property's stops) sampled under an `Animation`
//! (the shared timing: duration, delay, iteration count, direction, fill).
//!
//! Timing is the Web Animations model Blink runs CSS animations on (third_party/blink/renderer/core/
//! animation/timing.cc, CalculateTimings): local time → phase (before / active / after) → active time
//! (per fill mode) → overall and simple iteration progress (a whole iteration ending exactly at the end
//! counts as progress 1 of the previous iteration) → directed progress (alternate flips odd
//! iterations). The effect itself is linear: as in Blink, `animation-timing-function` is applied per
//! keyframe interval (each stop carries its own easing, CSS Animations §3.9), from that stop to the next.
//!
//! Several properties in one @keyframes block are one `Keyframes` each with the same offsets and easings,
//! sampled under the same `Animation` (Blink's PropertySpecificKeyframes).
//!
//! Time is milliseconds; `start` is when the animation was applied (the element got its `animation`).

use super::easing::Easing;
use super::tween::Lerp;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// The `animation` shorthand minus the name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub duration_ms: f64,
    /// `animation-timing-function`: the easing of every stop that does not set its own.
    pub easing: Easing,
    pub delay_ms: f64,
    /// `f64::INFINITY` for `infinite`.
    pub iterations: f64,
    pub direction: Direction,
    pub fill: Fill,
}

impl Animation {
    /// `animation: <name> <duration> <easing>` (CSS initial values for the rest: 0s, 1, normal, none).
    pub const fn new(duration_ms: f64, easing: Easing) -> Self {
        Self { duration_ms, easing, delay_ms: 0.0, iterations: 1.0, direction: Direction::Normal, fill: Fill::None }
    }
    pub const fn delay(mut self, ms: f64) -> Self {
        self.delay_ms = ms;
        self
    }
    pub const fn infinite(mut self) -> Self {
        self.iterations = f64::INFINITY;
        self
    }
    pub const fn direction(mut self, d: Direction) -> Self {
        self.direction = d;
        self
    }
    pub const fn fill(mut self, f: Fill) -> Self {
        self.fill = f;
        self
    }

    fn active_duration(&self) -> f64 {
        if self.iterations == 0.0 {
            0.0
        } else {
            self.duration_ms * self.iterations
        }
    }

    /// The directed (pre-keyframe) progress at local time `t` ms, or None where the animation has no effect.
    pub fn progress(&self, t: f64) -> Option<f64> {
        let active_dur = self.active_duration();
        let end = (self.delay_ms + active_dur).max(0.0);
        let backwards = matches!(self.fill, Fill::Backwards | Fill::Both);
        let forwards = matches!(self.fill, Fill::Forwards | Fill::Both);
        // phase and active time (timing.cc CalculatePhase / CalculateActiveTime)
        let (active_time, after) = if t < self.delay_ms.min(end) {
            if !backwards {
                return None;
            }
            (0.0, false)
        } else if t < (self.delay_ms + active_dur).min(end) {
            (t - self.delay_ms, false)
        } else {
            if !forwards {
                return None;
            }
            (active_dur.min((end - self.delay_ms).max(0.0)), true)
        };
        if self.duration_ms <= 0.0 {
            return Some(if after { 1.0 } else { 0.0 });
        }
        // overall / simple iteration progress (timing.cc CalculateOverallProgress, CalculateSimpleIterationProgress)
        let overall = active_time / self.duration_ms;
        let mut simple = if overall.is_infinite() { 0.0 } else { overall % 1.0 };
        if simple == 0.0 && after && active_time == active_dur && self.iterations != 0.0 && active_time != 0.0 {
            simple = 1.0;
        }
        // current iteration (CalculateCurrentIteration)
        let iteration = if after && self.iterations.is_infinite() {
            f64::INFINITY
        } else if simple == 1.0 {
            overall.floor() - 1.0
        } else {
            overall.floor()
        };
        // directed progress (CalculateDirectedProgress)
        let forwards_dir = match self.direction {
            Direction::Normal => true,
            Direction::Reverse => false,
            Direction::Alternate | Direction::AlternateReverse => {
                let even = if iteration.is_infinite() { true } else { iteration % 2.0 == 0.0 };
                even == (self.direction == Direction::Alternate)
            }
        };
        Some(if forwards_dir { simple } else { 1.0 - simple })
    }
}

/// One stop of a property: `<offset>% { prop: value; animation-timing-function: easing }`.
#[derive(Clone, Copy, Debug)]
pub struct Stop<T> {
    pub offset: f64,
    pub value: T,
    /// None: the animation's `animation-timing-function`.
    pub easing: Option<Easing>,
}

pub fn stop<T>(offset: f64, value: T) -> Stop<T> {
    Stop { offset, value, easing: None }
}

/// One property's keyframes. The stops must run from offset 0 to offset 1 (fill a missing `from`/`to`
/// with the element's own value, as CSS does), sorted by offset.
#[derive(Clone, Debug)]
pub struct Keyframes<T> {
    stops: Vec<Stop<T>>,
}

impl<T: Lerp> Keyframes<T> {
    pub fn new(stops: Vec<Stop<T>>) -> Self {
        assert!(stops.len() >= 2, "keyframes need at least two stops");
        assert!(stops.first().unwrap().offset == 0.0 && stops.last().unwrap().offset == 1.0, "keyframes must cover 0..1");
        assert!(stops.windows(2).all(|w| w[0].offset <= w[1].offset), "keyframe offsets must be sorted");
        Self { stops }
    }

    /// The value at keyframe progress `p` in [0, 1] (keyframe_effect_model / interpolation_effect).
    pub fn at(&self, p: f64, default_easing: Easing) -> T {
        let s = &self.stops;
        // the interval: the last stop with offset <= p, but never the last stop itself
        let mut i = 0;
        while i + 2 < s.len() && s[i + 1].offset <= p {
            i += 1;
        }
        // stops at the same offset: p at that offset takes the later one
        while i + 2 < s.len() && s[i + 1].offset == s[i].offset {
            i += 1;
        }
        let (a, b) = (&s[i], &s[i + 1]);
        let span = b.offset - a.offset;
        if span <= 0.0 {
            return b.value;
        }
        let local = ((p - a.offset) / span).clamp(0.0, 1.0);
        let eased = a.easing.unwrap_or(default_easing).apply(local);
        T::lerp(a.value, b.value, eased)
    }

    /// The animated value at `now` for an animation applied at `start`, or None where it has no effect
    /// (the element shows its own value).
    pub fn sample(&self, anim: &Animation, start: f64, now: f64) -> Option<T> {
        anim.progress(now - start).map(|p| self.at(p, anim.easing))
    }

    /// The animated value, or `base` where the animation has no effect.
    pub fn value_or(&self, anim: &Animation, start: f64, now: f64, base: T) -> T {
        self.sample(anim, start, now).unwrap_or(base)
    }
}

/// True while an animation applied at `start` still changes at `now`: the window must keep requesting
/// frames. (An infinite animation is always active; a finished one holds its fill value.)
pub fn active(anim: &Animation, start: f64, now: f64) -> bool {
    now - start < anim.delay_ms + anim.active_duration()
}

#[cfg(test)]
mod tests {
    use super::*;

    // src/App.css:1600-1606 @keyframes bar-pulse
    fn bar_pulse() -> Keyframes<f32> {
        Keyframes::new(vec![stop(0.0, 3.0), stop(0.25, 8.0), stop(0.5, 5.0), stop(0.75, 14.0), stop(1.0, 8.0)])
    }

    #[test]
    fn alternate_infinite_with_delay() {
        // src/App.css:1581-1584: 1.1s ease-in-out infinite alternate, nth-child(2) delay .18s
        let anim = Animation::new(1100.0, Easing::EASE_IN_OUT).infinite().direction(Direction::Alternate).delay(180.0);
        let k = bar_pulse();
        assert_eq!(k.sample(&anim, 0.0, 100.0), None, "fill none: no effect during the delay");
        assert_eq!(k.sample(&anim, 0.0, 180.0), Some(3.0));
        assert_eq!(k.sample(&anim, 0.0, 180.0 + 275.0), Some(8.0));
        assert_eq!(k.sample(&anim, 0.0, 180.0 + 1100.0), Some(8.0), "the turn of an alternate");
        // iteration 1 runs backwards: 25% into it is the 75% stop
        assert_eq!(k.sample(&anim, 0.0, 180.0 + 1100.0 + 275.0), Some(14.0));
        assert!(active(&anim, 0.0, 1e9));
    }

    #[test]
    fn forwards_holds_the_end() {
        // src/App.css:1706 fs-in 280ms cubic-bezier(0.4, 0, 0.2, 1) forwards
        let anim = Animation::new(280.0, Easing::STANDARD).fill(Fill::Forwards);
        let k = Keyframes::new(vec![stop(0.0, 0.0f32), stop(1.0, 1.0)]);
        assert_eq!(k.sample(&anim, 0.0, 0.0), Some(0.0));
        assert_eq!(k.sample(&anim, 0.0, 280.0), Some(1.0));
        assert_eq!(k.sample(&anim, 0.0, 5000.0), Some(1.0));
        assert!(!active(&anim, 0.0, 280.0));
        // fill none: back to the element's value once done
        let none = Animation::new(280.0, Easing::STANDARD);
        assert_eq!(k.sample(&none, 0.0, 280.0), None);
    }

    #[test]
    fn per_stop_easing_and_shared_offsets() {
        // src/App.css:2226-2238 np-sing: 0% / 70%,85% / 100% (opacity), 3.2s cubic-bezier(0.45, 0, 0.25, 1)
        let k = Keyframes::new(vec![stop(0.0, 1.0f32), stop(0.7, 1.0), stop(0.85, 1.0), stop(1.0, 0.0)]);
        let e = Easing::CubicBezier(0.45, 0.0, 0.25, 1.0);
        assert_eq!(k.at(0.5, e), 1.0);
        assert_eq!(k.at(0.85, e), 1.0);
        let mid = k.at(0.925, e);
        assert!((mid - (1.0 - e.apply(0.5) as f32)).abs() < 1e-6);
        // a stop's own easing beats the animation's
        let k2 = Keyframes::new(vec![Stop { offset: 0.0, value: 0.0f32, easing: Some(Easing::LINEAR) }, stop(1.0, 1.0)]);
        assert_eq!(k2.at(0.3, Easing::EASE), 0.3);
    }
}
