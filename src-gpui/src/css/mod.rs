//! The CSS layer (port.md 2.4): one shared Rust implementation per CSS feature the app uses, each proven
//! against a tiny Electron test page (`scripts/primitives/`, `Playback.exe --primitive <case>`).
//!
//! Slice A (motion): `easing`, `tween`, `keyframes`, `clip_height`, `hover`, plus `translate` (the
//! keyframe cases move boxes by fractions of a pixel, which taffy's rounded offsets cannot do).
//! Slice B (geometry): `scale`, `rounded`, `shadow`, `mask`.
//! All time arguments are milliseconds on the window's clock (a virtual one in the primitive cases).
//! The screens (Phases 5-7) use the rest of each helper's API; until then only the primitive cases call it.
#![allow(dead_code, unused_imports)]

pub mod clip_height;
pub mod easing;
pub mod hover;
pub mod keyframes;
pub mod mask;
pub mod scale;
pub mod rounded;
pub mod shadow;
pub mod translate;
pub mod tween;

pub use clip_height::ClipHeight;
pub use easing::Easing;
pub use hover::{Hover, HoverProp};
pub use mask::{masked, Masked};
pub use scale::{scale, Scale};
pub use keyframes::{stop, Animation, Direction, Fill, Keyframes, Stop};
pub use rounded::{clip_child, Radii};
pub use shadow::{inset, inset_shadow, outer, shadowed, Shadow};
pub use translate::translate;
pub use tween::{Lerp, Transition, Tween};

/// A length as Blink stores it for layout: a LayoutUnit, 1/64 px, truncated toward zero
/// (third_party/blink/renderer/platform/geometry/layout_unit.h, `LayoutUnit(float)`).
pub fn layout_unit(px: f32) -> f32 {
    (px as f64 * 64.0).trunc() as f32 / 64.0
}

