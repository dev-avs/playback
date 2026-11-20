use crate::settings::{Settings, CATEGORIES};
use gpui::{div, prelude::*, px, rgb, rgba, Context, FontWeight, SharedString, Window};
use std::sync::{Arc, RwLock};

pub struct SettingsView {
    pub shared: Arc<RwLock<Settings>>,
    pub active_cat: usize,
}

impl SettingsView {
    pub fn new(shared: Arc<RwLock<Settings>>) -> Self {
        Self {
            shared,
            active_cat: 0,
        }
    }

    fn mutate(&self, f: impl FnOnce(&mut Settings)) {
        if let Ok(mut s) = self.shared.write() {
            f(&mut s);
            s.save();
        }
    }

    fn toggle_row(
        &self,
        id: &'static str,
        label: &'static str,
        desc: &'static str,
        value: bool,
        on_toggle: impl Fn(&mut Settings) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(10.0))
            .bg(rgba(0xffffff0c))
            .border_1()
            .border_color(rgba(0xffffff12))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf5f5f7))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(0xffffff80))
                            .child(desc),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(id))
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded(px(999.0))
                    .px(px(3.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .when(!value, |d| d.justify_start())
                    .bg(if value {
                        rgb(0xffffff)
                    } else {
                        rgba(0xffffff28)
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mutate(&on_toggle);
                        cx.notify();
                    }))
                    .child(div().size(px(16.0)).rounded(px(999.0)).bg(if value {
                        rgb(0x0b0b0d)
                    } else {
                        rgb(0xffffff)
                    })),
            )
    }

    fn cycle_row(
        &self,
        id: &'static str,
        label: &'static str,
        desc: &'static str,
        current: String,
        on_cycle: impl Fn(&mut Settings) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(10.0))
            .bg(rgba(0xffffff0c))
            .border_1()
            .border_color(rgba(0xffffff12))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf5f5f7))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(0xffffff80))
                            .child(desc),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(id))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .bg(rgba(0xffffff1a))
                    .hover(|s| s.bg(rgba(0xffffff2c)))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mutate(&on_cycle);
                        cx.notify();
                    }))
                    .child(current),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn number_row(
        &self,
        id_minus: &'static str,
        id_plus: &'static str,
        label: &'static str,
        desc: &'static str,
        display: String,
        on_minus: impl Fn(&mut Settings) + 'static,
        on_plus: impl Fn(&mut Settings) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(10.0))
            .bg(rgba(0xffffff0c))
            .border_1()
            .border_color(rgba(0xffffff12))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf5f5f7))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(0xffffff80))
                            .child(desc),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id(SharedString::from(id_minus))
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded(px(6.0))
                            .bg(rgba(0xffffff1a))
                            .hover(|s| s.bg(rgba(0xffffff30)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.mutate(&on_minus);
                                cx.notify();
                            }))
                            .child("−"),
                    )
                    .child(
                        div()
                            .min_w(px(46.0))
                            .text_center()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(display),
                    )
                    .child(
                        div()
                            .id(SharedString::from(id_plus))
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded(px(6.0))
                            .bg(rgba(0xffffff1a))
                            .hover(|s| s.bg(rgba(0xffffff30)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.mutate(&on_plus);
                                cx.notify();
                            }))
                            .child("+"),
                    ),
            )
    }
}

