//! `--primitive <case> <out.png|outDir>`: draw one primitive case of the CSS layer off-screen on a
//! virtual clock and write each frame as a PNG, for `scripts/primitives/diff.mjs` to compare with the
//! Electron capture (parity/plan.md Phase 4).
//!
//! The snapshot path of slice 3a: a frameless transparent window at `TEST_ORIGIN`, cloaked until it has
//! painted, PrintWindow captures, alpha recovered from a black capture and a white-backdrop capture of
//! the same frame (`snapshot::recover_alpha`). Time never comes from the wall clock: frame k is drawn at
//! `primitives::frame_time(k)` and both captures of a frame are of that same instant. Timeline ops run
//! at the frame time, as in scripts/primitives/main.cjs; `move` / `out` set the injected cursor
//! (`widget::Cursor`, never the real mouse) and run one cursor-poll tick (port.md 7.6).

use super::snapshot::{crop, recover_alpha, write_png};
use super::sleep;
use super::widget::{Cursor, TEST_ORIGIN};
use crate::primitives::{cases, frame_time, Scene};
use crate::win32::{self, Rect};
use gpui::{
    div, point, prelude::*, px, size, App, AsyncApp, Bounds, Context, Hsla, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

struct Prim {
    scene: Scene,
    now: f64,
    backdrop: Option<Hsla>,
    renders: Rc<Cell<u64>>,
}

impl Render for Prim {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().when_some(self.backdrop, |d, c| d.bg(c)).child(self.scene.render(self.now))
    }
}

fn update<R>(cx: &AsyncApp, h: WindowHandle<Prim>, f: impl FnOnce(&mut Prim, &mut Context<Prim>) -> R) -> Option<R> {
    cx.update(|app| h.update(app, |p, _, cx| f(p, cx)).ok()).ok().flatten()
}

/// Redraw with the view as `f` leaves it, and wait until that frame is on the window.
async fn redraw(cx: &AsyncApp, h: WindowHandle<Prim>, renders: &Rc<Cell<u64>>, f: impl FnOnce(&mut Prim)) -> bool {
    let before = renders.get();
    update(cx, h, |p, cx| {
        f(p);
        cx.notify();
    });
    let mut waited = 0;
    while renders.get() == before {
        if waited > 3000 {
            return false;
        }
        sleep(cx, 5).await;
        waited += 5;
    }
    // presented and composed by DWM before PrintWindow reads it
    sleep(cx, 80).await;
    true
}

/// Windows keeps a window at least its minimum tracking width (~120 px here, wider than kf-bar-pulse's
/// 67): the scene is laid out from the top-left, so the case's area is the capture's top-left corner.
fn top_left((cw, ch, px): (i32, i32, Vec<u8>), w: i32, h: i32) -> Option<(i32, i32, Vec<u8>)> {
    if cw < w || ch < h {
        return None;
    }
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row = ((y * cw) * 4) as usize;
        out.extend_from_slice(&px[row..row + (w * 4) as usize]);
    }
    Some((w, h, out))
}

fn fail(msg: String) -> ! {
    eprintln!("primitive: {msg}");
    std::process::exit(1);
}

