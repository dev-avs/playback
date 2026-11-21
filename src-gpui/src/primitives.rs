//! The primitive cases of the CSS layer (parity/plan.md Phase 4): each scene draws what
//! `scripts/primitives/pages/<case>.html` draws, with the `css/` helpers, so `--primitive <case>` can be
//! diffed against the Electron capture. The cases (size, frame count, timeline) come from
//! `scripts/primitives/cases.json`, the same file the Electron runner reads.
//!
//! Every value is the page's (the page cites the App.css line it was taken from); the stand-in boxes
//! (sizes, fills marked "harness" in the page) are the page's own.

use crate::css::{
    layout_unit, stop, translate, Animation, ClipHeight, Direction, Easing, Fill, HoverProp, Hover, Keyframes,
    Transition, Tween,
};
use gpui::{div, point, prelude::*, px, AnyElement, Bounds, Hsla, Pixels, Point, Rgba};
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
    Lyrics { open: bool, panel: ClipHeight, margin_bottom: Tween<f32> },
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
            "clip-lyrics" => Scene::Lyrics {
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
            Scene::Lyrics { open, panel, margin_bottom } => {
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
            Scene::NextPopupIn { start } | Scene::VolPillIn { start } if add == Some("show") => *start = Some(now),
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
            Scene::Card { bg, opacity, .. } => stage()
                .child(
                    // src/App.css:115-131 .widget: radius 13px; 200 × 80 harness stand-in for the content
                    div().w(px(200.0)).h(px(80.0)).rounded(px(13.0)).bg(fill(bg.value(now))).opacity(opacity.value(now)),
                )
                .into_any_element(),

            Scene::Lyrics { panel, margin_bottom, .. } => {
                // pages/clip-lyrics.html: 8 rows of 30px, odd .18 / even .06 white
                let rows = div().flex_shrink_0().w_full().flex().flex_col().children((0..8).map(|i| {
                    let a = if i % 2 == 0 { 0.18 } else { 0.06 };
                    div().flex_shrink_0().w_full().h(px(30.0)).bg(fill(rgba(255, 255, 255, a)))
                }));
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
                            // src/App.css:268-293 .lyrics-panel: margin 0 -10px, margin-bottom transitions, overflow hidden
                            .child(
                                panel
                                    .wrap(240.0, now, true, rows)
                                    .mx(px(-10.0))
                                    .mb(px(layout_unit(margin_bottom.value(now)))),
                            )
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
