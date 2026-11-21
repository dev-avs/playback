//! Where each card window opens (electron/main.cjs `openSettings`, `openPastChangelog`, `openChangelog`,
//! `openVersions`, `openUpdate`, `openSetup`). Pure functions over physical-pixel rects: a DIP constant `d`
//! becomes `ceil(d * scale)` (what `Shell::to_px` does for sizes), so every rule below is the main.cjs line
//! with its numbers scaled. `src-tauri/src/ui/cards.rs` `place` is the Tauri translation, used as a cross-check.

use super::widget::STAGE_PAD;
use crate::win32::Rect;

/// DIPs to physical pixels.
pub fn px(dip: i32, scale: f32) -> i32 {
    (dip as f32 * scale).ceil() as i32
}

/// JavaScript's `Math.round`: halves go up.
fn js_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CardKind {
    Settings,
    Changelog,
    PastChangelog,
    Setup,
    Update,
    Versions,
}

impl CardKind {
    /// The hash src/main.tsx:19-23 routes on (the changelog adds `?from=<version>`).
    pub fn route(self) -> &'static str {
        match self {
            CardKind::Settings => "settings",
            CardKind::Changelog => "changelog",
            CardKind::PastChangelog => "changelog-past",
            CardKind::Setup => "setup",
            CardKind::Update => "update",
            CardKind::Versions => "versions",
        }
    }

    /// Window width in DIPs: the card's width + 2 * STAGE_PAD.
    pub fn width(self) -> i32 {
        let pad = STAGE_PAD as i32 * 2;
        match self {
            // electron/main.cjs:1088
            CardKind::Settings => 600 + pad,
            // electron/main.cjs:1147 (both changelogs and Version control)
            CardKind::Changelog | CardKind::PastChangelog | CardKind::Versions => 320 + pad,
            // electron/main.cjs:1291
            CardKind::Update => 300 + pad,
            // electron/main.cjs:1358
            CardKind::Setup => 340 + pad,
        }
    }

    /// First window height in DIPs, before the card has measured itself.
    pub fn height0(self) -> i32 {
        match self {
            // electron/main.cjs:1105
            CardKind::Settings => 520,
            // electron/main.cjs:1165, 1221
            CardKind::Changelog | CardKind::PastChangelog => 480,
            // electron/main.cjs:1437
            CardKind::Setup => 480,
            // electron/main.cjs:1305
            CardKind::Update => 240,
            // electron/main.cjs:1265
            CardKind::Versions => 560,
        }
    }

    /// Settings is topmost exactly when the widget is (electron/main.cjs:1109); every other card always is
    /// (electron/main.cjs:1169, 1225, 1269, 1309, 1441).
    pub fn follows_widget_topmost(self) -> bool {
        self == CardKind::Settings
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

/// electron/main.cjs:1096-1101. `widget` is `win.getPosition()` and `widgetOuterW` in px.
pub fn settings(widget_xy: (i32, i32), widget_outer_w: i32, area: Rect, scale: f32) -> (i32, i32) {
    let (wx, wy) = widget_xy;
    let (w, pad) = (px(CardKind::Settings.width(), scale), px(STAGE_PAD as i32, scale));
    let right = wx + widget_outer_w;
    let x = if right + w <= area.x + area.w { right - pad } else { wx - w + pad };
    (x, wy)
}

/// electron/main.cjs:1155-1160. `anchor` is the changelog's position, else the widget's.
pub fn past_changelog(anchor_xy: (i32, i32), area: Rect, scale: f32) -> (i32, i32) {
    let (ax, ay) = anchor_xy;
    let (w, pad) = (px(CardKind::PastChangelog.width(), scale), px(STAGE_PAD as i32, scale));
    let x = if ax + w <= area.x + area.w - w { ax + w - pad } else { ax - w + pad };
    (x, ay)
}

/// electron/main.cjs:1201-1217. `widget` is `win.getBounds()` when the widget exists.
pub fn changelog(widget: Option<Rect>, area: Rect, scale: f32) -> (i32, i32) {
    let (w, h, pad) = (px(CardKind::Changelog.width(), scale), px(CardKind::Changelog.height0(), scale), px(STAGE_PAD as i32, scale));
    let mut x = js_round(area.x as f64 + (area.w - w) as f64 / 2.0);
    let mut y = js_round(area.y as f64 + area.h as f64 * 0.2);
    if let Some(b) = widget {
        if overlaps(Rect { x, y, w, h }, b) {
            let right = b.x + b.w - pad;
            x = if right + w <= area.x + area.w { right } else { area.x.max(b.x - w + pad) };
            y = b.y.max(area.y).min(area.y + area.h - h);
        }
    }
    (x, y)
}

/// Version control, electron/main.cjs:1262-1263: centred, y at 12 % of the work area.
pub fn versions(area: Rect, scale: f32) -> (i32, i32) {
    centred(CardKind::Versions, area, 0.12, scale)
}

/// Update (electron/main.cjs:1302-1303) and Setup (1434-1435): centred, y at 20 %.
pub fn centred_at_20(kind: CardKind, area: Rect, scale: f32) -> (i32, i32) {
    centred(kind, area, 0.2, scale)
}

fn centred(kind: CardKind, area: Rect, top: f64, scale: f32) -> (i32, i32) {
    let w = px(kind.width(), scale);
    (js_round(area.x as f64 + (area.w - w) as f64 / 2.0), js_round(area.y as f64 + area.h as f64 * top))
}

/// Everything `Windows::open_card` needs to know about the widget and the screen.
pub struct Anchors {
    /// `win.getBounds()`, None before the widget exists (the position is then [100, 100]).
    pub widget: Option<Rect>,
    pub widget_outer_w: i32,
    pub changelog: Option<Rect>,
    pub scale: f32,
}

/// The nearest work area is chosen by the caller (`screen.getDisplayNearestPoint` of the anchor).
pub fn place(kind: CardKind, c: &Anchors, area: Rect) -> (i32, i32) {
    let wpos = c.widget.map_or((100, 100), |b| (b.x, b.y));
    match kind {
        CardKind::Settings => settings(wpos, c.widget_outer_w, area, c.scale),
        CardKind::PastChangelog => past_changelog(c.changelog.map(|b| (b.x, b.y)).unwrap_or(wpos), area, c.scale),
        CardKind::Changelog => changelog(c.widget, area, c.scale),
        CardKind::Versions => versions(area, c.scale),
        CardKind::Setup | CardKind::Update => centred_at_20(kind, area, c.scale),
    }
}

/// The point whose display is asked for (`getDisplayNearestPoint`): the anchor window's position.
pub fn anchor_point(kind: CardKind, c: &Anchors) -> (i32, i32) {
    let wpos = c.widget.map_or((100, 100), |b| (b.x, b.y));
    if kind == CardKind::PastChangelog {
        c.changelog.map(|b| (b.x, b.y)).unwrap_or(wpos)
    } else {
        wpos
    }
}

/// `screen.getDisplayNearestPoint`: the display containing the point, else the closest one.
pub fn nearest(areas: &[Rect], x: i32, y: i32) -> Option<Rect> {
    let dist = |r: &Rect| {
        let dx = (r.x - x).max(0).max(x - (r.right() - 1)) as i64;
        let dy = (r.y - y).max(0).max(y - (r.bottom() - 1)) as i64;
        dx * dx + dy * dy
    };
    areas.iter().copied().min_by_key(dist)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    // the same cases as src-tauri/src/ui/cards.rs
    #[test]
    fn settings_open_on_the_side_with_room() {
        assert_eq!(settings((100, 200), 394, AREA, 1.0), (472, 200));
        assert_eq!(settings((1500, 200), 394, AREA, 1.0), (1500 - 644 + 22, 200));
    }

    #[test]
    fn the_changelog_moves_off_the_widget() {
        assert_eq!(changelog(Some(Rect { x: 0, y: 800, w: 394, h: 200 }), AREA, 1.0), (778, 208));
        assert_eq!(changelog(Some(Rect { x: 800, y: 200, w: 394, h: 300 }), AREA, 1.0), (1172, 200));
    }
}
