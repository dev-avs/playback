//! The card windows beside the widget: settings, the changelogs, setup, the
//! update prompt and version control. Each is the same page as the widget
//! with its own #hash (src/main.tsx routes it), frameless and transparent,
//! and sizes itself to its card through `fit`.

use super::widget::STAGE_PAD;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Card {
    Settings,
    /// what changed, after an update (from that version) or from Settings
    Changelog,
    /// every version before the current one, beside the latest
    PastChangelog,
    /// Spicetify and the bridge; opens at launch when there is something to do
    Setup,
    /// "an update is out", at launch
    Update,
    Versions,
}

impl Card {
    fn label(self) -> &'static str {
        match self {
            Card::Settings => "settings",
            Card::Changelog => "changelog",
            Card::PastChangelog => "changelog-past",
            Card::Setup => "setup",
            Card::Update => "update",
            Card::Versions => "versions",
        }
    }

    /// width and first height, in logical px, shadow room included
    fn size(self) -> (f64, f64) {
        let card = |w: f64, h: f64| (w + STAGE_PAD * 2.0, h);
        match self {
            Card::Settings => card(600.0, 520.0),
            Card::Changelog | Card::PastChangelog => card(320.0, 480.0),
            Card::Setup => card(340.0, 480.0),
            Card::Update => card(300.0, 240.0),
            Card::Versions => card(320.0, 560.0),
        }
    }
}

/// A rectangle in logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && self.right() > o.x && self.y < o.y + o.h && self.y + self.h > o.y
    }
}

/// Where a card opens, as electron/main.cjs placed it. `widget` is the
/// widget's window, `anchor` the window a past changelog opens beside, and
/// `area` the screen's work area.
pub fn place(card: Card, widget: Rect, anchor: Option<Rect>, area: Rect) -> (f64, f64) {
    let (w, h) = card.size();
    let centred = |top: f64| {
        (
            (area.x + (area.w - w) / 2.0).round(),
            (area.y + area.h * top).round(),
        )
    };
    match card {
        // beside the widget, on whichever side has room
        Card::Settings => {
            let right = widget.right();
            let x = if right + w <= area.right() {
                right - STAGE_PAD
            } else {
                widget.x - w + STAGE_PAD
            };
            (x, widget.y)
        }
        // read once, so it sits apart, unless that would cover the widget
        Card::Changelog => {
            let (mut x, mut y) = centred(0.2);
            if (Rect { x, y, w, h }).overlaps(&widget) {
                let right = widget.right() - STAGE_PAD;
                x = if right + w <= area.right() {
                    right
                } else {
                    area.x.max(widget.x - w + STAGE_PAD)
                };
                y = widget.y.max(area.y).min(area.y + area.h - h);
            }
            (x, y)
        }
        Card::PastChangelog => {
            let a = anchor.unwrap_or(widget);
            let x = if a.x + w <= area.right() - w {
                a.x + w - STAGE_PAD
            } else {
                a.x - w + STAGE_PAD
            };
            (x, a.y)
        }
        Card::Versions => centred(0.12),
        Card::Setup | Card::Update => centred(0.2),
    }
}

/// A window's bounds and its screen's work area, in logical px.
fn bounds(win: &WebviewWindow) -> Option<(Rect, Rect)> {
    let scale = win.scale_factor().ok()?;
    let pos = win.outer_position().ok()?.to_logical::<f64>(scale);
    let size = win.outer_size().ok()?.to_logical::<f64>(scale);
    let monitor = win.current_monitor().ok()??;
    let work = monitor.work_area();
    let (at, extent) = (
        work.position.to_logical::<f64>(scale),
        work.size.to_logical::<f64>(scale),
    );
    Some((
        Rect {
            x: pos.x,
            y: pos.y,
            w: size.width,
            h: size.height,
        },
        Rect {
            x: at.x,
            y: at.y,
            w: extent.width,
            h: extent.height,
        },
    ))
}

/// Tells the settings window whether What's new is open, so its button can
/// grey out.
fn changelog_changed(app: &AppHandle) {
    let open = app.get_webview_window(Card::Changelog.label()).is_some();
    let _ = app.emit_to(Card::Settings.label(), "changelog:openChanged", open);
}

