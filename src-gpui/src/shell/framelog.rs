//! `--frame-log <file> [--resize off|shell|every-frame]`: the frame-driver proof (port.md 2.5).
//! A 350 ms tween of the placeholder card runs twice (grow, then shrink) and the paint instants of the
//! window are written to the file and summarised. `--resize` picks what the window does meanwhile:
//! `off` = no window resize, `shell` = the card-measure resize of port.md 7.8 (the real behaviour),
//! `every-frame` = the window follows the card on every frame (worst case).

use super::widget::{open_widget, Opts, ResizeMode, TWEEN};
use super::{sleep, wait_until, with_widget};
use crate::{frame_driver::FrameLog, instance};
use gpui::App;
use std::{cell::RefCell, fmt::Write as _, path::PathBuf, rc::Rc, time::Instant};

pub struct Summary {
    pub label: &'static str,
    pub frames: usize,
    pub avg_fps: f64,
    pub min_fps: f64,
    pub max_gap_ms: f64,
    pub stalls: u32,
}

/// Frames painted in [t0, t0 + TWEEN]; gaps run from t0 (the tween start) to the first paint after the end.
pub fn summarize(label: &'static str, log: &FrameLog, t0: Instant, text: &mut String) -> Summary {
    let end = t0 + TWEEN;
    let mut gaps = Vec::new();
    let mut prev: Option<Instant> = None;
    let mut n = 0;
    let mut closed = false;
    for (t, anim) in &log.frames {
        if *t < t0 {
            continue;
        }
        if closed {
            break;
        }
        let from = prev.unwrap_or(t0);
        let gap = t.duration_since(from).as_secs_f64() * 1000.0;
        let _ = writeln!(text, "{label} t={:8.2} gap={:6.2} animating={}", t.duration_since(t0).as_secs_f64() * 1000.0, gap, anim);
        if *t <= end {
            n += 1;
            gaps.push(gap);
        } else {
            // the first paint after the end closes the last interval inside the tween
            gaps.push(gap);
            closed = true;
        }
        prev = Some(*t);
    }
    let max_gap = gaps.iter().cloned().fold(0.0, f64::max);
    Summary {
        label,
        frames: n,
        avg_fps: n as f64 / TWEEN.as_secs_f64(),
        min_fps: if max_gap > 0.0 { 1000.0 / max_gap } else { 0.0 },
        max_gap_ms: max_gap,
        stalls: log.stalls,
    }
}

pub fn run(cx: &mut App, path: PathBuf, mode: ResizeMode, tag: &str) {
    let wake = match instance::acquire(tag) {
        instance::Instance::First(rx) => Some(rx),
        instance::Instance::Second => None,
    };
    let (handle, shell, driver) = open_widget(cx, Opts { test: true, wake, resize_mode: mode });
    shell.cursor.set_fake(Some((-1, -1)));
    let out: Rc<RefCell<Vec<Summary>>> = Rc::default();
    cx.spawn(async move |cx| {
        let ok = wait_until(cx, 5000, || shell.trace().uncloaked_at.is_some()).await;
        if !ok {
            eprintln!("frame-log: window never uncloaked");
            let _ = cx.update(|app| app.quit());
            return;
        }
        // let the first layout and the initial shrink debounce settle
        sleep(cx, 800).await;
        let mut text = String::new();
        let mut sums = Vec::new();
        for (label, to) in [("grow", 500.0_f32), ("shrink", 300.0_f32)] {
            let log = driver.log();
            let t0 = Instant::now();
            with_widget(cx, handle, |w, cx| w.start_tween(to, cx));
            sleep(cx, 350 + 400).await;
            sums.push(summarize(label, &log.borrow(), t0, &mut text));
        }
        let _ = std::fs::write(&path, text);
        *out.borrow_mut() = sums;
        for s in out.borrow().iter() {
            println!(
                "frame-log {:?} {}: frames={} avg_fps={:.1} min_fps={:.1} max_gap_ms={:.1} stalls={}",
                mode, s.label, s.frames, s.avg_fps, s.min_fps, s.max_gap_ms, s.stalls
            );
        }
        let _ = cx.update(|app| app.quit());
    })
    .detach();
}
