//! Getting a key without typing it: Spicy Lyrics' developer dashboard in a
//! window of its own, and the key taken off the clipboard the moment the
//! user copies it there. Ported from electron/spicyKey.cjs (connect).
//!
//! The window is the remote site and nothing else: its own browser profile
//! (signed in once, remembered after), and no capability, so the site can
//! call nothing in the app. All the app does is show a hint at the bottom
//! (the toast) and fill in the new application's name and URL.

use super::key::{self, Checked};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::watch;

const LABEL: &str = "spicy-key";
const KEY_PAGE_URL: &str = "https://developers.spicylyrics.org/dashboard/applications";
const SITE_HOST: &str = "developers.spicylyrics.org";
const APP_NAME: &str = "Playback";
const PROJECT_URL: &str = "https://github.com/dev-avs";
const POLL: Duration = Duration::from_millis(400);
const CLOSE_AFTER: Duration = Duration::from_millis(1200);

/// The open window's outcome: None while it is open, then whether a key
/// was saved. Every caller waits on the same one.
static OPEN: Mutex<Option<watch::Receiver<Option<bool>>>> = Mutex::new(None);

fn signing_in(url: &Url) -> bool {
    url.host_str() != Some(SITE_HOST)
        || url.path().starts_with("/sign-in")
        || url.path().starts_with("/sign-up")
}

fn step_for(url: &Url) -> &'static str {
    if signing_in(url) {
        "Let's get you signed-in first."
    } else {
        "Create an application, then copy the key to your clipboard"
    }
}

fn toast_script(text: &str) -> String {
    let text = serde_json::to_string(text).expect("a string");
    format!(
        r#"(() => {{
  window.__playerToastText = {text};
  const ensure = () => {{
    let t = document.getElementById("player-key-toast");
    if (!t) {{
      t = document.createElement("div");
      t.id = "player-key-toast";
      t.style.cssText = "position:fixed;left:50%;bottom:18px;transform:translateX(-50%);z-index:2147483647;" +
        "display:flex;align-items:center;gap:9px;padding:7px 14px 7px 11px;border-radius:6px;" +
        "background:#21141a;border:1px solid #3a2730;color:#ddd3d7;" +
        "font:600 13px/1.3 'Segoe UI',system-ui,sans-serif;pointer-events:none;white-space:nowrap";
      t.innerHTML = '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" ' +
        'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="2.5 8.5 6 12 13.5 4"/></svg><span></span>';
      (document.body ?? document.documentElement).appendChild(t);
    }}
    const label = t.querySelector("span");
    if (label.textContent !== window.__playerToastText) label.textContent = window.__playerToastText;
  }};
  ensure();
  if (!window.__playerToastTimer) window.__playerToastTimer = setInterval(ensure, 300);
}})()"#
    )
}

fn autofill_script() -> String {
    let (name, url) = (
        serde_json::to_string(APP_NAME).unwrap(),
        serde_json::to_string(PROJECT_URL).unwrap(),
    );
    format!(
        r#"(() => {{
  if (window.__playerAutofill) return;
  const set = (input, value) => {{
    if (!input || input.dataset.playerFilled || input.value) return;
    input.dataset.playerFilled = "1";
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, value);
    input.dispatchEvent(new Event("input", {{ bubbles: true }}));
  }};
  const inputAfter = (labelText) => {{
    const label = [...document.querySelectorAll("label")].find((l) => l.textContent.trim().toLowerCase() === labelText);
    if (!label) return null;
    return (label.htmlFor && document.getElementById(label.htmlFor)) ||
      label.querySelector("input") || label.parentElement?.querySelector("input");
  }};
  window.__playerAutofill = setInterval(() => {{
    set(inputAfter("name"), {name});
    set(document.querySelector('input[placeholder*="github.com/owner"]') || inputAfter("project url"), {url});
  }}, 300);
}})()"#
    )
}

/// The hint on screen, kept so a new page shows it again.
#[derive(Clone)]
struct Toast(Arc<Mutex<String>>);

impl Toast {
    fn show(&self, win: &WebviewWindow, text: Option<&str>) {
        if let Some(text) = text {
            *self.0.lock().unwrap() = text.to_string();
        }
        let _ = win.eval(toast_script(&self.0.lock().unwrap()));
    }
}

/// Centred on the widget's screen, as big as fits up to 980×780. Worked
/// out in that screen's own pixels and applied once the window exists: a
/// logical position given to the builder is scaled by whichever screen
/// the window starts on, which put it on the wrong one.
fn place(app: &AppHandle) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let m = app
        .get_webview_window(crate::ui::WIDGET)
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let (scale, area) = (m.scale_factor(), m.work_area());
    let fit =
        |extent: u32, most: f64| ((extent as f64 - 40.0 * scale).min(most * scale)).max(1.0) as u32;
    let (w, h) = (fit(area.size.width, 980.0), fit(area.size.height, 780.0));
    let x = area.position.x + (area.size.width.saturating_sub(w) / 2) as i32;
    let y = area.position.y + (area.size.height.saturating_sub(h) / 2) as i32;
    Some((PhysicalPosition::new(x, y), PhysicalSize::new(w, h)))
}

