//! The line-state rules of src/App.tsx, in App.tsx order. In the TypeScript they
//! now live in src/lineState.ts (moved verbatim out of App.tsx so the golden
//! script can load them without React); the `// src/App.tsx:<line>` comments give
//! the line they had in App.tsx at 1.6.3-a.
//!
//! Lines are `crate::lyrics::LyricLine`. Numbers are `f64` and every comparison
//! keeps JavaScript's NaN behaviour (`t >= x` is false for NaN, `Math.max/min`
//! return NaN).

use crate::lyrics::{mark_parenthetical_backing_vocals, LyricLine, LyricWord};
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

// ── JS number helpers (no TS counterpart) ──

/// `Math.min(a, b)`
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b {
        a
    } else {
        b
    }
}

/// `Math.max(a, b)`
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// `String(n)` for a non-negative integer-valued double (what `m.toString()` sees in `fmt`):
/// digits as-is below 1e21, `d.ddde+N` from there on.
fn js_int_to_string(n: f64) -> String {
    if n < 1e21 {
        format!("{n}")
    } else {
        format!("{n:e}").replacen('e', "e+", 1)
    }
}

// src/App.tsx:85 the next-song popup shows for this many seconds before the song ends
pub const NEXT_POPUP_SECS: f64 = 10.0;

// src/App.tsx:94
pub const MIN_GAP: f64 = 3.0; // getLyricsBetweenShow() — interlude threshold
// src/App.tsx:95
pub const PRE_HIDDEN_DOT_MS: f64 = 500.0; // matches spicy-lyrics' preHiddenDotLineMs
// src/App.tsx:96
pub const INTERLUDE_PADDING: f64 = -(PRE_HIDDEN_DOT_MS + 50.0) / 1000.0; // -0.55s

// src/App.tsx:103
pub fn make_gap(from: f64, to: f64) -> LyricLine {
    let total_time = to - from;
    let base_dot_time = total_time / 3.0;
    let dot_padding = INTERLUDE_PADDING / 3.0;
    let d1 = js_max(from, from + base_dot_time + dot_padding);
    let d2 = js_max(d1, from + base_dot_time * 2.0 + dot_padding * 2.0);
    let d3 = js_max(d2, from + total_time + INTERLUDE_PADDING);

    let dot = |t: f64, end: f64| LyricWord { t, end: Some(end), text: "•".to_string(), ..Default::default() };
    LyricLine {
        t: from,
        end: Some(to),
        gap: Some(true),
        hide_at: Some(js_max(from, to - PRE_HIDDEN_DOT_MS / 1000.0)),
        text: "• • •".to_string(),
        words: Some(vec![dot(from, d1), dot(d1, d2), dot(d2, d3)]),
        ..Default::default()
    }
}

// src/App.tsx:137
pub fn line_end_of(line: &LyricLine, next: &LyricLine, estimated: bool) -> f64 {
    if let Some(end) = line.end {
        return end;
    }
    if !estimated {
        if let Some(words) = line.words.as_ref().filter(|w| !w.is_empty()) {
            let last_word = &words[words.len() - 1];
            // `lastWord.end ?? lastWord.t + 1.2`
            return js_min(last_word.end.unwrap_or(last_word.t + 1.2), next.t);
        }
    }
    // Nothing in the source says when this line stops, so it runs until the next
    // one starts.
    next.t
}

// src/App.tsx:159
pub fn with_gaps(lines: &[LyricLine], estimated: bool) -> Vec<LyricLine> {
    if lines.is_empty() {
        return lines.to_vec();
    }

    let mut out: Vec<LyricLine> = Vec::new();

    // Intro gap
    if lines[0].t >= MIN_GAP {
        out.push(make_gap(0.0, lines[0].t));
    }

    // A gap is discovered at its opening lead line but must be emitted *after*
    // that lead's backing vocals, so it is held until the next lead arrives.
    let mut pending_gap: Option<LyricLine> = None;

    for i in 0..lines.len() {
        let line = &lines[i];
        let bg = line.bg == Some(true);
        if !bg {
            if let Some(g) = pending_gap.take() {
                out.push(g);
            }
        }
        out.push(line.clone());
        if bg {
            continue;
        }

        let mut j = i + 1;
        while j < lines.len() && lines[j].bg == Some(true) {
            j += 1;
        }
        let Some(next) = lines.get(j) else { continue };

        let end = line_end_of(line, next, estimated);
        if next.t - end >= MIN_GAP {
            pending_gap = Some(make_gap(end, next.t));
        }
    }

    if let Some(g) = pending_gap {
        out.push(g);
    }
    out
}

