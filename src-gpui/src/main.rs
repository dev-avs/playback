#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod lyrics;
mod media;
mod settings;
mod settings_view;

use gpui::{
    div, img, prelude::*, px, rgb, rgba, size, App, Application, Bounds, Context, FontWeight,
    ImageSource, RenderImage, SharedString, Window, WindowBackgroundAppearance, WindowBounds,
    WindowOptions,
};
use image::{Frame, RgbaImage};
use lyrics::{active_line_index, fetch_lyrics, LyricsResult};
use media::{format_time, poll_smtc, send_action, MediaAction, NowPlaying};
use settings::Settings;
use settings_view::SettingsView;
use std::borrow::Cow;
use std::sync::{Arc, RwLock};
use std::time::Duration;

static FONT_REGULAR: &[u8] = include_bytes!("../../src/fonts/spicy-lyrics/LyricsRegular.woff2");
static FONT_MEDIUM: &[u8] = include_bytes!("../../src/fonts/spicy-lyrics/LyricsMedium.woff2");
static FONT_SEMIBOLD: &[u8] = include_bytes!("../../src/fonts/spicy-lyrics/LyricsSemibold.woff2");
static FONT_BOLD: &[u8] = include_bytes!("../../src/fonts/spicy-lyrics/LyricsBold.woff2");

struct SharedState {
    now: RwLock<NowPlaying>,
    lyrics: RwLock<Option<LyricsResult>>,
    settings: Arc<RwLock<Settings>>,
}

struct PlaybackWidget {
    state: Arc<SharedState>,
    lyrics_open: bool,
    last_art_key: String,
    art_image: Option<Arc<RenderImage>>,
}

impl PlaybackWidget {
    fn new(state: Arc<SharedState>, cx: &mut Context<Self>) -> Self {
        let lyrics_open = state
            .settings
            .read()
            .map(|s| s.lyrics_on_launch)
            .unwrap_or(false);

        // Refresh UI at ~15 fps for smooth playback timer and lyric line tracking.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(66))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();

        Self {
            state,
            lyrics_open,
            last_art_key: String::new(),
            art_image: None,
        }
    }

    fn sync_art(&mut self, now: &NowPlaying) {
        let key = now.track_key();
        if key == self.last_art_key {
            return;
        }
        if let Some((w, h, ref bgra)) = now.art_rgba {
            if let Some(buf) = RgbaImage::from_raw(w, h, bgra.clone()) {
                let frame = Frame::new(buf);
                self.art_image = Some(Arc::new(RenderImage::new(vec![frame])));
                self.last_art_key = key;
            }
        } else if key.is_empty() || key == "|" {
            self.art_image = None;
            self.last_art_key = key;
        }
    }