fn open(app: &AppHandle) -> tauri::Result<watch::Receiver<Option<bool>>> {
    let (done, outcome) = watch::channel(None);
    let page = Url::parse(KEY_PAGE_URL).expect("a valid URL");
    let toast = Toast(Arc::new(Mutex::new(step_for(&page).to_string())));
    let profile = app.path().app_local_data_dir()?.join("spicylyrics");

    let on_load = toast.clone();
    let popups = app.clone();
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(page.clone()))
        .title("Get your Spicy Lyrics key")
        .inner_size(980.0, 780.0)
        .visible(false)
        .always_on_top(true)
        .background_color(tauri::window::Color(0x11, 0x10, 0x14, 0xff))
        .data_directory(profile)
        .on_page_load(move |w, p| {
            if p.event() == PageLoadEvent::Finished {
                on_load.show(&w, None);
                let _ = w.eval(autofill_script());
            }
        })
        // a link that opens a new window opens here instead
        .on_new_window(move |url, _| {
            if url.scheme() == "https" {
                if let Some(w) = popups.get_webview_window(LABEL) {
                    let _ = w.navigate(url);
                }
            }
            NewWindowResponse::Deny
        })
        .build()?;
    if let Some((pos, size)) = place(app) {
        let _ = win.set_size(size);
        let _ = win.set_position(pos);
    }
    win.show()?;

    let closed = done.clone();
    win.on_window_event(move |e| {
        if let tauri::WindowEvent::Destroyed = e {
            closed.send_if_modified(|v| v.is_none() && v.replace(false).is_none());
            *OPEN.lock().unwrap() = None;
        }
    });
    tauri::async_runtime::spawn(watch_window(app.clone(), win, toast, done));
    Ok(outcome)
}

/// Follows the page and the clipboard until a key is saved or the window
/// closes.
async fn watch_window(
    app: AppHandle,
    win: WebviewWindow,
    toast: Toast,
    done: watch::Sender<Option<bool>>,
) {
    // A failed read (the clipboard busy) is skipped, not taken as empty:
    // otherwise whatever was already there would look newly copied.
    let read = || {
        app.clipboard()
            .read_text()
            .ok()
            .map(|t| t.trim().to_string())
    };
    let mut last_text = read();
    let mut last_url = String::new();
    let mut was_signing_in = false;
    while done.borrow().is_none() {
        tokio::time::sleep(POLL).await;
        if app.get_webview_window(LABEL).is_none() {
            return;
        }
        // The site moves between pages without loading them, so the address
        // is watched rather than the loads. Once signed in, back to the
        // applications page, wherever the sign-in left off.
        if let Ok(url) = win.url() {
            if url.as_str() != last_url {
                last_url = url.to_string();
                if signing_in(&url) {
                    was_signing_in = true;
                } else if was_signing_in {
                    was_signing_in = false;
                    if !url.path().starts_with("/dashboard/applications") {
                        let _ = win.navigate(Url::parse(KEY_PAGE_URL).expect("a valid URL"));
                        continue;
                    }
                }
                toast.show(&win, Some(step_for(&url)));
            }
        }
        let Some(text) = read() else { continue };
        if last_text.as_ref() == Some(&text) {
            continue;
        }
        // the first good read is what was there before
        if last_text.replace(text.clone()).is_none() || !key::looks_like_key(&text) {
            continue;
        }
        toast.show(&win, Some("Checking your key"));
        let checked = key::set(&text).await;
        if !checked.ok {
            toast.show(&win, checked.error.as_deref());
            continue;
        }
        toast.show(&win, Some("Got your key!"));
        done.send_replace(Some(true));
        tokio::time::sleep(CLOSE_AFTER).await;
        let _ = win.close();
    }
}

/// Opens the dashboard (or brings it forward) and waits: ok once a key is
/// checked and saved, not ok if the window closed first.
pub async fn connect(app: &AppHandle) -> Checked {
    let waiting = {
        let mut slot = OPEN.lock().unwrap();
        match slot.as_ref() {
            Some(rx) => {
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.set_focus();
                }
                Ok(rx.clone())
            }
            None => open(app).inspect(|rx| *slot = Some(rx.clone())),
        }
    };
    let mut rx = match waiting {
        Ok(rx) => rx,
        Err(e) => {
            return Checked {
                ok: false,
                error: Some(e.to_string()),
            }
        }
    };
    let saved = rx
        .wait_for(Option::is_some)
        .await
        .map(|v| *v == Some(true))
        .unwrap_or(false);
    Checked {
        ok: saved,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_signing_in_from_the_dashboard() {
        let at = |s: &str| Url::parse(s).unwrap();
        assert!(signing_in(&at(
            "https://developers.spicylyrics.org/sign-in?next=x"
        )));
        assert!(signing_in(&at("https://accounts.google.com/o/oauth2")));
        assert!(!signing_in(&at(KEY_PAGE_URL)));
        assert_eq!(
            step_for(&at(KEY_PAGE_URL)),
            "Create an application, then copy the key to your clipboard"
        );
    }
}