// src/App.tsx:204
pub fn seal_ends(lines: &[LyricLine], estimated: bool) -> Vec<LyricLine> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line.end.is_some() {
                return line.clone();
            }
            let mut j = i + 1;
            while j < lines.len() && lines[j].bg == Some(true) {
                j += 1;
            }
            let fallback;
            let next = match lines.get(j) {
                Some(n) => n,
                None => {
                    fallback = LyricLine { t: line.t + 4.0, text: String::new(), ..Default::default() };
                    &fallback
                }
            };
            let mut l = line.clone();
            l.end = Some(line_end_of(line, next, estimated));
            l
        })
        .collect()
}

// src/App.tsx:227 forward lead on the playhead (the user's [ / ] nudge stacks on top)
pub const LYRIC_LEAD: f64 = 0.05;

// src/App.tsx:230 depth cue: each line of distance from the playhead adds this much bloom (px)
pub const BLUR_STEP: f64 = 0.55;
// src/App.tsx:231
pub const BLUR_MAX: f64 = 2.2;

// src/App.tsx:233
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveLines {
    /// every line that is Active right now, in document order
    pub active: Vec<usize>,
    /// the one line the scroller and the depth cue key off; -1 before playback
    pub anchor: isize,
}

// src/App.tsx:344
pub fn resolve_active(lines: &[LyricLine], t: f64) -> ActiveLines {
    let mut active: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(end) = line.end {
            if t >= line.t && t < end {
                active.push(i);
            }
        }
    }
    if !active.is_empty() {
        let lead = active.iter().copied().find(|&i| lines[i].bg != Some(true));
        let anchor = lead.unwrap_or(active[0]) as isize;
        return ActiveLines { active, anchor };
    }
    // Nothing Active — hold the view on the most recent line that started.
    for i in (0..lines.len()).rev() {
        if t >= lines[i].t {
            // `Math.max(0, i - 1)`
            let anchor = if lines[i].bg == Some(true) { i.saturating_sub(1) } else { i };
            return ActiveLines { active, anchor: anchor as isize };
        }
    }
    ActiveLines { active, anchor: -1 }
}

// src/App.tsx:363 interlude dots clear out shortly before the next line lands
pub fn is_gap_hiding(line: Option<&LyricLine>, t: f64) -> bool {
    match line {
        // `!!line?.gap && line.hideAt != null && t >= line.hideAt`
        Some(l) => l.gap == Some(true) && l.hide_at.is_some_and(|h| t >= h),
        None => false,
    }
}

// src/App.tsx:372
pub fn fmt(s: f64, remaining: bool) -> String {
    if !s.is_finite() {
        return if remaining { "-0:00" } else { "0:00" }.to_string();
    }
    let abs = s.abs();
    let m = (abs / 60.0).floor();
    let sec = (abs % 60.0).floor();
    // `${m}:${sec.toString().padStart(2, "0")}`; sec is an integer 0..=59
    let str = format!("{}:{:0>2}", js_int_to_string(m), format!("{sec}"));
    if remaining {
        format!("-{str}")
    } else {
        str
    }
}