    fn open_settings(&self, cx: &mut Context<Self>) {
        let shared = self.state.settings.clone();
        let bounds = Bounds::centered(None, size(px(580.0), px(460.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| cx.new(|_| SettingsView::new(shared)),
        );
    }
}

fn rgba_u32(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
}

fn font_weight_of(name: &str) -> FontWeight {
    match name {
        "normal" => FontWeight::NORMAL,
        "semibold" => FontWeight::SEMIBOLD,
        "heavy" => FontWeight::EXTRA_BOLD,
        _ => FontWeight::BOLD,
    }
}

impl Render for PlaybackWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.state.now.read().map(|n| n.clone()).unwrap_or_default();
        self.sync_art(&now);
        let lyrics = self.state.lyrics.read().ok().and_then(|l| l.clone());
        let set = self
            .state
            .settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default();

        let pos = now.current_position();
        let dur = now.duration;
        let pct = if dur > 0.1 {
            (pos / dur).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let (gr, gg, gb) = set.glass_rgb();
        let (ar, ag, ab) = now.accent_rgb;
        // Blend glass tint with ambient album art accent.
        let bg_r = ((gr as u16 * 3 + ar as u16) / 4) as u8;
        let bg_g = ((gg as u16 * 3 + ag as u16) / 4) as u8;
        let bg_b = ((gb as u16 * 3 + ab as u16) / 4) as u8;
        let bg_a = ((set.bg_alpha.clamp(20.0, 100.0) / 100.0) * 245.0) as u8;

        let highlight_hex = set.highlight_rgb();
        let bar_color = if set.tint_bar {
            highlight_hex
        } else {
            0xffffff
        };
        let cover_radius = match set.cover_shape.as_str() {
            "square" => px(2.0),
            "circle" => px(999.0),
            _ => px(8.0),
        };

        let title_text = if now.title.is_empty() {
            "Nothing playing".to_string()
        } else {
            now.title.clone()
        };
        let artist_text = if now.artist.is_empty() {
            "Play music in Spotify or any Windows app".to_string()
        } else {
            now.artist.clone()
        };

        let right_time = if set.show_total_time {
            format_time(dur)
        } else {
            format!("-{}", format_time((dur - pos).max(0.0)))
        };

        let footer_source = match (&now.app_name[..], lyrics.as_ref()) {
            ("", _) => "GPUI".to_string(),
            (app, Some(l)) if set.show_lyric_source => format!("{app} · {}", l.source),
            (app, _) => app.to_string(),
        };

        let active_idx = lyrics
            .as_ref()
            .filter(|l| l.synced)
            .and_then(|l| active_line_index(&l.lines, pos + set.lyric_offset));

        let lyric_weight = font_weight_of(&set.lyric_weight);
        let centered = set.lyric_align == "center";

        let _ = window;

        div()
            .size_full()
            .p(px(10.0))
            .bg(rgba(0x00000000))
            .font_family("SpicyLyrics")
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .p(px(14.0))
                    .rounded(px(set.corners))
                    .bg(rgba(rgba_u32(bg_r, bg_g, bg_b, bg_a)))
                    .when(set.edge_highlight, |d| {
                        d.border_1().border_color(rgba(0xffffff22))
                    })
                    .text_color(rgb(0xf5f5f7))
                    // Header (click-drag moves the window)
                    .child(
                        div()
                            .id("widget-header")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(11.0))
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            })
                            .when(set.show_cover, |d| {
                                d.child(
                                    div()
                                        .size(px(44.0))
                                        .flex_none()
                                        .rounded(cover_radius)
                                        .overflow_hidden()
                                        .bg(rgba(0xffffff14))
                                        .border_1()
                                        .border_color(rgba(0xffffff1f))
                                        .when_some(self.art_image.clone(), |box_el, image| {
                                            box_el
                                                .child(img(ImageSource::Render(image)).size_full())
                                        }),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0 * set.title_scale))
                                            .font_weight(FontWeight::BOLD)
                                            .truncate()
                                            .child(title_text),
                                    )
                                    .when(set.show_artist, |d| {
                                        d.child(
                                            div()
                                                .text_size(px(12.0 * set.title_scale))
                                                .text_color(rgba(0xffffff99))
                                                .truncate()
                                                .child(artist_text),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .id("btn-lyrics")
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded(px(8.0))
                                            .bg(if self.lyrics_open {
                                                rgba(0xffffff2a)
                                            } else {
                                                rgba(0xffffff12)
                                            })
                                            .hover(|s| s.bg(rgba(0xffffff2e)))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.lyrics_open = !this.lyrics_open;
                                                let h = if this.lyrics_open {
                                                    let ph = this
                                                        .state
                                                        .settings
                                                        .read()
                                                        .map(|s| s.panel_height)
                                                        .unwrap_or(300.0);
                                                    158.0 + ph
                                                } else {
                                                    158.0
                                                };
                                                let w = this
                                                    .state
                                                    .settings
                                                    .read()
                                                    .map(|s| s.widget_width + 44.0)
                                                    .unwrap_or(394.0);
                                                window.resize(size(px(w), px(h)));
                                                cx.notify();
                                            }))
                                            .child("Lyrics"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-settings")
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded(px(8.0))
                                            .bg(rgba(0xffffff12))
                                            .hover(|s| s.bg(rgba(0xffffff28)))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_settings(cx);
                                            }))
                                            .child("⚙"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-close")
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded(px(8.0))
                                            .bg(rgba(0xffffff12))
                                            .hover(|s| s.bg(rgb(0xff453a)))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.0))
                                            .cursor_pointer()
                                            .on_click(|_, _, cx| cx.quit())
                                            .child("×"),
                                    ),
                            ),
                    )
                    // Synced lyrics list (when expanded)
                    .when(self.lyrics_open, |d| {
                        let content: Vec<gpui::AnyElement> = if let Some(ref res) = lyrics {
                            let focus = active_idx.unwrap_or(0);
                            let start = focus.saturating_sub(1);
                            let end = (start + 6).min(res.lines.len());
                            res.lines[start..end]
                                .iter()
                                .enumerate()
                                .map(|(offset, line)| {
                                    let idx = start + offset;
                                    let is_active = active_idx == Some(idx);
                                    let is_past = active_idx.is_some_and(|a| idx < a);
                                    let line_time = line.time;
                                    let click_jump = set.click_to_jump && res.synced;
                                    let text = if set.all_caps {
                                        line.text.to_uppercase()
                                    } else {
                                        line.text.clone()
                                    };
                                    div()
                                        .id(SharedString::from(format!("lyric-{idx}")))
                                        .text_size(px(set.lyric_size))
                                        .font_weight(lyric_weight)
                                        .when(centered, |el| el.text_center())
                                        .text_color(if is_active {
                                            rgb(highlight_hex)
                                        } else if is_past {
                                            rgba(0xffffff66)
                                        } else {
                                            rgba(0xffffff4c)
                                        })
                                        .when(click_jump, |el| {
                                            el.cursor_pointer()
                                                .hover(|s| s.text_color(rgb(0xffffff)))
                                                .on_click(move |_, _, _| {
                                                    std::thread::spawn(move || {
                                                        send_action(MediaAction::Seek(line_time))
                                                    });
                                                })
                                        })
                                        .child(if text.is_empty() {
                                            "•••".to_string()
                                        } else {
                                            text
                                        })
                                        .into_any_element()
                                })
                                .collect()
                        } else {
                            vec![div()
                                .text_size(px(14.0))
                                .text_color(rgba(0xffffff66))
                                .child("No synced lyrics found for this track")
                                .into_any_element()]
                        };

                        d.child(
                            div()
                                .flex_1()
                                .my(px(12.0))
                                .flex()
                                .flex_col()
                                .justify_center()
                                .gap(px(set.line_spacing))
                                .overflow_hidden()
                                .children(content),
                        )
                    })
                    // Transport + progress bar + footer
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(14.0))
                                    .child(
                                        div()
                                            .id("ctrl-prev")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(8.0))
                                            .hover(|s| s.bg(rgba(0xffffff1a)))
                                            .text_size(px(13.0))
                                            .cursor_pointer()
                                            .on_click(|_, _, _| {
                                                std::thread::spawn(|| {
                                                    send_action(MediaAction::Previous)
                                                });
                                            })
                                            .child("⏮"),
                                    )
                                    .child(
                                        div()
                                            .id("ctrl-play")
                                            .px(px(12.0))
                                            .py(px(4.0))
                                            .rounded(px(999.0))
                                            .bg(rgba(0xffffff22))
                                            .hover(|s| s.bg(rgba(0xffffff38)))
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::BOLD)
                                            .cursor_pointer()
                                            .on_click(|_, _, _| {
                                                std::thread::spawn(|| {
                                                    send_action(MediaAction::TogglePlay)
                                                });
                                            })
                                            .child(if now.playing { "⏸" } else { "▶" }),
                                    )
                                    .child(
                                        div()
                                            .id("ctrl-next")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(8.0))
                                            .hover(|s| s.bg(rgba(0xffffff1a)))
                                            .text_size(px(13.0))
                                            .cursor_pointer()
                                            .on_click(|_, _, _| {
                                                std::thread::spawn(|| {
                                                    send_action(MediaAction::Next)
                                                });
                                            })
                                            .child("⏭"),
                                    ),
                            )
                            .when(set.show_progress, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .h(px(set.bar_thickness.max(2.0)))
                                        .rounded(px(999.0))
                                        .bg(rgba(0xffffff26))
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .h_full()
                                                .w(gpui::relative(pct))
                                                .rounded(px(999.0))
                                                .bg(rgb(bar_color)),
                                        ),
                                )
                            })
                            .when(set.show_times || set.show_source, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_between()
                                        .text_size(px(11.0))
                                        .text_color(rgba(0xffffff80))
                                        .child(if set.show_times {
                                            format_time(pos)
                                        } else {
                                            String::new()
                                        })
                                        .child(if set.show_source {
                                            footer_source
                                        } else {
                                            String::new()
                                        })
                                        .child(if set.show_times {
                                            right_time
                                        } else {
                                            String::new()
                                        }),
                                )
                            }),
                    ),
            )
    }
}