/// Opens a card, or brings it forward when it is already open. `query` is
/// added to the hash (the changelog's `from=<version>`).
pub fn open(app: &AppHandle, card: Card, query: Option<String>) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window(card.label()) {
        return win.set_focus();
    }
    let Some(widget) = app.get_webview_window(super::WIDGET) else {
        return Ok(());
    };
    let (widget_rect, area) = bounds(&widget).unwrap_or((
        Rect {
            x: 100.0,
            y: 100.0,
            w: 394.0,
            h: 420.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1920.0,
            h: 1040.0,
        },
    ));
    let anchor = (card == Card::PastChangelog)
        .then(|| app.get_webview_window(Card::Changelog.label()))
        .flatten()
        .and_then(|w| bounds(&w))
        .map(|b| b.0);
    let (x, y) = place(card, widget_rect, anchor, area);
    let (w, h) = card.size();

    // the widget's own address, so dev (Vite) and release (bundled) both work
    let mut url = widget.url()?;
    url.set_fragment(Some(&match query {
        Some(q) => format!("{}?{q}", card.label()),
        None => card.label().to_string(),
    }));
    let on_top = card != Card::Settings || widget.is_always_on_top().unwrap_or(true);
    let mut builder = WebviewWindowBuilder::new(app, card.label(), WebviewUrl::External(url));
    if let Some(script) = crate::migrate::settings_script(app) {
        builder = builder.initialization_script(script);
    }
    let win = builder
        .title("Playback")
        .inner_size(w, h)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(on_top)
        // shown once the page has drawn, so it never flashes empty
        .visible(false)
        .on_page_load(|win, load| {
            if matches!(load.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = win.show();
            }
        })
        .build()?;

    let app = app.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            match card {
                Card::Changelog => changelog_changed(&app),
                // re-read next time, so a font installed meanwhile appears
                Card::Settings => super::fonts::forget(),
                // the widget and the update prompt wait for Setup to go
                Card::Setup => app.state::<crate::launch::Launch>().settle(),
                _ => {}
            }
        }
    });
    if card == Card::Changelog {
        changelog_changed(win.app_handle());
    }
    Ok(())
}

pub fn close(app: &AppHandle, card: Card) {
    if let Some(win) = app.get_webview_window(card.label()) {
        let _ = win.close();
    }
}

pub fn is_open(app: &AppHandle, card: Card) -> bool {
    app.get_webview_window(card.label()).is_some()
}

/// Sizes a card window to its card: the width stays, the height follows.
pub fn fit(win: &WebviewWindow, height: f64) {
    let h = height.ceil().clamp(120.0, 1000.0);
    let Ok(scale) = win.scale_factor() else {
        return;
    };
    let Ok(size) = win.inner_size() else { return };
    let size = size.to_logical::<f64>(scale);
    if (size.height - h).abs() < 2.0 {
        return;
    }
    let _ = win.set_size(tauri::LogicalSize::new(size.width, h));
}

#[cfg(test)]
mod tests {
    use super::{place, Card, Rect};

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1040.0,
    };

    #[test]
    fn settings_open_on_the_side_with_room() {
        let left = Rect {
            x: 100.0,
            y: 200.0,
            w: 394.0,
            h: 300.0,
        };
        assert_eq!(place(Card::Settings, left, None, AREA), (472.0, 200.0));
        let right = Rect {
            x: 1500.0,
            y: 200.0,
            w: 394.0,
            h: 300.0,
        };
        assert_eq!(
            place(Card::Settings, right, None, AREA),
            (1500.0 - 644.0 + 22.0, 200.0)
        );
    }

    #[test]
    fn the_changelog_moves_off_the_widget() {
        let far = Rect {
            x: 0.0,
            y: 800.0,
            w: 394.0,
            h: 200.0,
        };
        assert_eq!(place(Card::Changelog, far, None, AREA), (778.0, 208.0));
        let under = Rect {
            x: 800.0,
            y: 200.0,
            w: 394.0,
            h: 300.0,
        };
        assert_eq!(place(Card::Changelog, under, None, AREA), (1172.0, 200.0));
    }
}
