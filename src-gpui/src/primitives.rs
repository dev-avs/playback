//! The primitive cases of the CSS layer (parity/plan.md Phase 4): each scene draws what
//! `scripts/primitives/pages/<case>.html` draws, with the `css/` helpers, so `--primitive <case>` can be
//! diffed against the Electron capture. The cases (size, frame count, timeline) come from
//! `scripts/primitives/cases.json`, the same file the Electron runner reads.
//!
//! Every value is the page's (the page cites the App.css line it was taken from); the stand-in boxes
//! (sizes, fills marked "harness" in the page) are the page's own.

use crate::css::{
    inset, inset_shadow, layout_unit, outer, rounded::uniform, shadowed, stop, translate, Animation, ClipHeight,
    Direction, Easing, Fill, HoverProp, Hover, Keyframes, Transition, Tween,
};
use crate::css::mask::{masked, LYRICS_PANEL};
use crate::css::scale::{self, scale};
use crate::css::rounded::{clip_child, Radii, Rect};
use gpui::{div, img, point, prelude::*, px, AnyElement, Bounds, FontWeight, Hsla, ImageSource, Pixels, Point, Rgba};
use serde::Deserialize;
use std::{cell::Cell, rc::Rc};

#[derive(Deserialize)]
struct CaseFile {
    cases: Vec<Case>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Case {
    pub name: String,
    /// Window size in DIPs.
    pub size: [i32; 2],
    /// Motion frames every `FRAME_MS` from t = 0; 0 = one still.
    pub frames: u32,
    pub timeline: Vec<Op>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Op {
    pub t: f64,
    pub op: String,
    pub add: Option<String>,
    pub remove: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

/// scripts/primitives/main.cjs: frame k is captured at k × two 1/60 s steps of virtual time.
pub fn frame_time(k: u32) -> f64 {
    k as f64 * (1000.0 / 60.0) * 2.0
}

pub fn cases() -> Vec<Case> {
    let f: CaseFile = serde_json::from_str(include_str!("../../scripts/primitives/cases.json")).expect("cases.json");
    f.cases
}

/// `rgba(r, g, b, a)` as CSS writes it.
fn rgba(r: u8, g: u8, b: u8, a: f32) -> Rgba {
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

fn fill(c: Rgba) -> Hsla {
    c.into()
}

// pages/*.html: `.stage { display: block; padding: 22px }` (src/App.css:101)
const STAGE_PAD: f32 = 22.0;

fn stage() -> gpui::Div {
    div().size_full().p(px(STAGE_PAD)).flex().flex_col().items_start()
}

pub enum Scene {
    /// tween-reverse, tween-color: one card with `transition: background 400ms ease, opacity 300ms ease`.
    Card { hovered: bool, warm: bool, bg: Tween<Rgba>, opacity: Tween<f32> },
    /// clip-lyrics: the `.lyrics-panel` max-height clip inside a widget with `overflow: hidden`.
    /// mask-lyrics-open: the same, with the panel's mask-image.
    Lyrics { open: bool, panel: ClipHeight, margin_bottom: Tween<f32>, mask: bool },
    /// clip-controls: the `.controls` max-height clip that turns `overflow: visible` at once on hover.
    Controls { open: bool, panel: ClipHeight, opacity: Tween<f32>, margin_top: Tween<f32> },
    /// hover-next-up: `.next-up:hover` through the injected cursor and the cursor poll.
    NextUp { hover: Hover, bounds: Rc<Cell<Option<Bounds<Pixels>>>>, opacity: HoverProp<f32>, bg: HoverProp<Rgba> },
    /// kf-*: the element got its `animation` at `start` (None: the class is not on yet).
    BarPulse { start: Option<f64> },
    EmptyPulse { start: Option<f64> },
    FsIn { start: Option<f64> },
    NextPopupIn { start: Option<f64> },
    VolPillIn { start: Option<f64> },
    /// shadow-*, rounded-*, mask-*, scale-pill-0xx / -text: one still, drawn by `still`.
    Still(&'static str),
    /// scale-pill-in: the reset pill's entry animation.
    PillIn { start: Option<f64> },
    /// scale-pill-pop: the reset pill landing (pop animation, background transition).
    PillPop { start: Option<f64>, bg: Tween<Rgba> },
}

// pages/tween-*.html (src/App.css:115-131): background rgba(20, 20, 22, .72)
fn card_bg() -> Rgba {
    rgba(20, 20, 22, 0.72)
}
// pages/tween-color.html (src/settings.ts:259 GLASS_TINTS.warm)
fn card_bg_warm() -> Rgba {
    rgba(56, 34, 22, 0.72)
}
// pages/tween-reverse.html (src/App.css:135): .widget:not(.hovered) { opacity: .2 }
const REST_OPACITY: f32 = 0.2;

impl Scene {
    pub fn new(name: &str) -> Option<Scene> {
        Some(match name {
            "tween-reverse" | "tween-color" => Scene::Card {
                hovered: true,
                warm: false,
                // src/App.css:115-131: transition background 400ms ease, opacity 300ms ease
                bg: Tween::new(card_bg(), Transition::new(400.0, Easing::EASE)),
                opacity: Tween::new(1.0, Transition::new(300.0, Easing::EASE)),
            },
            "clip-lyrics" | "mask-lyrics-open" => Scene::Lyrics {
                mask: name == "mask-lyrics-open",
                open: false,
                // src/App.css:268-293: max-height 350ms cubic-bezier(0.4, 0, 0.2, 1), margin-bottom 350ms ease
                panel: ClipHeight::new(0.0, Transition::new(350.0, Easing::STANDARD)),
                margin_bottom: Tween::new(0.0, Transition::new(350.0, Easing::EASE)),
            },
            "clip-controls" => Scene::Controls {
                open: false,
                // src/App.css:892-910: max-height 200ms ease, opacity 160ms ease, margin-top 200ms ease
                panel: ClipHeight::new(0.0, Transition::new(200.0, Easing::EASE)),
                opacity: Tween::new(0.0, Transition::new(160.0, Easing::EASE)),
                margin_top: Tween::new(0.0, Transition::new(200.0, Easing::EASE)),
            },
            "hover-next-up" => Scene::NextUp {
                hover: Hover::default(),
                bounds: Rc::new(Cell::new(None)),
                // src/App.css:2438-2462: opacity .75 → 1, background none → rgba(255,255,255,.08), 120ms ease
                opacity: HoverProp::new(0.75, 1.0, Transition::new(120.0, Easing::EASE)),
                bg: HoverProp::new(rgba(0, 0, 0, 0.0), rgba(255, 255, 255, 0.08), Transition::new(120.0, Easing::EASE)),
            },
            "kf-bar-pulse" => Scene::BarPulse { start: None },
            "kf-empty-pulse" => Scene::EmptyPulse { start: None },
            "kf-fs-in" => Scene::FsIn { start: None },
            "kf-next-popup-in" => Scene::NextPopupIn { start: None },
            "kf-vol-pill-in" => Scene::VolPillIn { start: None },
            "shadow-widget" => Scene::Still("shadow-widget"),
            "shadow-preview" => Scene::Still("shadow-preview"),
            "shadow-vol-pill" => Scene::Still("shadow-vol-pill"),
            "shadow-hairline-pill" => Scene::Still("shadow-hairline-pill"),
            "rounded-widget" => Scene::Still("rounded-widget"),
            "mask-lyrics" => Scene::Still("mask-lyrics"),
            "scale-pill-085" => Scene::Still("scale-pill-085"),
            "scale-pill-094" => Scene::Still("scale-pill-094"),
            "scale-pill-110" => Scene::Still("scale-pill-110"),
            "scale-pill-text" => Scene::Still("scale-pill-text"),
            "scale-pill-in" => Scene::PillIn { start: None },
            // src/windows/Settings.css:429-444: transition background 160ms ease
            "scale-pill-pop" => Scene::PillPop { start: None, bg: Tween::new(PILL_BG, Transition::new(160.0, Easing::EASE)) },
            _ => return None,
        })
    }

    /// A `class` op of the timeline at `now`: the style changes, transitions / animations start.
    pub fn class(&mut self, add: Option<&str>, remove: Option<&str>, now: f64) {
        let on = |name: &str, cur: bool| if add == Some(name) { true } else if remove == Some(name) { false } else { cur };
        match self {
            Scene::Card { hovered, warm, bg, opacity } => {
                *hovered = on("hovered", *hovered);
                *warm = on("warm", *warm);
                opacity.set(if *hovered { 1.0 } else { REST_OPACITY }, now);
                bg.set(if *warm { card_bg_warm() } else { card_bg() }, now);
            }
            Scene::Lyrics { open, panel, margin_bottom, .. } => {
                *open = on("lyrics-open", *open);
                // src/App.css:291-294: .widget.lyrics-open .lyrics-panel { max-height: var(--panel-h, 300px); margin-bottom: 8px }
                panel.set(if *open { 300.0 } else { 0.0 }, now);
                margin_bottom.set(if *open { 8.0 } else { 0.0 }, now);
            }
            Scene::Controls { open, panel, opacity, margin_top } => {
                *open = on("hovered", *open);
                // src/App.css:905-911: .widget.hovered .controls { max-height: 32px; opacity: 1; margin-top: 5px }
                panel.set(if *open { 32.0 } else { 0.0 }, now);
                opacity.set(if *open { 1.0 } else { 0.0 }, now);
                margin_top.set(if *open { 5.0 } else { 0.0 }, now);
            }
            Scene::BarPulse { start } if add == Some("playing") => *start = Some(now),
            Scene::EmptyPulse { start } if add == Some("connecting") => *start = Some(now),
            Scene::FsIn { start } if add == Some("fullscreen") => *start = Some(now),
            Scene::NextPopupIn { start } | Scene::VolPillIn { start } | Scene::PillIn { start } if add == Some("show") => *start = Some(now),
            Scene::PillPop { start, bg } if add == Some("done") => {
                *start = Some(now);
                bg.set(PILL_DONE_BG, now);
            }
            _ => panic!("primitive: class op {add:?}/{remove:?} does not apply to this scene"),
        }
    }

    /// One cursor-poll tick at `now`: the cursor in window coordinates, None outside the window.
    pub fn poll_cursor(&mut self, cursor: Option<Point<Pixels>>, now: f64) {
        if let Scene::NextUp { hover, bounds, opacity, bg } = self {
            if let Some(b) = bounds.get() {
                hover.set_bounds(b);
            }
            if let Some(h) = hover.poll(cursor) {
                opacity.follow(h, now);
                bg.follow(h, now);
            }
        }
    }

    pub fn render(&self, now: f64) -> AnyElement {
        match self {
            Scene::Still(name) => still(name),
            Scene::PillIn { start } => {
                let Some(s) = *start else { return stage().into_any_element() };
                // src/windows/Settings.css:482-485 @keyframes st-reset-in: opacity 0 → 1, scale .85 → 1;
                // src/windows/Settings.css:429-444: 260ms cubic-bezier(0.3, 1.5, 0.5, 1) both
                let anim = Animation::new(260.0, Easing::SPRINGY).fill(Fill::Both);
                let o = Keyframes::new(vec![stop(0.0, 0.0f32), stop(1.0, 1.0)]).value_or(&anim, s, now, 1.0);
                let k = Keyframes::new(vec![stop(0.0, 0.85f32), stop(1.0, 1.0)]).value_or(&anim, s, now, 1.0);
                stage().child(scale(k, o, reset_pill(PILL_BG, PILL_EDGE, stand_in()))).into_any_element()
            }
            Scene::PillPop { start, bg } => {
                // src/windows/Settings.css:472-476 .st-reset-done .st-reset-pill: background (transitioned),
                // box-shadow (not transitioned), st-reset-pop 380ms cubic-bezier(0.3, 1.6, 0.5, 1) both
                let (edge, k) = match *start {
                    None => (PILL_EDGE, 1.0),
                    Some(s) => {
                        let anim = Animation::new(380.0, Easing::SPRINGIER).fill(Fill::Both);
                        // src/windows/Settings.css:487-491 @keyframes st-reset-pop: 0% .94, 45% 1.1, 100% 1
                        let k = Keyframes::new(vec![stop(0.0, 0.94f32), stop(0.45, 1.1), stop(1.0, 1.0)]).value_or(&anim, s, now, 1.0);
                        (PILL_DONE_EDGE, k)
                    }
                };
                let bg = bg.value(now);
                stage().child(scale(k, 1.0, reset_pill(bg, edge, stand_in()))).into_any_element()
            }
            Scene::Card { bg, opacity, .. } => stage()
                .child(
                    // src/App.css:115-131 .widget: radius 13px; 200 × 80 harness stand-in for the content
                    div().w(px(200.0)).h(px(80.0)).rounded(px(13.0)).bg(fill(bg.value(now))).opacity(opacity.value(now)),
                )
                .into_any_element(),

            Scene::Lyrics { panel, margin_bottom, mask, .. } => {
                // pages/clip-lyrics.html: 8 rows of 30px, odd .18 / even .06 white
                let rows = || {
                    div().flex_shrink_0().w_full().flex().flex_col().children((0..8).map(|i| {
                        let a = if i % 2 == 0 { 0.18 } else { 0.06 };
                        div().flex_shrink_0().w_full().h(px(30.0)).bg(fill(rgba(255, 255, 255, a)))
                    }))
                };
                // src/App.css:268-293 .lyrics-panel: margin 0 -10px, margin-bottom transitions, overflow hidden;
                // pages/mask-lyrics-open.html adds its mask-image (src/App.css:277-288)
                let panel_el = if *mask {
                    let p = panel.clone();
                    div().flex_shrink_0().child(masked(LYRICS_PANEL, move || p.wrap(240.0, now, true, rows()).w_full().into_any_element()))
                } else {
                    panel.wrap(240.0, now, true, rows())
                };
                stage()
                    .child(
                        // src/App.css:115-131 .widget: padding 10px 10px 8px, width 200px content-box
                        // (GPUI sizes border-box: 200 + 2 × 10), column, overflow hidden, radius 13px
                        div()
                            .w(px(220.0))
                            .pt(px(10.0))
                            .px(px(10.0))
                            .pb(px(8.0))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded(px(13.0))
                            .bg(fill(card_bg()))
                            // harness .head 40px rgba(255,255,255,.3)
                            .child(div().flex_shrink_0().h(px(40.0)).bg(fill(rgba(255, 255, 255, 0.3))))
                            .child(panel_el.mx(px(-10.0)).mb(px(layout_unit(margin_bottom.value(now)))))
                            // harness .foot 24px rgba(255,255,255,.5)
                            .child(div().flex_shrink_0().h(px(24.0)).bg(fill(rgba(255, 255, 255, 0.5)))),
                    )
                    .into_any_element()
            }

            Scene::Controls { open, panel, opacity, margin_top } => stage()
                .child(
                    // src/App.css:115-131 .widget (no overflow clip here), width 200 content-box + 2 × 10
                    div()
                        .w(px(220.0))
                        .pt(px(10.0))
                        .px(px(10.0))
                        .pb(px(8.0))
                        .flex()
                        .flex_col()
                        .rounded(px(13.0))
                        .bg(fill(card_bg()))
                        // harness .head 40px rgba(255,255,255,.3)
                        .child(div().flex_shrink_0().h(px(40.0)).bg(fill(rgba(255, 255, 255, 0.3))))
                        // src/App.css:892-910 .controls: flex, align-items center, overflow hidden until hovered
                        .child(
                            panel
                                .wrap(
                                    28.0,
                                    now,
                                    !*open,
                                    // harness .btns: 100% × 28px rgba(255,255,255,.85)
                                    div().flex_shrink_0().w_full().h(px(28.0)).bg(fill(rgba(255, 255, 255, 0.85))),
                                )
                                .flex()
                                .items_center()
                                .mt(px(layout_unit(margin_top.value(now))))
                                .opacity(opacity.value(now)),
                        ),
                )
                .into_any_element(),

            Scene::NextUp { bounds, opacity, bg, .. } => {
                let b = bounds.clone();
                stage()
                    .child(
                        div()
                            .on_children_prepainted(move |bs, _, _| b.set(bs.first().copied()))
                            .child(
                                // src/App.css:2438-2462 .next-up: flex, center, gap 6px, padding 3px 3px 3px 6px,
                                // radius 6px, width max-content
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .pt(px(3.0))
                                    .pr(px(3.0))
                                    .pb(px(3.0))
                                    .pl(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(fill(bg.value(now)))
                                    .opacity(opacity.value(now))
                                    // src/App.css:2495-2501 .next-up-art 26px, radius 4px (harness fill rgb(200,100,50))
                                    .child(div().flex_shrink_0().size(px(26.0)).rounded(px(4.0)).bg(fill(rgba(200, 100, 50, 1.0))))
                                    // harness .text 40 × 10 rgba(255,255,255,.85)
                                    .child(div().flex_shrink_0().w(px(40.0)).h(px(10.0)).bg(fill(rgba(255, 255, 255, 0.85)))),
                            ),
                    )
                    .into_any_element()
            }

            Scene::BarPulse { start } => {
                // src/App.css:1600-1606 @keyframes bar-pulse
                let k = Keyframes::new(vec![stop(0.0, 3.0f32), stop(0.25, 8.0), stop(0.5, 5.0), stop(0.75, 14.0), stop(1.0, 8.0)]);
                // src/App.css:1580-1587: 1.1s ease-in-out infinite alternate, delays 0 / .18 / .36 / .54 / .72s
                let delays = [0.0, 180.0, 360.0, 540.0, 720.0];
                stage()
                    .child(
                        // src/App.css:1558-1564 .art-mini-bars: flex, align-items flex-end, gap 2px, height 14px
                        div().flex_shrink_0().flex().items_end().gap(px(2.0)).h(px(14.0)).children(delays.map(|d| {
                            let anim = Animation::new(1100.0, Easing::EASE_IN_OUT).infinite().direction(Direction::Alternate).delay(d);
                            // src/App.css:1566-1576 span: 3 × 3px, radius 1.5px, rgba(255,255,255,.85)
                            let h = start.map_or(3.0, |s| k.value_or(&anim, s, now, 3.0));
                            div().flex_shrink_0().w(px(3.0)).h(px(layout_unit(h))).rounded(px(1.5)).bg(fill(rgba(255, 255, 255, 0.85)))
                        })),
                    )
                    .into_any_element()
            }

            Scene::EmptyPulse { start } => {
                // src/App.css:2260-2267 @keyframes empty-pulse: opacity 1 → .4, 1.4s ease-in-out infinite alternate
                let k = Keyframes::new(vec![stop(0.0, 1.0f32), stop(1.0, 0.4)]);
                let anim = Animation::new(1400.0, Easing::EASE_IN_OUT).infinite().direction(Direction::Alternate);
                let o = start.map_or(1.0, |s| k.value_or(&anim, s, now, 1.0));
                // harness stand-in for .np-empty-title: 160 × 16 #fff
                stage().child(div().w(px(160.0)).h(px(16.0)).bg(fill(rgba(255, 255, 255, 1.0))).opacity(o)).into_any_element()
            }

            Scene::FsIn { start } => {
                let Some(s) = *start else { return div().into_any_element() };
                // src/App.css:1700-1716: fs-in 280ms cubic-bezier(0.4, 0, 0.2, 1) forwards, opacity 0 → 1
                let k = Keyframes::new(vec![stop(0.0, 0.0f32), stop(1.0, 1.0)]);
                let anim = Animation::new(280.0, Easing::STANDARD).fill(Fill::Forwards);
                // position: fixed; inset: 0; harness fill rgba(20,20,22,.72)
                div().size_full().bg(fill(card_bg())).opacity(k.value_or(&anim, s, now, 1.0)).into_any_element()
            }

            Scene::NextPopupIn { start } => {
                let Some(s) = *start else { return stage().into_any_element() };
                // src/App.css:2558-2561 @keyframes next-popup-in: opacity 0 → 1, translateX(16px) → none;
                // src/App.css:2504-2533: 320ms cubic-bezier(0.2, 0.8, 0.2, 1) both
                let anim = Animation::new(320.0, Easing::CubicBezier(0.2, 0.8, 0.2, 1.0)).fill(Fill::Both);
                let o = Keyframes::new(vec![stop(0.0, 0.0f32), stop(1.0, 1.0)]).value_or(&anim, s, now, 1.0);
                let x = Keyframes::new(vec![stop(0.0, 16.0f32), stop(1.0, 0.0)]).value_or(&anim, s, now, 0.0);
                stage()
                    .child(translate(
                        point(px(x), px(0.0)),
                        // radius 8px, rgba(255,255,255,.2); 140 × 56 harness stand-in
                        div().w(px(140.0)).h(px(56.0)).rounded(px(8.0)).bg(fill(rgba(255, 255, 255, 0.2))).opacity(o),
                    ))
                    .into_any_element()
            }

            Scene::VolPillIn { start } => {
                let Some(s) = *start else { return stage().into_any_element() };
                // src/App.css:1051-1054 @keyframes vol-pill-in: opacity 0 → 1, translateY(3px) → 0;
                // src/App.css:999-1018: 160ms cubic-bezier(0.4, 0, 0.2, 1) forwards
                let anim = Animation::new(160.0, Easing::STANDARD).fill(Fill::Forwards);
                let o = Keyframes::new(vec![stop(0.0, 0.0f32), stop(1.0, 1.0)]).value_or(&anim, s, now, 1.0);
                let y = Keyframes::new(vec![stop(0.0, 3.0f32), stop(1.0, 0.0)]).value_or(&anim, s, now, 0.0);
                stage()
                    .child(translate(
                        point(px(0.0), px(y)),
                        // radius 8px, rgba(20,20,22,.72); 120 × 36 harness stand-in
                        div().w(px(120.0)).h(px(36.0)).rounded(px(8.0)).bg(fill(card_bg())).opacity(o),
                    ))
                    .into_any_element()
            }
        }
    }
}

/// The still cases of slice B (pages/<name>.html).
fn still(name: &str) -> AnyElement {
    match name {
        // pages/shadow-widget.html: src/App.css:115-131 .widget, box-shadow at the defaults; 200 × 80 harness size
        "shadow-widget" => stage()
            .child(shadowed(
                &[outer(0.0, 0.0, 0.0, 0.5, rgba(255, 255, 255, 0.08)), outer(0.0, 4.0, 16.0, 0.0, rgba(0, 0, 0, 0.45))],
                uniform(13.0),
                div().w(px(200.0)).h(px(80.0)).rounded(px(13.0)).bg(fill(card_bg())),
            ))
            .into_any_element(),
        // pages/shadow-preview.html: src/windows/Settings.css:505-516 .st-preview (radius 10px, fallbacks); 200 × 176
        "shadow-preview" => stage()
            .child(shadowed(
                &[outer(0.0, 0.0, 0.0, 0.5, rgba(255, 255, 255, 0.12)), outer(0.0, 3.0, 10.0, 0.0, rgba(0, 0, 0, 0.45))],
                uniform(10.0),
                div().w(px(200.0)).h(px(176.0)).rounded(px(10.0)).bg(fill(card_bg())),
            ))
            .into_any_element(),
        // pages/shadow-vol-pill.html: src/App.css:999-1018 .vol-pill (172px wide, radius 8px); 28px harness height
        "shadow-vol-pill" => stage()
            .child(shadowed(
                &[outer(0.0, 0.0, 0.0, 0.5, rgba(255, 255, 255, 0.12)), outer(0.0, 6.0, 18.0, 0.0, rgba(0, 0, 0, 0.4))],
                uniform(8.0),
                div().w(px(172.0)).h(px(28.0)).rounded(px(8.0)).bg(fill(card_bg())),
            ))
            .into_any_element(),
        // pages/shadow-hairline-pill.html: column, gap 8px
        "shadow-hairline-pill" => {
            let boxed = |w: f32, h: f32, r: f32, bg: Rgba, edge: Rgba| {
                div().relative().flex_shrink_0().w(px(w)).h(px(h)).rounded(px(r)).bg(fill(bg)).child(inset(&[inset_shadow(0.0, 0.0, 0.0, 0.5, edge)], uniform(r)))
            };
            stage()
                .gap(px(8.0))
                // src/windows/Settings.css:429-444 .st-reset-pill (96 × 17.75 harness size)
                .child(boxed(96.0, 17.75, 999.0, rgba(255, 90, 70, 0.28), rgba(255, 120, 100, 0.5)))
                // src/windows/Settings.css:472-476 .st-reset-done .st-reset-pill
                .child(boxed(96.0, 17.75, 999.0, rgba(70, 200, 120, 0.55), rgba(120, 230, 160, 0.6)))
                // src/windows/Settings.css:298-306 .st-info-tile (120 × 40 harness size)
                .child(boxed(120.0, 40.0, 10.0, rgba(255, 255, 255, 0.06), rgba(255, 255, 255, 0.08)))
                .into_any_element()
        }
        // pages/rounded-widget.html
        "rounded-widget" => {
            // src/App.css:115-131 .widget: 200px content-box (+ 2 × 10 padding), padding 10px 10px 8px,
            // radius 13px, overflow hidden; height = 10 + 44 (.top) + 8 + 24 (.foot) − 8 + 8 = 86
            let (w, h, r) = (220.0, 86.0, uniform(13.0));
            let place = |child: Rect| clip_child(w, h, r, child).expect("rounded-widget: representable child");
            // src/App.css:138-148 .widget-bg: inset -60px, opacity .55; harness fill rgb(200, 100, 50)
            let (bg_rect, bg_r) = place(Rect::new(-60.0, -60.0, w + 120.0, h + 120.0));
            // harness .foot: 24px, margin 0 -10px -8px (flush to the bottom corners), rgba(255,255,255,.5)
            let (foot_rect, foot_r) = place(Rect::new(0.0, h - 24.0, w, 24.0));
            // src/App.css:178-190 .art-wrap 44px, radius 6px, overflow hidden; src/App.css:205-210 .art 100%
            let (art_rect, art_r) = clip_child(44.0, 44.0, uniform(6.0), Rect::new(0.0, 0.0, 44.0, 44.0)).expect("art");
            stage()
                .child(
                    div()
                        .relative()
                        .w(px(w))
                        .h(px(h))
                        .pt(px(10.0))
                        .px(px(10.0))
                        .pb(px(8.0))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .rounded(px(13.0))
                        .bg(fill(card_bg()))
                        .child(rounded_fill(bg_rect, bg_r, rgba(200, 100, 50, 1.0)).opacity(0.55))
                        // src/App.css:166-176 .top: flex, center, gap 8px, margin-bottom 8px
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .mb(px(8.0))
                                .child(
                                    div().relative().flex_shrink_0().size(px(44.0)).overflow_hidden().rounded(px(6.0)).child(
                                        img(ImageSource::Render(art_44()))
                                            .absolute()
                                            .left(px(art_rect.x))
                                            .top(px(art_rect.y))
                                            .w(px(art_rect.w))
                                            .h(px(art_rect.h))
                                            .rounded_tl(px(art_r[0]))
                                            .rounded_tr(px(art_r[1]))
                                            .rounded_br(px(art_r[2]))
                                            .rounded_bl(px(art_r[3])),
                                    ),
                                )
                                // harness .info: flex 1, 20px, rgba(255,255,255,.3)
                                .child(div().flex_1().h(px(20.0)).bg(fill(rgba(255, 255, 255, 0.3)))),
                        )
                        .child(rounded_fill(foot_rect, foot_r, rgba(255, 255, 255, 0.5))),
                )
                .into_any_element()
        }
        // pages/mask-lyrics.html: src/App.css:268-293 .lyrics-panel open at 300px (220 wide) with its mask-image;
        // harness rows: 10 × 30px, odd rgba(255,255,255,.85), even rgb(200,100,50)
        "mask-lyrics" => stage()
            .child(masked(LYRICS_PANEL, || {
                div()
                    .flex_shrink_0()
                    .w(px(220.0))
                    .h(px(300.0))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .children((0..10).map(|i| {
                        let c = if i % 2 == 0 { rgba(255, 255, 255, 0.85) } else { rgba(200, 100, 50, 1.0) };
                        div().flex_shrink_0().w_full().h(px(30.0)).bg(fill(c))
                    }))
                    .into_any_element()
            }))
            .into_any_element(),
        // pages/scale-pill-085.html, -094, -110: the reset pill held at a scale
        // (src/windows/Settings.css:482-485 st-reset-in from .85; :461-464 holding .94; :487-491 st-reset-pop 45% 1.1)
        "scale-pill-085" | "scale-pill-094" | "scale-pill-110" => {
            let s = match name {
                "scale-pill-085" => 0.85,
                "scale-pill-094" => 0.94,
                _ => 1.1,
            };
            stage().child(scale(s, 1.0, reset_pill(PILL_BG, PILL_EDGE, stand_in()))).into_any_element()
        }
        // pages/scale-pill-text.html: holding (.94) with its text (src/windows/Settings.tsx, .st-reset-text)
        "scale-pill-text" => {
            // src/windows/Settings.css:16 .st-card font stack ("SF Pro" is not installed: SF Pro Text);
            // :429-444 11px 600 #fff nowrap; :406-414 line-height 1.25; .st-reset-text relative
            let mut f = gpui::font("SF Pro Text");
            f.weight = FontWeight::SEMIBOLD;
            let label = scale::text(div().relative().flex_shrink_0(), "Keep holding\u{2026}", f, 11.0, 11.0 * 1.25, rgba(255, 255, 255, 1.0));
            stage().child(scale(0.94, 1.0, reset_pill(PILL_BG, PILL_EDGE, label))).into_any_element()
        }
        _ => unreachable!("no still {name}"),
    }
}

// src/windows/Settings.css:429-444 .st-reset-pill, :472-476 .st-reset-done .st-reset-pill
const PILL_BG: Rgba = Rgba { r: 255.0 / 255.0, g: 90.0 / 255.0, b: 70.0 / 255.0, a: 0.28 };
const PILL_EDGE: Rgba = Rgba { r: 255.0 / 255.0, g: 120.0 / 255.0, b: 100.0 / 255.0, a: 0.5 };
const PILL_DONE_BG: Rgba = Rgba { r: 70.0 / 255.0, g: 200.0 / 255.0, b: 120.0 / 255.0, a: 0.55 };
const PILL_DONE_EDGE: Rgba = Rgba { r: 120.0 / 255.0, g: 230.0 / 255.0, b: 160.0 / 255.0, a: 0.6 };

/// src/windows/Settings.css:429-444 .st-reset-pill as a `css::scale` node: padding 2px 10px, radius 999px,
/// inset 0 0 0 .5px hairline, overflow hidden (its content stays inside the padding).
fn reset_pill(bg: Rgba, edge: Rgba, content: scale::Node) -> scale::Node {
    scale::node(div().relative().flex_shrink_0().px(px(10.0)).py(px(2.0)))
        .bg(bg)
        .radii(uniform(999.0))
        .shadows(&[inset_shadow(0.0, 0.0, 0.0, 0.5, edge)])
        .child(content)
}

/// pages/scale-pill-*.html harness stand-in for the text: 76 × 13.75, rgba(255,255,255,.85)
fn stand_in() -> scale::Node {
    scale::node(div().flex_shrink_0().w(px(76.0)).h(px(13.75))).bg(rgba(255, 255, 255, 0.85))
}

/// A fill at `rect` (parent coordinates, absolute) with the radii `rounded::clip_child` gave it.
fn rounded_fill(rect: Rect, r: Radii, c: Rgba) -> gpui::Div {
    div()
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.w))
        .h(px(rect.h))
        .rounded_tl(px(r[0]))
        .rounded_tr(px(r[1]))
        .rounded_br(px(r[2]))
        .rounded_bl(px(r[3]))
        .bg(fill(c))
}

/// scripts/primitives/pages/art-44.png (the harness cover) as a GPUI image (BGRA, port.md 7.13).
fn art_44() -> std::sync::Arc<gpui::RenderImage> {
    thread_local! {
        static ART: std::sync::Arc<gpui::RenderImage> = {
            let dec = png::Decoder::new(std::io::Cursor::new(include_bytes!("../../scripts/primitives/pages/art-44.png").as_slice()));
            let mut r = dec.read_info().expect("art-44.png");
            let mut buf = vec![0; r.output_buffer_size()];
            let info = r.next_frame(&mut buf).expect("art-44.png frame");
            assert_eq!(info.color_type, png::ColorType::Rgba);
            buf.truncate(info.buffer_size());
            for p in buf.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            let im = image::RgbaImage::from_raw(info.width, info.height, buf).expect("art size");
            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(im)]))
        };
    }
    ART.with(|a| a.clone())
}