pub fn run(cx: &mut App, name: String, out: PathBuf) {
    let all = cases();
    let Some(case) = all.iter().find(|c| c.name == name).cloned() else {
        eprintln!("primitive: unknown case {name:?} (known: {})", all.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
        std::process::exit(2);
    };
    crate::css::text::load_fonts(cx).unwrap_or_else(|e| fail(format!("fonts: {e}")));
    let scene = Scene::new(&name).unwrap_or_else(|| fail(format!("no scene for case {name}")));
    let (w, h) = (case.size[0], case.size[1]);
    let renders = Rc::new(Cell::new(0u64));
    let r2 = renders.clone();
    let hwnd = Rc::new(Cell::new(0isize));
    let hw2 = hwnd.clone();
    // GPUI refuses bounds that are not on a monitor; the window is moved off-screen with Win32 while hidden
    let bounds = Bounds { origin: point(px(100.0), px(100.0)), size: size(px(w as f32), px(h as f32)) };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                focus: false,
                show: false,
                kind: WindowKind::Normal,
                ..Default::default()
            },
            move |window, cx| {
                hw2.set(win32::hwnd_of(window));
                cx.new(|_| Prim { scene, now: 0.0, backdrop: None, renders: r2 })
            },
        )
        .expect("open primitive window");
    let hwnd = hwnd.get();
    if win32::dpi(hwnd) != 96 {
        fail(format!("display scale must be 100% (dpi {}): the Electron captures are 1 px per DIP", win32::dpi(hwnd)));
    }
    // port.md 7.7 / 7.5: cloaked while it first paints, no DWM frame or corners
    win32::set_cloak(hwnd, true);
    win32::dwm_strip(hwnd);
    win32::set_visible_rect(hwnd, Rect { x: TEST_ORIGIN.0, y: TEST_ORIGIN.1, w, h }, true, true);
    win32::show_no_activate(hwnd);

    let cursor = Cursor::default();
    cursor.set_fake(Some((TEST_ORIGIN.0 - 1000, TEST_ORIGIN.1 - 1000)));
    cx.spawn(async move |cx| {
        for _ in 0..3 {
            if !redraw(cx, handle, &renders, |_| {}).await {
                fail("the window never painted".into());
            }
        }
        win32::set_cloak(hwnd, false);
        sleep(cx, 200).await;

        let count = case.frames.max(1);
        if case.frames > 0 {
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir_all(&out).expect("create out dir");
        }
        let mut ops = case.timeline.clone();
        ops.sort_by(|a, b| a.t.total_cmp(&b.t));
        let mut next = 0;
        for k in 0..count {
            let t = frame_time(k);
            while next < ops.len() && ops[next].t <= t + 1e-6 {
                let op = &ops[next];
                next += 1;
                match op.op.as_str() {
                    "class" => {
                        update(cx, handle, |p, _| p.scene.class(op.add.as_deref(), op.remove.as_deref(), t));
                    }
                    "move" | "out" => {
                        let pos = if op.op == "move" {
                            let vis = win32::visible_rect(hwnd);
                            (vis.x + op.x.unwrap_or(0), vis.y + op.y.unwrap_or(0))
                        } else {
                            (TEST_ORIGIN.0 - 1000, TEST_ORIGIN.1 - 1000)
                        };
                        cursor.set_fake(Some(pos));
                        // the cursor poll of widget::Widget::poll: window coordinates, None outside the drawn rect
                        let (x, y) = cursor.pos();
                        let vis = win32::visible_rect(hwnd);
                        let inside = vis.contains(x, y).then(|| point(px((x - vis.x) as f32), px((y - vis.y) as f32)));
                        update(cx, handle, |p, _| p.scene.poll_cursor(inside, t));
                    }
                    other => fail(format!("unknown op {other}")),
                }
            }
            if !redraw(cx, handle, &renders, |p| {
                p.now = t;
                p.backdrop = None;
            })
            .await
            {
                fail(format!("frame {k} never painted"));
            }
            let black = crop(hwnd).and_then(|c| top_left(c, w, h));
            if !redraw(cx, handle, &renders, |p| p.backdrop = Some(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 1.0 })).await {
                fail(format!("frame {k} (white) never painted"));
            }
            let white = crop(hwnd).and_then(|c| top_left(c, w, h));
            match (black, white) {
                (Some((bw, bh, b)), Some((ww, wh, wt))) if (bw, bh) == (ww, wh) && (bw, bh) == (w, h) => {
                    let file = if case.frames > 0 { out.join(format!("f{k:03}.png")) } else { out.clone() };
                    write_png(&file, w, h, &recover_alpha(w, h, &b, &wt));
                }
                (b, wt) => fail(format!(
                    "frame {k}: capture failed or wrong size ({:?} / {:?}, want {w}x{h})",
                    b.map(|x| (x.0, x.1)),
                    wt.map(|x| (x.0, x.1))
                )),
            }
        }
        println!("primitive {name}: {count} frame(s) {w}x{h} -> {}", out.display());
        let _ = cx.update(|app| app.quit());
    })
    .detach();
}
