//! Releases on GitHub, for version control.
//!
//! This build cannot install anything yet, so it reports itself as a "dev"
//! build, as the Electron dev build does: the update window and version
//! control show the releases but offer no install.

use crate::support::http::client;
use crate::support::json::{list_of, str_of};
use serde::Serialize;
use serde_json::{json, Value};

const RELEASES: &str = "https://api.github.com/repos/dev-avs/playback/releases?per_page=100";

/// UpdateState in src/player.d.ts.
pub fn state() -> Value {
    json!({ "status": "dev", "version": null, "downloaded": 0, "total": null, "error": null })
}

/// Release in src/player.d.ts.
#[derive(Serialize)]
pub struct Release {
    version: String,
    date: String,
    notes: String,
    /// before 1.3.7: installed with its full installer, no updater of its own
    legacy: bool,
}

/// Every release with something to install, newest first.
fn installable(body: &Value) -> Vec<Release> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"] != true)
        .filter_map(|r| {
            let version = str_of(r, "tag_name").trim_start_matches('v').to_string();
            let has = |name: &str| list_of(r, "assets").iter().any(|a| str_of(a, "name") == name);
            let legacy = !has("latest.yml");
            if legacy && !has(&format!("player-setup-{version}.exe")) {
                return None;
            }
            Some(Release { date: str_of(r, "published_at").into(), notes: str_of(r, "body").into(), version, legacy })
        })
        .collect()
}

/// `{ ok, list }` or `{ ok: false, error }`, as the page expects.
pub async fn releases() -> Value {
    let found = async {
        let res = client().get(RELEASES).header("Accept", "application/vnd.github+json").send().await?;
        if !res.status().is_success() {
            return Ok(Err(format!("GitHub answered {}", res.status().as_u16())));
        }
        Ok::<_, reqwest::Error>(Ok(installable(&res.json().await?)))
    }
    .await;
    match found {
        Ok(Ok(list)) => json!({ "ok": true, "list": list }),
        Ok(Err(e)) => json!({ "ok": false, "error": e }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn lists_only_what_can_be_installed() {
        let body = json!([
            { "tag_name": "v1.5.7", "published_at": "2026-11-01", "body": "n", "assets": [{ "name": "latest.yml" }] },
            { "tag_name": "v1.3.5", "assets": [{ "name": "player-setup-1.3.5.exe" }] },
            { "tag_name": "v1.3.0", "assets": [] },
            { "tag_name": "v9.9.9", "draft": true, "assets": [{ "name": "latest.yml" }] },
        ]);
        let list = super::installable(&body);
        assert_eq!(list.iter().map(|r| (r.version.as_str(), r.legacy)).collect::<Vec<_>>(), [("1.5.7", false), ("1.3.5", true)]);
    }
}
