//! `--snapshot <scene> <out.png>`: render a scene off-screen and write its pixels, transparent stage
//! included, to a PNG of the window's drawn size (the size Electron's `capturePage` gives).
//!
//! PrintWindow draws a transparent pixel as black, so alpha is recovered from two captures of the same
//! frame: once as it is (colour premultiplied over black, B) and once with an opaque white backdrop
//! behind it (colour over white, W): a = 1 - mean(W - B) / 255 and colour = B / a.

use super::widget::{open_widget, Opts, ResizeMode};
use super::{sleep, wait_until, with_widget};
use crate::{instance, win32};
use gpui::{App, Hsla};
use std::path::PathBuf;

/// Scenes slice 3a can draw. The real ones arrive with the screens (parity/plan.md).
pub const SCENES: &[&str] = &["widget"];

pub(crate) fn crop(hwnd: isize) -> Option<(i32, i32, Vec<u8>)> {
    let (w, _h, px) = win32::capture_bgra(hwnd)?;
    let win = win32::window_rect(hwnd);
    let vis = win32::visible_rect(hwnd);
    let (ox, oy) = (vis.x - win.x, vis.y - win.y);
    let mut out = Vec::with_capacity((vis.w * vis.h * 4) as usize);
    for y in 0..vis.h {
        let row = (((oy + y) * w + ox) * 4) as usize;
        out.extend_from_slice(&px[row..row + (vis.w * 4) as usize]);
    }
    Some((vis.w, vis.h, out))
}

/// RGBA (straight alpha) from the black (`b`) and white-backdrop (`wh`) BGRA captures of one frame.
pub(crate) fn recover_alpha(w: i32, h: i32, b: &[u8], wh: &[u8]) -> Vec<u8> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..(w * h) as usize {
        let (bp, wp) = (&b[i * 4..i * 4 + 4], &wh[i * 4..i * 4 + 4]);
        let d: f32 = (0..3).map(|c| (wp[c] as f32 - bp[c] as f32).max(0.0)).sum::<f32>() / 3.0;
        let a = (1.0 - d / 255.0).clamp(0.0, 1.0);
        let o = &mut rgba[i * 4..i * 4 + 4];
        if a > 0.0 {
            // BGRA -> RGBA, un-premultiplied
            o[0] = (bp[2] as f32 / a).round().min(255.0) as u8;
            o[1] = (bp[1] as f32 / a).round().min(255.0) as u8;
            o[2] = (bp[0] as f32 / a).round().min(255.0) as u8;
        }
        o[3] = (a * 255.0).round() as u8;
    }
    rgba
}

pub(crate) fn write_png(out: &std::path::Path, w: i32, h: i32, rgba: &[u8]) {
    let file = std::fs::File::create(out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(rgba)).expect("write png");
}

pub fn run(cx: &mut App, scene: String, out: PathBuf, tag: &str) {
    if !SCENES.contains(&scene.as_str()) {
        eprintln!("snapshot: unknown scene {scene:?} (known: {})", SCENES.join(", "));
        std::process::exit(2);
    }
    let wake = match instance::acquire(tag) {
        instance::Instance::First(rx) => Some(rx),
        instance::Instance::Second => None,
    };
    let (handle, shell, _driver) = open_widget(cx, Opts { test: true, wake, resize_mode: ResizeMode::Shell });
    shell.cursor.set_fake(Some((-1, -1)));
    cx.spawn(async move |cx| {
        let ok = wait_until(cx, 5000, || shell.trace().uncloaked_at.is_some()).await;
        let mut code = 0;
        if !ok {
            eprintln!("snapshot: window never uncloaked");
            code = 1;
        } else {
            sleep(cx, 800).await;
            let black = crop(shell.hwnd);
            with_widget(cx, handle, |w, cx| {
                w.backdrop = Some(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 1.0 });
                cx.notify();
            });
            sleep(cx, 250).await;
            let white = crop(shell.hwnd);
            match (black, white) {
                (Some((w, h, b)), Some((w2, h2, wh))) if (w, h) == (w2, h2) => {
                    write_png(&out, w, h, &recover_alpha(w, h, &b, &wh));
                    println!("snapshot {scene}: {}x{} -> {}", w, h, out.display());
                }
                _ => {
                    eprintln!("snapshot: capture failed");
                    code = 1;
                }
            }
        }
        if code != 0 {
            std::process::exit(code);
        }
        let _ = cx.update(|app| app.quit());
    })
    .detach();
}
