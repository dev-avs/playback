//! What GitHub has: each release's latest.yml (electron-builder's update
//! feed, which every release carries), and the list of releases for version
//! control.

use crate::support::http::client;
use crate::support::json::{list_of, str_of};
use serde::Serialize;
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Was dev-avs/player up to 1.4.2; GitHub redirects the old name.
const REPO: &str = "https://github.com/dev-avs/playback";
const RELEASES: &str = "https://api.github.com/repos/dev-avs/playback/releases?per_page=100";

/// One file of a release, as latest.yml names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub file: String,
    /// base64 SHA-512 of the file
    pub sha512: String,
    pub size: Option<u64>,
}

/// A release's latest.yml: the web installer's stub, and the package it
/// would otherwise download itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Feed {
    pub version: String,
    pub stub: Asset,
    pub package: Asset,
}

impl Feed {
    pub fn url(&self, asset: &Asset) -> String {
        format!("{REPO}/releases/download/v{}/{}", self.version, asset.file)
    }
}

/// Reads the few fields it needs out of latest.yml: the top-level version,
/// path and sha512, and `packages: x64:`.
pub fn parse(yml: &str) -> Option<Feed> {
    let mut top: HashMap<&str, String> = HashMap::new();
    let mut package: HashMap<&str, String> = HashMap::new();
    let mut section = "";
    let mut in_x64 = false;
    for line in yml.lines() {
        let indent = line.len() - line.trim_start().len();
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        match indent {
            0 => {
                section = key;
                in_x64 = false;
                if !value.is_empty() {
                    top.insert(key, value);
                }
            }
            2 if section == "packages" => in_x64 = key == "x64",
            4 if section == "packages" && in_x64 => {
                package.insert(key, value);
            }
            _ => {}
        }
    }
    Some(Feed {
        version: top.remove("version")?,
        stub: Asset {
            file: top.remove("path")?,
            sha512: top.remove("sha512")?,
            size: None,
        },
        package: Asset {
            file: package.remove("path")?,
            sha512: package.remove("sha512")?,
            size: package.get("size").and_then(|s| s.parse().ok()),
        },
    })
}

/// The newest release's feed, or one version's own.
pub async fn fetch(version: Option<&str>) -> Result<Feed, String> {
    let url = match version {
        Some(v) => format!("{REPO}/releases/download/v{v}/latest.yml"),
        None => format!("{REPO}/releases/latest/download/latest.yml"),
    };
    let res = client().get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(match version {
            Some(v) => format!("v{v} is not on GitHub"),
            None => format!("GitHub answered {}", res.status().as_u16()),
        });
    }
    parse(&res.text().await.map_err(|e| e.to_string())?)
        .ok_or_else(|| "the release's latest.yml could not be read".into())
}

/// Dotted versions compared part by part, as numbers, then by the flavor
/// revision after a dash: 1.6.0 < 1.6.0-a < 1.6.0-b < 1.6.0-aa < 1.6.1.
pub fn compare(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| {
        let (number, revision) = v.split_once('-').unwrap_or((v, ""));
        let number = number
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>();
        (number, revision.len(), revision.to_string())
    };
    parts(a).cmp(&parts(b))
}

/// Release in src/types/player.d.ts.
#[derive(Serialize)]
pub struct Release {
    version: String,
    date: String,
    notes: String,
    /// before 1.3.7: installed with its full installer, no updater of its own
    legacy: bool,
    /// available flavors in this release (e.g. ["tauri", "electron", "gpui"])
    #[serde(skip_serializing_if = "Vec::is_empty")]
    flavors: Vec<String>,
    /// each flavor's package (.nsis.7z) size in bytes
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    sizes: HashMap<String, u64>,
}

pub type FlavorUrls = HashMap<(String, String), (String, String)>;

/// Every release with something to install, newest first, plus legacy
/// installers and per-flavor (stub, package) URLs.
fn installable(body: &Value) -> (Vec<Release>, HashMap<String, String>, FlavorUrls) {
    let mut legacy_installers = HashMap::new();
    let mut flavor_urls: FlavorUrls = HashMap::new();
    let list = body
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"] != true)
        .filter_map(|r| {
            let version = str_of(r, "tag_name").trim_start_matches('v').to_string();
            let assets = list_of(r, "assets");
            let asset = |name: &str| {
                assets
                    .iter()
                    .find(|a| str_of(a, "name") == name)
                    .map(|a| str_of(a, "browser_download_url").to_string())
            };
            let size = |name: &str| {
                assets
                    .iter()
                    .find(|a| str_of(a, "name") == name)
                    .and_then(|a| a["size"].as_u64())
                    .unwrap_or(0)
            };
            let legacy = asset("latest.yml").is_none();
            if legacy {
                legacy_installers.insert(
                    version.clone(),
                    asset(&format!("player-setup-{version}.exe"))?,
                );
            }
            let mut flavors = Vec::new();
            let mut sizes = HashMap::new();
            if !legacy {
                // Default package (Tauri from 1.5.9+)
                if let (Some(stub), Some(pkg)) = (
                    asset(&format!("playback-setup-{version}.exe")),
                    asset(&format!("playback-{version}-x64.nsis.7z")),
                ) {
                    let pkg_size = size(&format!("playback-{version}-x64.nsis.7z"));
                    let id = if pkg_size > 0 && pkg_size < 25 * 1024 * 1024 {
                        "tauri"
                    } else {
                        "electron"
                    };
                    flavors.push(id.to_string());
                    flavor_urls.insert((version.clone(), id.to_string()), (stub, pkg));
                    if pkg_size > 0 {
                        sizes.insert(id.to_string(), pkg_size);
                    }
                }
                for id in ["tauri", "electron", "gpui"] {
                    if let (Some(stub), Some(pkg)) = (
                        asset(&format!("playback-{id}-setup-{version}.exe")),
                        asset(&format!("playback-{id}-{version}-x64.nsis.7z")),
                    ) {
                        if !flavors.iter().any(|f| f == id) {
                            flavors.push(id.to_string());
                        }
                        flavor_urls.insert((version.clone(), id.to_string()), (stub, pkg));
                        let pkg_size = size(&format!("playback-{id}-{version}-x64.nsis.7z"));
                        if pkg_size > 0 {
                            sizes.insert(id.to_string(), pkg_size);
                        }
                    }
                }
            }
            Some(Release {
                date: str_of(r, "published_at").into(),
                notes: str_of(r, "body").into(),
                version,
                legacy,
                flavors,
                sizes,
            })
        })
        .collect();
    (list, legacy_installers, flavor_urls)
}