fn start_background_worker(state: Arc<SharedState>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let mut last_key = String::new();
            let mut last_art: Option<(u32, u32, Vec<u8>)> = None;
            let mut last_accent = (46, 36, 58);

            loop {
                if let Some(mut polled) = poll_smtc(&last_key) {
                    let key = polled.track_key();
                    let changed = key != last_key;
                    if changed {
                        last_key = key.clone();
                        last_art = polled.art_rgba.clone();
                        last_accent = polled.accent_rgb;
                        if let Ok(mut l) = state.lyrics.write() {
                            *l = None;
                        }
                        let st = state.clone();
                        let title = polled.title.clone();
                        let artist = polled.artist.clone();
                        let dur = polled.duration;
                        tokio::spawn(async move {
                            let res = fetch_lyrics(&title, &artist, dur).await;
                            if let Ok(cur) = st.now.read() {
                                if cur.track_key() != key {
                                    return;
                                }
                            }
                            if let Ok(mut l) = st.lyrics.write() {
                                *l = res;
                            }
                        });
                    } else {
                        polled.art_rgba = last_art.clone();
                        polled.accent_rgb = last_accent;
                    }
                    if let Ok(mut n) = state.now.write() {
                        *n = polled;
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    });
}

fn main() {
    let settings = Arc::new(RwLock::new(Settings::load()));
    let state = Arc::new(SharedState {
        now: RwLock::new(NowPlaying::default()),
        lyrics: RwLock::new(None),
        settings: settings.clone(),
    });
    start_background_worker(state.clone());

    Application::new().run(move |cx: &mut App| {
        let _ = cx.text_system().add_fonts(vec![
            Cow::Borrowed(FONT_REGULAR),
            Cow::Borrowed(FONT_MEDIUM),
            Cow::Borrowed(FONT_SEMIBOLD),
            Cow::Borrowed(FONT_BOLD),
        ]);

        let (w, h) = settings
            .read()
            .map(|s| {
                let h = if s.lyrics_on_launch {
                    158.0 + s.panel_height
                } else {
                    158.0
                };
                (s.widget_width + 44.0, h)
            })
            .unwrap_or((394.0, 158.0));

        let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
        let st = state.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| cx.new(|cx| PlaybackWidget::new(st, cx)),
        )
        .expect("failed to open Playback GPUI window");
        cx.activate(true);
    });
}
