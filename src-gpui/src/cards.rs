//! Native GPUI secondary card windows: Changelog (What's New) and Updates.

use gpui::{div, prelude::*, px, rgb, rgba, Context, FontWeight, Window};

pub struct ChangelogView;

impl Render for ChangelogView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(10.0))
            .bg(rgba(0x00000000))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(18.0))
                    .rounded(px(16.0))
                    .bg(rgba(0x121216f4))
                    .border_1()
                    .border_color(rgba(0xffffff1a))
                    .text_color(rgb(0xf5f5f7))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("What's New · Playback 1.5.9"),
                            )
                            .child(
                                div()
                                    .id("cl-close")
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .rounded(px(999.0))
                                    .bg(rgba(0xffffff14))
                                    .hover(|s| s.bg(rgb(0xff453a)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .on_click(|_, window, _| window.remove_window())
                                    .child("×"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .text_size(px(12.5))
                            .text_color(rgba(0xffffffcc))
                            .child("• Native GPUI port with Spicetify bridge, Spicy Lyrics word-by-word highlight, Album mode, Fullscreen mode, and all 12 Settings categories.")
                            .child("• Reworked installer with flavor selector (Tauri, Electron, or GPUI).")
                            .child("• Bundled SpicyLyrics font locally and fixed Latin-only & counting romanization bugs.")
                            .child("• Redesigned all 12 Settings sidebar icons."),
                    ),
            )
    }
}