/// `{ ok, list }` or `{ ok: false, error }` for the page, legacy installers,
/// and per-flavor asset URLs.
pub async fn releases() -> (Value, HashMap<String, String>, FlavorUrls) {
    let found = async {
        let res = client()
            .get(RELEASES)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("GitHub answered {}", res.status().as_u16()));
        }
        Ok(installable(&res.json().await.map_err(|e| e.to_string())?))
    }
    .await;
    match found {
        Ok((list, legacy, flavors)) => (json!({ "ok": true, "list": list }), legacy, flavors),
        Err(e) => (
            json!({ "ok": false, "error": e }),
            HashMap::new(),
            HashMap::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YML: &str = "version: 1.5.7
files:
  - url: playback-setup-1.5.7.exe
    sha512: STUB==
path: playback-setup-1.5.7.exe
sha512: STUB==
packages:
  x64:
    size: 118739429
    sha512: PKG==
    blockMapSize: 124440
    path: playback-1.5.7-x64.nsis.7z
    file: playback-1.5.7-x64.nsis.7z
releaseDate: '2026-11-20T02:08:25.674Z'
";

    #[test]
    fn reads_the_feed() {
        let f = parse(YML).unwrap();
        assert_eq!(f.version, "1.5.7");
        assert_eq!(
            f.stub,
            Asset {
                file: "playback-setup-1.5.7.exe".into(),
                sha512: "STUB==".into(),
                size: None
            }
        );
        assert_eq!(
            f.package,
            Asset {
                file: "playback-1.5.7-x64.nsis.7z".into(),
                sha512: "PKG==".into(),
                size: Some(118739429)
            }
        );
        assert_eq!(f.url(&f.package), "https://github.com/dev-avs/playback/releases/download/v1.5.7/playback-1.5.7-x64.nsis.7z");
        assert!(parse("version: 1.0.0\n").is_none());
    }

    #[test]
    fn compares_versions_as_numbers() {
        assert_eq!(compare("1.5.10", "1.5.9"), Ordering::Greater);
        assert_eq!(compare("1.5.8", "1.5.8"), Ordering::Equal);
        assert_eq!(compare("1.4.9", "1.5.0"), Ordering::Less);
        assert_eq!(compare("1.6.0-a", "1.6.0"), Ordering::Greater);
        assert_eq!(compare("1.6.0-b", "1.6.0-a"), Ordering::Greater);
        assert_eq!(compare("1.6.0-aa", "1.6.0-z"), Ordering::Greater);
        assert_eq!(compare("1.6.1", "1.6.0-b"), Ordering::Greater);
        assert_eq!(compare("1.6.0-a", "1.6.0-a"), Ordering::Equal);
    }

    #[test]
    fn lists_only_what_can_be_installed() {
        let body = json!([
            { "tag_name": "v1.5.7", "published_at": "2026-11-01", "body": "n", "assets": [{ "name": "latest.yml" }] },
            { "tag_name": "v1.3.5", "assets": [{ "name": "player-setup-1.3.5.exe", "browser_download_url": "https://x/1.3.5" }] },
            { "tag_name": "v1.3.0", "assets": [] },
            { "tag_name": "v9.9.9", "draft": true, "assets": [{ "name": "latest.yml" }] },
        ]);
        let (list, legacy, _) = installable(&body);
        assert_eq!(
            list.iter()
                .map(|r| (r.version.as_str(), r.legacy))
                .collect::<Vec<_>>(),
            [("1.5.7", false), ("1.3.5", true)]
        );
        assert_eq!(legacy["1.3.5"], "https://x/1.3.5");
        assert!(serde_json::to_value(&list[0]).unwrap().get("sizes").is_none());
    }

    #[test]
    fn gives_each_flavors_package_size() {
        let files = |v: &str, tauri: u64, electron: u64| {
            json!([
                { "name": "latest.yml" },
                { "name": format!("playback-setup-{v}.exe") },
                { "name": format!("playback-{v}-x64.nsis.7z"), "size": tauri },
                { "name": format!("playback-electron-setup-{v}.exe") },
                { "name": format!("playback-electron-{v}-x64.nsis.7z"), "size": electron },
            ])
        };
        let body = json!([
            { "tag_name": "v1.6.3-a", "assets": files("1.6.3-a", 10327004, 116128404) },
            { "tag_name": "v1.6.3", "assets": files("1.6.3", 10326092, 116128404) },
        ]);
        let (list, _, _) = installable(&body);
        let sizes = list
            .iter()
            .map(|r| serde_json::to_value(r).unwrap()["sizes"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            sizes,
            [
                json!({ "tauri": 10327004, "electron": 116128404 }),
                json!({ "tauri": 10326092, "electron": 116128404 }),
            ]
        );
    }
}