// src/App.tsx:548 — scripts that join or reorder their glyphs cannot be split into per-letter boxes
// (Hebrew, Arabic, Devanagari, Bengali, Tamil, Thai, Khmer), so those words stay whole.
// `UNSPLITTABLE.test(text)`: any UTF-16 unit in the ranges (all are BMP).
pub fn unsplittable(c: char) -> bool {
    matches!(
        c as u32,
        0x0590..=0x05FF
            | 0x0600..=0x06FF
            | 0x0750..=0x077F
            | 0x08A0..=0x08FF
            | 0x0900..=0x097F
            | 0x0980..=0x09FF
            | 0x0B80..=0x0BFF
            | 0x0E00..=0x0E7F
            | 0x1780..=0x17FF
    )
}

// src/App.tsx:560 `segmenter` (Intl.Segmenter, grapheme) is `unicode_segmentation::graphemes(true)`.
// src/App.tsx:565
pub fn split_for_wave(text: &str) -> Option<Vec<String>> {
    // `text.length` is UTF-16 units
    if text.encode_utf16().count() < 2 || text.chars().any(unsplittable) {
        return None;
    }
    let chars: Vec<String> = text.graphemes(true).map(str::to_string).collect();
    if chars.len() > 1 {
        Some(chars)
    } else {
        None
    }
}

// ── extracted from App.tsx: inline expressions there, now functions in src/lineState.ts ──

// src/App.tsx:1710 `next + offsetRef.current + LYRIC_LEAD`
pub fn lyric_time_of(position: f64, offset: f64) -> f64 {
    position + offset + LYRIC_LEAD
}

// src/App.tsx:1692
pub fn is_near_end(dur: f64, next: f64) -> bool {
    dur > NEXT_POPUP_SECS * 2.0 && dur - next <= NEXT_POPUP_SECS && next > 0.0
}

// src/App.tsx:1311-1319 the tail of the `dynamicLyrics` memo; `synced` is `lyricsFor.synced`
// (undefined counts as synced)
pub fn finish_lines(base: &[LyricLine], synced: Option<bool>, line_sweep: bool) -> Vec<LyricLine> {
    if synced == Some(false) {
        return base.to_vec();
    }
    with_gaps(&mark_parenthetical_backing_vocals(&seal_ends(base, line_sweep)), line_sweep)
}

// src/App.tsx:1920
pub fn first_lyric_index(lines: &[LyricLine]) -> isize {
    lines.iter().position(|l| l.gap != Some(true) && l.bg != Some(true)).map_or(-1, |i| i as isize)
}

// src/App.tsx:1926 a backing vocal shares its lead's depth
pub fn lead_indexes(lines: &[LyricLine]) -> Vec<usize> {
    let mut map = Vec::with_capacity(lines.len());
    let mut lead = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.bg != Some(true) {
            lead = i;
        }
        map.push(lead);
    }
    map
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineFlags {
    pub is_current: bool,
    pub is_past: bool,
    pub is_intro_preview: bool,
}

// src/App.tsx:2470-2478
pub fn line_flags(
    lines: &[LyricLine],
    i: usize,
    is_synced: bool,
    active_set: &HashSet<usize>,
    elapsed: f64,
    first_lyric_idx: isize,
) -> LineFlags {
    let line = &lines[i];
    let is_current = is_synced && active_set.contains(&i);
    // `line.end != null && elapsed >= line.end`
    let is_past = is_synced && !is_current && line.end.is_some_and(|e| elapsed >= e);
    let is_intro_preview = is_synced && i as isize == first_lyric_idx && !is_current && elapsed < line.t;
    LineFlags { is_current, is_past, is_intro_preview }
}

// src/App.tsx:2481 `current_idx` is the anchor (-1 before playback); `lead_of` is `lead_indexes`
pub fn line_blur(
    blur_lines: bool,
    is_synced: bool,
    current_idx: isize,
    is_intro_preview: bool,
    lead_of: &[usize],
    i: usize,
) -> f64 {
    if blur_lines && is_synced && current_idx >= 0 && !is_intro_preview {
        js_min(BLUR_MAX, (lead_of[i] as f64 - lead_of[current_idx as usize] as f64).abs() * BLUR_STEP)
    } else {
        0.0
    }
}
