#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod cards;
mod lyrics;
mod media;
mod romanize;
mod settings;
mod settings_view;

use bridge::BridgeState;
use cards::ChangelogView;
use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, px, rgb, rgba, size, App,
    Application, Bounds, Context, FontWeight, ImageSource, MouseButton, RenderImage, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
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

const COLLAPSED_H: f32 = 122.0;

struct SharedState {
    now: RwLock<NowPlaying>,
    bridge: Arc<RwLock<BridgeState>>,
    lyrics: RwLock<Option<LyricsResult>>,
    settings: Arc<RwLock<Settings>>,
}

struct PlaybackWidget {
    state: Arc<SharedState>,
    lyrics_open: bool,
    album_mode: bool,
    fullscreen: bool,
    hovered: bool,
    roma_mode: String,
    last_art_key: String,
    art_image: Option<Arc<RenderImage>>,
}

impl PlaybackWidget {
    fn new(state: Arc<SharedState>, cx: &mut Context<Self>) -> Self {
        let (lyrics_open, album_mode, roma_mode) = state
            .settings
            .read()
            .map(|s| {
                (
                    s.lyrics_on_launch,
                    s.album_on_launch,
                    s.romanize_default.clone(),
                )
            })
            .unwrap_or((false, false, "off".into()));

        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();

        Self {
            state,
            lyrics_open,
            album_mode,
            fullscreen: false,
            hovered: false,
            roma_mode,
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

    fn sync_window_size(&self, window: &mut Window) {
        if self.fullscreen {
            return;
        }
        let (w, ph) = self
            .state
            .settings
            .read()
            .map(|s| (s.widget_width + 24.0, s.panel_height))
            .unwrap_or((374.0, 300.0));
        let h = if self.album_mode {
            w
        } else if self.lyrics_open {
            COLLAPSED_H + ph
        } else {
            COLLAPSED_H
        };
        window.resize(size(px(w), px(h)));
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

    fn open_changelog(&self, cx: &mut Context<Self>) {
        let bounds = Bounds::centered(None, size(px(440.0), px(280.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| cx.new(|_| ChangelogView),
        );
    }
}

fn rgba_u32(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
}

fn mix_dark(accent: (u8, u8, u8), base: (u8, u8, u8), weight: f32) -> (u8, u8, u8) {
    let blend =
        |a: u8, b: u8| ((a as f32 * weight + b as f32 * (1.0 - weight)).clamp(0.0, 255.0)) as u8;
    (
        blend(accent.0, base.0),
        blend(accent.1, base.1),
        blend(accent.2, base.2),
    )
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
        let br = self
            .state
            .bridge
            .read()
            .map(|b| b.clone())
            .unwrap_or_default();
        let lyrics = self.state.lyrics.read().ok().and_then(|l| l.clone());
        let set = self
            .state
            .settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default();

        let use_bridge =
            br.connected && now.app_name.eq_ignore_ascii_case("spotify") && br.duration > 1.0;
        let dur = if use_bridge {
            br.duration
        } else {
            now.duration
        };
        let pos = if use_bridge {
            let base = if br.playing {
                br.position + br.sampled_at.elapsed().as_secs_f64()
            } else {
                br.position
            };
            base.min(dur.max(0.0))
        } else {
            now.current_position()
        };
        let pct = if dur > 0.1 {
            (pos / dur).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let glass = set.glass_rgb();
        // Rich two-tone ambient backdrop matching Electron/Tauri's blurred cover glow.
        let top_c = mix_dark(now.accent_top, glass, 0.52);
        let bot_c = mix_dark(now.accent_bottom, (10, 10, 14), 0.34);
        let bg_a = ((set.bg_alpha.clamp(25.0, 100.0) / 100.0) * 246.0) as u8;
        let card_bg = linear_gradient(
            155.0,
            linear_color_stop(rgba(rgba_u32(top_c.0, top_c.1, top_c.2, bg_a)), 0.0),
            linear_color_stop(rgba(rgba_u32(bot_c.0, bot_c.1, bot_c.2, bg_a)), 1.0),
        );

        let highlight_hex = set.highlight_rgb();
        let bar_color = if set.tint_bar {
            highlight_hex
        } else {
            0xffffff
        };
        let cover_radius = match set.cover_shape.as_str() {
            "square" => px(2.0),
            "circle" => px(999.0),
            _ => px(7.0),
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
            ("", _) => "Playback".to_string(),
            (app, Some(l)) if set.show_lyric_source => {
                format!("{app} · {} {}", l.source, l.tier_dots())
            }
            (app, _) => app.to_string(),
        };

        let active_idx = lyrics
            .as_ref()
            .filter(|l| l.synced)
            .and_then(|l| active_line_index(&l.lines, pos + set.lyric_offset));

        let lyric_weight = font_weight_of(&set.lyric_weight);
        let centered = set.lyric_align == "center";
        let lyric_px = if self.fullscreen {
            set.lyric_size * (set.fs_lyric_scale / 100.0) * 1.65
        } else {
            set.lyric_size
        };

        let show_controls = self.hovered || set.pin_controls || self.fullscreen;
        let show_next_label =
            (set.next_song == "label" || set.next_song == "both") && br.next.is_some();

        let _ = window;

        // Album mode: sleek square cover with floating active lyric bubble.
        if self.album_mode && !self.fullscreen {
            let active_text = lyrics
                .as_ref()
                .and_then(|l| active_idx.and_then(|i| l.lines.get(i)))
                .map(|l| l.text.clone())
                .unwrap_or_else(|| title_text.clone());
            let bubble_a = ((set.bubble_opacity.clamp(15.0, 90.0) / 100.0) * 255.0) as u8;
            return div()
                .size_full()
                .p(px(8.0))
                .bg(rgba(0x00000000))
                .font_family("SpicyLyrics")
                .child(
                    div()
                        .id("album-card")
                        .size_full()
                        .relative()
                        .rounded(px(set.corners))
                        .overflow_hidden()
                        .bg(card_bg)
                        .border_1()
                        .border_color(rgba(0xffffff1a))
                        .on_mouse_down(MouseButton::Left, |_, window, _| {
                            window.start_window_move();
                        })
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, _, cx| this.open_settings(cx)),
                        )
                        .when_some(self.art_image.clone(), |el, image| {
                            el.child(img(ImageSource::Render(image)).size_full())
                        })
                        .child(
                            div()
                                .id("album-exit")
                                .absolute()
                                .top(px(10.0))
                                .right(px(10.0))
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded(px(999.0))
                                .bg(rgba(0x00000088))
                                .text_size(px(10.5))
                                .text_color(rgb(0xffffff))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.album_mode = false;
                                    this.sync_window_size(window);
                                    cx.notify();
                                }))
                                .child("Widget"),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(12.0))
                                .right(px(12.0))
                                .bottom(px(12.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .rounded(px(12.0))
                                .bg(rgba(rgba_u32(10, 10, 12, bubble_a)))
                                .text_size(px(set.album_lyric_size))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(highlight_hex))
                                .child(active_text),
                        ),
                );
        }

        div()
            .size_full()
            .p(px(if self.fullscreen { 0.0 } else { 8.0 }))
            .bg(rgba(0x00000000))
            .font_family("SpicyLyrics")
            .child(
                div()
                    .id("widget-card")
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .px(px(if self.fullscreen { 48.0 } else { 14.0 }))
                    .py(px(if self.fullscreen { 36.0 } else { 12.0 }))
                    .rounded(px(if self.fullscreen { 0.0 } else { set.corners }))
                    .bg(card_bg)
                    .when(set.edge_highlight && !self.fullscreen, |d| {
                        d.border_1().border_color(rgba(0xffffff1a))
                    })
                    .text_color(rgb(0xf5f5f7))
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.hovered = *hovered;
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.open_settings(cx)),
                    )
                    // Clean zero-clutter header (matches Electron & Tauri)
                    .child(
                        div()
                            .id("widget-header")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            })
                            .when(set.show_cover, |d| {
                                d.child(
                                    div()
                                        .id("cover-btn")
                                        .size(px(38.0))
                                        .flex_none()
                                        .rounded(cover_radius)
                                        .overflow_hidden()
                                        .bg(rgba(0xffffff14))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.album_mode = !this.album_mode;
                                            this.sync_window_size(window);
                                            cx.notify();
                                        }))
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
                                            .text_size(px(13.5 * set.title_scale))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .truncate()
                                            .child(title_text),
                                    )
                                    .when(set.show_artist, |d| {
                                        d.child(
                                            div()
                                                .text_size(px(11.5 * set.title_scale))
                                                .text_color(rgba(0xffffff8c))
                                                .truncate()
                                                .child(artist_text),
                                        )
                                    }),
                            )
                            .when(show_next_label, |d| {
                                let next_title = br
                                    .next
                                    .as_ref()
                                    .map(|n| n.title.clone())
                                    .unwrap_or_default();
                                d.child(
                                    div()
                                        .max_w(px(96.0))
                                        .flex()
                                        .flex_col()
                                        .items_end()
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgba(0xffffff66))
                                                .child("NEXT"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgba(0xffffffdd))
                                                .truncate()
                                                .child(next_title),
                                        ),
                                )
                            }),
                    )
                    // Synced lyrics list (clean generous spacing, matches Electron/Tauri)
                    .when(self.lyrics_open || self.fullscreen, |d| {
                        let visible_count = if self.fullscreen { 8 } else { 6 };
                        let content: Vec<gpui::AnyElement> = if let Some(ref res) = lyrics {
                            let filtered: Vec<(usize, &lyrics::LyricLine)> = res
                                .lines
                                .iter()
                                .enumerate()
                                .filter(|(_, l)| !(set.hide_gaps && l.is_gap))
                                .filter(|(_, l)| !(set.hide_backing_vocals && l.is_backing))
                                .collect();
                            let focus = active_idx
                                .and_then(|ai| filtered.iter().position(|(i, _)| *i == ai))
                                .unwrap_or(0);
                            let start = focus.saturating_sub(1);
                            let end = (start + visible_count).min(filtered.len());
                            filtered[start..end]
                                .iter()
                                .map(|(orig_idx, line)| {
                                    let is_active = active_idx == Some(*orig_idx);
                                    let is_past = active_idx.is_some_and(|a| *orig_idx < a);
                                    let line_time = line.time;
                                    let click_jump = set.click_to_jump && res.synced;

                                    let primary_text = if self.roma_mode == "latin" {
                                        line.roma.clone().unwrap_or_else(|| line.text.clone())
                                    } else {
                                        line.text.clone()
                                    };
                                    let primary_text = if set.all_caps {
                                        primary_text.to_uppercase()
                                    } else {
                                        primary_text
                                    };

                                    let show_words = is_active
                                        && !line.words.is_empty()
                                        && self.roma_mode != "latin";

                                    let main_row = if show_words {
                                        let now_pos = pos + set.lyric_offset;
                                        div()
                                            .flex()
                                            .flex_row()
                                            .flex_wrap()
                                            .when(centered, |el| el.justify_center())
                                            .children(line.words.iter().map(|w| {
                                                let sung = now_pos >= w.start;
                                                let wt = if set.all_caps {
                                                    w.text.to_uppercase()
                                                } else {
                                                    w.text.clone()
                                                };
                                                div()
                                                    .text_color(if sung {
                                                        rgb(highlight_hex)
                                                    } else {
                                                        rgba(0xffffff59)
                                                    })
                                                    .child(wt)
                                            }))
                                            .into_any_element()
                                    } else {
                                        div()
                                            .child(if primary_text.is_empty() {
                                                "•••".to_string()
                                            } else {
                                                primary_text
                                            })
                                            .into_any_element()
                                    };

                                    let dual_sub = if self.roma_mode == "dual" {
                                        line.roma.clone()
                                    } else {
                                        None
                                    };

                                    div()
                                        .id(SharedString::from(format!("lyric-{orig_idx}")))
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .text_size(px(lyric_px))
                                        .font_weight(lyric_weight)
                                        .when(centered, |el| el.text_center())
                                        .text_color(if is_active {
                                            rgb(highlight_hex)
                                        } else if is_past {
                                            rgba(0xffffff55)
                                        } else if set.blur_lines {
                                            rgba(0xffffff42)
                                        } else {
                                            rgba(0xffffff73)
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
                                        .child(main_row)
                                        .when_some(dual_sub, |el, roma| {
                                            el.child(
                                                div()
                                                    .text_size(px((lyric_px * 0.72).max(11.0)))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if is_active {
                                                        rgba(0xffffffb8)
                                                    } else {
                                                        rgba(0xffffff40)
                                                    })
                                                    .child(roma),
                                            )
                                        })
                                        .into_any_element()
                                })
                                .collect()
                        } else {
                            vec![div()
                                .text_size(px(14.0))
                                .text_color(rgba(0xffffff55))
                                .child("No synced lyrics found for this track")
                                .into_any_element()]
                        };

                        d.child(
                            div()
                                .flex_1()
                                .my(px(14.0))
                                .flex()
                                .flex_col()
                                .justify_center()
                                .gap(px(set.line_spacing))
                                .overflow_hidden()
                                .children(content),
                        )
                    })
                    // Hover-revealed controls + thin progress bar + minimal footer
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .when(show_controls, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .pb(px(2.0))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap(px(4.0))
                                                .child(
                                                    div()
                                                        .id("btn-roma")
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(11.0))
                                                        .text_color(if self.roma_mode != "off" {
                                                            rgb(0xffffff)
                                                        } else {
                                                            rgba(0xffffff80)
                                                        })
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.roma_mode =
                                                                match this.roma_mode.as_str() {
                                                                    "off" => "latin".into(),
                                                                    "latin" => "dual".into(),
                                                                    _ => "off".into(),
                                                                };
                                                            cx.notify();
                                                        }))
                                                        .child("あ"),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-fs")
                                                        .px(px(5.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(11.0))
                                                        .text_color(rgba(0xffffff88))
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.fullscreen = !this.fullscreen;
                                                                if this.fullscreen {
                                                                    this.lyrics_open = true;
                                                                }
                                                                window.toggle_fullscreen();
                                                                if !this.fullscreen {
                                                                    this.sync_window_size(window);
                                                                }
                                                                cx.notify();
                                                            },
                                                        ))
                                                        .child("⛶"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap(px(10.0))
                                                .child(
                                                    div()
                                                        .id("ctrl-prev")
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(12.0))
                                                        .text_color(rgba(0xffffffcc))
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(|_, _, _| {
                                                            std::thread::spawn(|| {
                                                                send_action(MediaAction::Previous)
                                                            });
                                                        })
                                                        .child("‹‹"),
                                                )
                                                .child(
                                                    div()
                                                        .id("ctrl-play")
                                                        .px(px(10.0))
                                                        .py(px(2.0))
                                                        .rounded(px(999.0))
                                                        .bg(rgba(0xffffff20))
                                                        .hover(|s| s.bg(rgba(0xffffff36)))
                                                        .text_size(px(12.0))
                                                        .font_weight(FontWeight::BOLD)
                                                        .cursor_pointer()
                                                        .on_click(|_, _, _| {
                                                            std::thread::spawn(|| {
                                                                send_action(MediaAction::TogglePlay)
                                                            });
                                                        })
                                                        .child(if now.playing {
                                                            "❚❚"
                                                        } else {
                                                            "▶"
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .id("ctrl-next")
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(12.0))
                                                        .text_color(rgba(0xffffffcc))
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(|_, _, _| {
                                                            std::thread::spawn(|| {
                                                                send_action(MediaAction::Next)
                                                            });
                                                        })
                                                        .child("››"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap(px(4.0))
                                                .child(
                                                    div()
                                                        .id("btn-lyrics")
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(11.0))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(if self.lyrics_open {
                                                            rgb(0xffffff)
                                                        } else {
                                                            rgba(0xffffff80)
                                                        })
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.lyrics_open =
                                                                    !this.lyrics_open;
                                                                this.sync_window_size(window);
                                                                cx.notify();
                                                            },
                                                        ))
                                                        .child("Lyrics"),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-changelog")
                                                        .px(px(5.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(11.0))
                                                        .text_color(rgba(0xffffff77))
                                                        .hover(|s| s.bg(rgba(0xffffff1a)))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.open_changelog(cx);
                                                        }))
                                                        .child("?"),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-close")
                                                        .px(px(5.0))
                                                        .py(px(2.0))
                                                        .rounded(px(6.0))
                                                        .text_size(px(11.5))
                                                        .text_color(rgba(0xffffff77))
                                                        .hover(|s| {
                                                            s.bg(rgb(0xff453a))
                                                                .text_color(rgb(0xffffff))
                                                        })
                                                        .cursor_pointer()
                                                        .on_click(|_, _, cx| cx.quit())
                                                        .child("×"),
                                                ),
                                        ),
                                )
                            })
                            .when(set.show_progress, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .h(px(set.bar_thickness.max(2.5)))
                                        .rounded(px(999.0))
                                        .bg(rgba(0xffffff24))
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
                                        .text_size(px(10.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgba(0xffffff73))
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
            tokio::spawn(bridge::run_server(state.bridge.clone()));

            let mut last_key = String::new();
            let mut last_spicy_count = 0usize;
            let mut last_art: Option<(u32, u32, Vec<u8>)> = None;
            let mut last_top = (42, 32, 48);
            let mut last_bot = (16, 16, 20);

            loop {
                if let Some(mut polled) = poll_smtc(&last_key) {
                    let key = polled.track_key();
                    let (spicy_count, spicy_payload) = state
                        .bridge
                        .read()
                        .map(|b| {
                            let count = b.spicy_payloads.len();
                            let p = b.spicy_payloads.get(&b.track_id).cloned();
                            (count, p)
                        })
                        .unwrap_or((0, None));

                    let changed = key != last_key || spicy_count != last_spicy_count;
                    if changed {
                        if key != last_key {
                            last_key = key.clone();
                            last_art = polled.art_rgba.clone();
                            last_top = polled.accent_top;
                            last_bot = polled.accent_bottom;
                            if let Ok(mut l) = state.lyrics.write() {
                                *l = None;
                            }
                        } else {
                            polled.art_rgba = last_art.clone();
                            polled.accent_top = last_top;
                            polled.accent_bottom = last_bot;
                        }
                        last_spicy_count = spicy_count;
                        let st = state.clone();
                        let title = polled.title.clone();
                        let artist = polled.artist.clone();
                        let dur = polled.duration;
                        let provider = state
                            .settings
                            .read()
                            .map(|s| s.lyric_provider.clone())
                            .unwrap_or_else(|_| "auto".into());
                        tokio::spawn(async move {
                            let res =
                                fetch_lyrics(&title, &artist, dur, &provider, spicy_payload).await;
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
                        polled.accent_top = last_top;
                        polled.accent_bottom = last_bot;
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
        bridge: Arc::new(RwLock::new(BridgeState::default())),
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
                let w = s.widget_width + 24.0;
                let h = if s.album_on_launch {
                    w
                } else if s.lyrics_on_launch {
                    COLLAPSED_H + s.panel_height
                } else {
                    COLLAPSED_H
                };
                (w, h)
            })
            .unwrap_or((374.0, COLLAPSED_H));

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