fn cycle(cur: &str, opts: &[&str]) -> String {
    let idx = opts.iter().position(|&o| o == cur).unwrap_or(0);
    opts[(idx + 1) % opts.len()].to_string()
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.shared.read().map(|g| g.clone()).unwrap_or_default();
        let active_cat = self.active_cat;

        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        match active_cat {
            0 => {
                // Lyrics
                rows.push(
                    self.cycle_row(
                        "set-provider",
                        "Lyric provider",
                        "Auto picks the best timed source; or force one.",
                        s.lyric_provider.clone(),
                        |st| {
                            st.lyric_provider = cycle(
                                &st.lyric_provider,
                                &["auto", "spicy", "lrcmux", "netease", "lrclib"],
                            )
                        },
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-lyrics-launch",
                        "Open lyrics on launch",
                        "Start expanded with the lyrics panel open.",
                        s.lyrics_on_launch,
                        |st| st.lyrics_on_launch = !st.lyrics_on_launch,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-click-jump",
                        "Click line to seek",
                        "Click any synced lyric line to jump playback to it.",
                        s.click_to_jump,
                        |st| st.click_to_jump = !st.click_to_jump,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-blur-lines",
                        "Dim inactive lines",
                        "Soften lines that are not currently being sung.",
                        s.blur_lines,
                        |st| st.blur_lines = !st.blur_lines,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.number_row(
                        "set-offset-m",
                        "set-offset-p",
                        "Lyric offset",
                        "Shift synced lyrics earlier or later (seconds).",
                        format!("{:+.1}s", s.lyric_offset),
                        |st| st.lyric_offset = ((st.lyric_offset - 0.1) * 10.0).round() / 10.0,
                        |st| st.lyric_offset = ((st.lyric_offset + 0.1) * 10.0).round() / 10.0,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.number_row(
                        "set-panel-m",
                        "set-panel-p",
                        "Panel height",
                        "Height of the expanded lyrics list.",
                        format!("{} px", s.panel_height as i32),
                        |st| st.panel_height = (st.panel_height - 20.0).max(160.0),
                        |st| st.panel_height = (st.panel_height + 20.0).min(560.0),
                        cx,
                    )
                    .into_any_element(),
                );
            }
            1 => {
                // Lyric text
                rows.push(
                    self.number_row(
                        "set-lsize-m",
                        "set-lsize-p",
                        "Lyric size",
                        "Font size of lyric lines.",
                        format!("{} px", s.lyric_size as i32),
                        |st| st.lyric_size = (st.lyric_size - 1.0).max(12.0),
                        |st| st.lyric_size = (st.lyric_size + 1.0).min(30.0),
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.cycle_row(
                        "set-lweight",
                        "Lyric weight",
                        "Font thickness for lyrics.",
                        s.lyric_weight.clone(),
                        |st| {
                            st.lyric_weight =
                                cycle(&st.lyric_weight, &["normal", "semibold", "bold", "heavy"])
                        },
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.cycle_row(
                        "set-lalign",
                        "Alignment",
                        "Left-aligned or centered lyrics.",
                        s.lyric_align.clone(),
                        |st| st.lyric_align = cycle(&st.lyric_align, &["left", "center"]),
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-allcaps",
                        "All caps",
                        "Display lyric lines in uppercase.",
                        s.all_caps,
                        |st| st.all_caps = !st.all_caps,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-italic",
                        "Italic lyrics",
                        "Slant lyric lines.",
                        s.italic_lyrics,
                        |st| st.italic_lyrics = !st.italic_lyrics,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            2 => {
                // Languages
                rows.push(
                    self.cycle_row(
                        "set-roma",
                        "Romanize default",
                        "Off, Latin only, or Both (dual lines).",
                        s.romanize_default.clone(),
                        |st| {
                            st.romanize_default =
                                cycle(&st.romanize_default, &["off", "latin", "dual"])
                        },
                        cx,
                    )
                    .into_any_element(),
                );
            }
            3 => {
                // Highlight
                rows.push(
                    self.cycle_row(
                        "set-highlight",
                        "Highlight color",
                        "Color of the active sung lyric line.",
                        s.highlight.clone(),
                        |st| {
                            st.highlight = cycle(
                                &st.highlight,
                                &["white", "pink", "gold", "mint", "sky", "rainbow"],
                            )
                        },
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.cycle_row(
                        "set-motion",
                        "Lyric motion",
                        "How much active lines emphasize.",
                        s.lyric_motion.clone(),
                        |st| {
                            st.lyric_motion =
                                cycle(&st.lyric_motion, &["off", "subtle", "normal", "expressive"])
                        },
                        cx,
                    )
                    .into_any_element(),
                );
            }
            4 => {
                // Glass
                rows.push(
                    self.cycle_row(
                        "set-tint",
                        "Glass tint",
                        "Base color of the widget card.",
                        s.glass_tint.clone(),
                        |st| {
                            st.glass_tint = cycle(
                                &st.glass_tint,
                                &["dark", "black", "warm", "cool", "rose", "forest"],
                            )
                        },
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.number_row(
                        "set-alpha-m",
                        "set-alpha-p",
                        "Background opacity",
                        "0% clear glass to 100% solid.",
                        format!("{}%", s.bg_alpha as i32),
                        |st| st.bg_alpha = (st.bg_alpha - 6.0).max(20.0),
                        |st| st.bg_alpha = (st.bg_alpha + 6.0).min(100.0),
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.number_row(
                        "set-corners-m",
                        "set-corners-p",
                        "Corner radius",
                        "Roundness of the widget card.",
                        format!("{} px", s.corners as i32),
                        |st| st.corners = (st.corners - 2.0).max(0.0),
                        |st| st.corners = (st.corners + 2.0).min(28.0),
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-edge",
                        "Edge highlight",
                        "Subtle hairline border around the card.",
                        s.edge_highlight,
                        |st| st.edge_highlight = !st.edge_highlight,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            5 => {
                // Header
                rows.push(
                    self.toggle_row(
                        "set-cover",
                        "Show cover art",
                        "Display the album thumbnail in the header.",
                        s.show_cover,
                        |st| st.show_cover = !st.show_cover,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.cycle_row(
                        "set-cover-shape",
                        "Cover shape",
                        "Rounded, square, or circle thumbnail.",
                        s.cover_shape.clone(),
                        |st| {
                            st.cover_shape =
                                cycle(&st.cover_shape, &["rounded", "square", "circle"])
                        },
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-artist",
                        "Show artist",
                        "Display the artist name under the song title.",
                        s.show_artist,
                        |st| st.show_artist = !st.show_artist,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            6 => {
                // Controls
                rows.push(
                    self.toggle_row(
                        "set-prog",
                        "Show progress bar",
                        "Playback timeline bar at the bottom.",
                        s.show_progress,
                        |st| st.show_progress = !st.show_progress,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-times",
                        "Show timestamps",
                        "Elapsed and remaining/total song times.",
                        s.show_times,
                        |st| st.show_times = !st.show_times,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-total-time",
                        "Show total time",
                        "Right timestamp shows song length instead of time left.",
                        s.show_total_time,
                        |st| st.show_total_time = !st.show_total_time,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-source",
                        "Show source label",
                        "Display media app and lyrics provider in the footer.",
                        s.show_source,
                        |st| st.show_source = !st.show_source,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-tint-bar",
                        "Tint progress bar",
                        "Color the progress bar using the Highlight color.",
                        s.tint_bar,
                        |st| st.tint_bar = !st.tint_bar,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            7 => {
                // Album mode
                rows.push(
                    self.toggle_row(
                        "set-album-launch",
                        "Start in album mode",
                        "Open in compact album-art mode on launch.",
                        s.album_on_launch,
                        |st| st.album_on_launch = !st.album_on_launch,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-viz",
                        "Show visualizer",
                        "Animated bars next to the title.",
                        s.show_visualizer,
                        |st| st.show_visualizer = !st.show_visualizer,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            8 => {
                // Fun
                rows.push(
                    self.toggle_row(
                        "set-spin",
                        "Spin cover art",
                        "Turn the album cover like a vinyl record.",
                        s.spin_cover,
                        |st| st.spin_cover = !st.spin_cover,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-party",
                        "Party background",
                        "Cycle the ambient glass color.",
                        s.party_background,
                        |st| st.party_background = !st.party_background,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            9 => {
                // Fullscreen
                rows.push(
                    self.cycle_row(
                        "set-fs-layout",
                        "Fullscreen layout",
                        "Split cover + lyrics, or lyrics-only.",
                        s.fs_layout.clone(),
                        |st| st.fs_layout = cycle(&st.fs_layout, &["split", "lyrics"]),
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-fs-lyrics",
                        "Show lyrics in fullscreen",
                        "Open fullscreen with lyrics visible.",
                        s.fs_show_lyrics,
                        |st| st.fs_show_lyrics = !st.fs_show_lyrics,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            10 => {
                // Behavior
                rows.push(
                    self.toggle_row(
                        "set-ontop",
                        "Always on top",
                        "Keep the widget floating above other windows.",
                        s.always_on_top,
                        |st| st.always_on_top = !st.always_on_top,
                        cx,
                    )
                    .into_any_element(),
                );
                rows.push(
                    self.toggle_row(
                        "set-hide-idle",
                        "Hide when idle",
                        "Hide the widget when no media is playing.",
                        s.hide_when_idle,
                        |st| st.hide_when_idle = !st.hide_when_idle,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            _ => {
                // App
                rows.push(
                    self.toggle_row(
                        "set-changelog",
                        "Show changelog after updates",
                        "Open what's new on the first launch of a new version.",
                        s.show_changelog,
                        |st| st.show_changelog = !st.show_changelog,
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }

        let _ = window;
        div().size_full().p(px(10.0)).bg(rgba(0x00000000)).child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .rounded(px(16.0))
                .bg(rgba(0x121216f2))
                .border_1()
                .border_color(rgba(0xffffff1a))
                .text_color(rgb(0xf5f5f7))
                .overflow_hidden()
                // Title bar
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .px(px(16.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(rgba(0xffffff12))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::BOLD)
                                .child("Playback Settings · GPUI"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("settings-reset")
                                        .px(px(10.0))
                                        .py(px(4.0))
                                        .rounded(px(999.0))
                                        .bg(rgba(0xffffff14))
                                        .hover(|st| st.bg(rgba(0xffffff24)))
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mutate(|st| *st = Settings::default());
                                            cx.notify();
                                        }))
                                        .child("Reset defaults"),
                                )
                                .child(
                                    div()
                                        .id("settings-close")
                                        .w(px(24.0))
                                        .h(px(24.0))
                                        .rounded(px(999.0))
                                        .bg(rgba(0xffffff14))
                                        .hover(|st| st.bg(rgb(0xff453a)))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(12.0))
                                        .cursor_pointer()
                                        .on_click(|_, window, _| {
                                            window.remove_window();
                                        })
                                        .child("×"),
                                ),
                        ),
                )
                // Sidebar + content
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_row()
                        .overflow_hidden()
                        .child(
                            div()
                                .w(px(144.0))
                                .p(px(10.0))
                                .border_r_1()
                                .border_color(rgba(0xffffff10))
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .children(CATEGORIES.iter().enumerate().map(|(i, name)| {
                                    let is_active = i == active_cat;
                                    div()
                                        .id(SharedString::from(format!("cat-{i}")))
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .rounded(px(8.0))
                                        .text_size(px(12.0))
                                        .font_weight(if is_active {
                                            FontWeight::BOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .bg(if is_active {
                                            rgba(0xffffff1f)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(if is_active {
                                            rgb(0xffffff)
                                        } else {
                                            rgba(0xffffff99)
                                        })
                                        .hover(|st| st.bg(rgba(0xffffff14)))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.active_cat = i;
                                            cx.notify();
                                        }))
                                        .child(*name)
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .p(px(14.0))
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(15.0))
                                        .font_weight(FontWeight::BOLD)
                                        .mb(px(4.0))
                                        .child(CATEGORIES[active_cat]),
                                )
                                .children(rows),
                        ),
                ),
        )
    }
}
