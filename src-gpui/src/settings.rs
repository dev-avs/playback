use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Every setting from `src/settings.ts` (`DEFAULTS`), kept in parity so the
/// GPUI port shares the same mental model and defaults as Electron and Tauri.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Lyrics
    pub romanize_default: String, // "off" | "latin" | "dual"
    pub animation_style: String,  // "playback" | "spicy"
    pub lyric_offset: f64,
    pub blur_lines: bool,
    pub lyrics_on_launch: bool,
    pub auto_sync: f64,
    pub click_to_jump: bool,
    pub show_line_times: bool,
    pub scroll_speed: f64,
    pub follow_speed: u32,
    pub hide_gaps: bool,
    pub hide_backing_vocals: bool,
    pub hide_credits: bool,
    pub panel_height: f32,
    pub lyric_provider: String, // "auto" | "spicy" | "lrcmux" | "netease" | "lrclib"
    pub song_offsets: HashMap<String, f64>,
    pub song_providers: HashMap<String, String>,

    // Lyric text
    pub lyric_size: f32,
    pub lyric_font: String,
    pub lyric_align: String,  // "left" | "center"
    pub lyric_weight: String, // "normal" | "semibold" | "bold" | "heavy"
    pub line_spacing: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub all_caps: bool,
    pub italic_lyrics: bool,

    // Languages
    pub font_ja: String,
    pub font_ko: String,
    pub font_zh: String,

    // Highlight
    pub highlight: String, // "white" | "pink" | "gold" | "mint" | "sky" | "rainbow"
    pub lyric_motion: String,
    pub upcoming_brightness: f32,
    pub sung_brightness: f32,
    pub highlight_strength: f32,
    pub glow_strength: f32,
    pub sweep_direction: String, // "across" | "down"
    pub sweep_softness: f32,

    // Glass
    pub corners: f32,
    pub cover_blur: f32,
    pub glass_tint: String, // "dark" | "black" | "warm" | "cool" | "rose" | "forest"
    pub cover_saturation: f32,
    pub cover_brightness: f32,
    pub edge_highlight: bool,
    pub shadow_strength: f32,
    pub bg_alpha: f32,
    pub rest_opacity: f32,

    // Header
    pub widget_width: f32,
    pub show_cover: bool,
    pub cover_shape: String, // "rounded" | "square" | "circle"
    pub show_artist: bool,
    pub title_scale: f32,
    pub pan_speed: f32,
    pub next_song: String, // "label" | "popup" | "both" | "off"

    // Controls
    pub show_progress: bool,
    pub bar_thickness: f32,
    pub tint_bar: bool,
    pub show_times: bool,
    pub show_total_time: bool,
    pub show_source: bool,
    pub show_lyric_source: bool,
    pub show_shuffle_repeat: bool,
    pub show_volume: bool,
    pub pin_controls: bool,

    // Album mode
    pub album_on_launch: bool,
    pub show_visualizer: bool,
    pub bubble_opacity: f32,
    pub album_lyric_size: f32,

    // Fun
    pub spin_cover: bool,
    pub spin_speed: f32,
    pub rainbow_speed: f32,
    pub party_background: bool,
    pub floating_cover: bool,

    // Fullscreen
    pub fs_layout: String, // "split" | "lyrics"
    pub fs_swap: bool,
    pub fs_hide_cover: bool,
    pub fs_show_lyrics: bool,
    pub fs_animated_bg: bool,
    pub fs_lyric_scale: f32,
    pub fs_pin_controls: bool,
    pub fs_on_top: bool,

    // Behavior
    pub always_on_top: bool,
    pub hide_when_idle: bool,

    // App
    pub show_changelog: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            romanize_default: "off".into(),
            animation_style: "playback".into(),
            lyric_offset: 0.0,
            blur_lines: true,
            lyrics_on_launch: false,
            auto_sync: 3.0,
            click_to_jump: true,
            show_line_times: true,
            scroll_speed: 1.0,
            follow_speed: 420,
            hide_gaps: false,
            hide_backing_vocals: false,
            hide_credits: false,
            panel_height: 300.0,
            lyric_provider: "auto".into(),
            song_offsets: HashMap::new(),
            song_providers: HashMap::new(),

            lyric_size: 17.0,
            lyric_font: "Default".into(),
            lyric_align: "left".into(),
            lyric_weight: "bold".into(),
            line_spacing: 10.0,
            line_height: 1.18,
            letter_spacing: 0.0,
            all_caps: false,
            italic_lyrics: false,

            font_ja: "default".into(),
            font_ko: "default".into(),
            font_zh: "default".into(),

            highlight: "white".into(),
            lyric_motion: "normal".into(),
            upcoming_brightness: 1.0,
            sung_brightness: 1.0,
            highlight_strength: 1.0,
            glow_strength: 1.0,
            sweep_direction: "across".into(),
            sweep_softness: 20.0,

            corners: 13.0,
            cover_blur: 50.0,
            glass_tint: "dark".into(),
            cover_saturation: 140.0,
            cover_brightness: 100.0,
            edge_highlight: true,
            shadow_strength: 100.0,
            bg_alpha: 72.0,
            rest_opacity: 100.0,

            widget_width: 350.0,
            show_cover: true,
            cover_shape: "rounded".into(),
            show_artist: true,
            title_scale: 1.0,
            pan_speed: 8.0,
            next_song: "label".into(),

            show_progress: true,
            bar_thickness: 2.0,
            tint_bar: false,
            show_times: true,
            show_total_time: false,
            show_source: true,
            show_lyric_source: true,
            show_shuffle_repeat: true,
            show_volume: true,
            pin_controls: false,

            album_on_launch: false,
            show_visualizer: true,
            bubble_opacity: 48.0,
            album_lyric_size: 13.0,

            spin_cover: false,
            spin_speed: 6.0,
            rainbow_speed: 8.0,
            party_background: false,
            floating_cover: false,

            fs_layout: "split".into(),
            fs_swap: false,
            fs_hide_cover: false,
            fs_show_lyrics: true,
            fs_animated_bg: true,
            fs_lyric_scale: 100.0,
            fs_pin_controls: false,
            fs_on_top: true,

            always_on_top: true,
            hide_when_idle: false,

            show_changelog: true,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("Playback").join("gpui-settings.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn highlight_rgb(&self) -> u32 {
        match self.highlight.as_str() {
            "pink" => 0xf896c8,
            "gold" => 0xfbd34d,
            "mint" => 0x7ee6b8,
            "sky" => 0x76cffb,
            "rainbow" => 0xf896c8,
            _ => 0xffffff,
        }
    }

    pub fn glass_rgb(&self) -> (u8, u8, u8) {
        match self.glass_tint.as_str() {
            "black" => (0, 0, 0),
            "warm" => (56, 34, 22),
            "cool" => (18, 30, 56),
            "rose" => (50, 20, 36),
            "forest" => (18, 42, 30),
            _ => (20, 20, 22),
        }
    }
}

pub const CATEGORIES: &[&str] = &[
    "Lyrics",
    "Lyric text",
    "Languages",
    "Highlight",
    "Glass",
    "Header",
    "Controls",
    "Album mode",
    "Fun",
    "Fullscreen",
    "Behavior",
    "App",
];

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn defaults_match_electron_values() {
        let s = Settings::default();
        assert_eq!(s.lyric_size, 17.0);
        assert_eq!(s.widget_width, 350.0);
        assert_eq!(s.panel_height, 300.0);
        assert_eq!(s.bg_alpha, 72.0);
        assert_eq!(s.corners, 13.0);
        assert_eq!(s.lyric_weight, "bold");
        assert!(s.always_on_top);
    }

    #[test]
    fn round_trips_json() {
        let s = Settings {
            highlight: "mint".into(),
            lyric_size: 20.0,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
