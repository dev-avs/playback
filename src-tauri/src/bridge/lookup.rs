//! Spotify ids for songs another app plays (Apple Music, a browser), which
//! only name a title and artist. The Spicetify extension searches Spotify
//! for them; this picks the right hit and remembers it. Ported from
//! electron/spotifyLookup.cjs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use regex::Regex;
use std::sync::{LazyLock, Mutex};

/// A song Spotify does not have is not searched for again for a week.
const MISS_TTL_MS: f64 = 7.0 * 24.0 * 3600.0 * 1000.0;
const MAX_DURATION_DIFF_MS: f64 = 10_000.0;

/// What the extension is asked to find.
#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct Wanted {
    pub title: String,
    pub artist: String,
    /// seconds, as the media session reports it
    #[serde(skip)]
    pub duration: Option<f64>,
}

/// One search hit.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub artists: Vec<String>,
    pub duration_ms: Option<f64>,
}

/// Letters and digits, lowercased, with "(feat. …)", "[Remastered]" and
/// " - Live" cut.
fn norm(s: &str) -> String {
    static SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\s+-\s+|\s*[(\[]).*$").expect("a valid pattern"));
    SUFFIX.replace(&s.to_lowercase(), "").chars().filter(|c| c.is_alphanumeric()).collect()
}

fn title_matches(a: &str, b: &str) -> bool {
    let (x, y) = (norm(a), norm(b));
    !x.is_empty() && !y.is_empty() && (x == y || x.contains(&y) || y.contains(&x))
}

fn artist_matches(playing: &str, artists: &[String]) -> bool {
    let p = norm(playing);
    !p.is_empty()
        && artists.iter().any(|a| {
            let n = norm(a);
            !n.is_empty() && (p.contains(&n) || n.contains(&p))
        })
}

/// The hit with the same title and artist, closest in length (within ten
/// seconds); without lengths, Spotify's own first.
pub fn pick(wanted: &Wanted, candidates: &[Candidate]) -> Option<String> {
    let want_ms = wanted.duration.filter(|d| *d > 0.0).map(|d| d * 1000.0);
    let mut best: Option<&str> = None;
    let mut best_diff = f64::INFINITY;
    for c in candidates {
        let Some(id) = c.id.as_deref() else { continue };
        if !title_matches(c.name.as_deref().unwrap_or(""), &wanted.title) || !artist_matches(&wanted.artist, &c.artists) {
            continue;
        }
        match (want_ms, c.duration_ms.filter(|d| *d > 0.0)) {
            (Some(want), Some(have)) => {
                let diff = (have - want).abs();
                if diff <= MAX_DURATION_DIFF_MS && diff < best_diff {
                    best = Some(id);
                    best_diff = diff;
                }
            }
            _ => {
                best = best.or(Some(id));
            }
        }
    }
    best.map(String::from)
}

#[derive(Serialize, Deserialize, Clone)]
struct Known {
    id: Option<String>,
    at: f64,
}

/// Title and artist → Spotify id, in `spotify-ids.json`. A miss is kept as
/// id null.
pub struct IdCache {
    file: PathBuf,
    entries: Mutex<HashMap<String, Known>>,
}

fn key_of(artist: &str, title: &str) -> String {
    format!("{}\u{0}{}", artist.trim().to_lowercase(), title.trim().to_lowercase())
}

impl IdCache {
    pub fn open(file: PathBuf) -> IdCache {
        let entries = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        IdCache { file, entries: Mutex::new(entries) }
    }

    /// Some(Some(id)) known, Some(None) known miss, None never looked up.
    pub fn get(&self, artist: &str, title: &str, now: f64) -> Option<Option<String>> {
        let e = self.entries.lock().unwrap().get(&key_of(artist, title)).cloned()?;
        if e.id.is_none() && now - e.at > MISS_TTL_MS {
            return None;
        }
        Some(e.id)
    }

    pub fn set(&self, artist: &str, title: &str, id: Option<String>, now: f64) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(key_of(artist, title), Known { id, at: now });
        if let Err(e) = std::fs::write(&self.file, serde_json::to_string(&*entries).expect("plain JSON")) {
            eprintln!("[lookup] could not persist ids: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, name: &str, artist: &str, ms: Option<f64>) -> Candidate {
        Candidate { id: Some(id.into()), name: Some(name.into()), artists: vec![artist.into()], duration_ms: ms }
    }

    #[test]
    fn picks_the_closest_matching_hit() {
        let wanted = Wanted { title: "Song - Live".into(), artist: "Band feat. Guest".into(), duration: Some(200.0) };
        let hits = [
            hit("other", "Different", "Band", Some(200_000.0)),
            hit("far", "Song", "Band", Some(230_000.0)),
            hit("close", "Song (Live)", "Band", Some(201_000.0)),
            hit("closer", "Song", "Band", Some(200_500.0)),
        ];
        assert_eq!(pick(&wanted, &hits).as_deref(), Some("closer"));
        assert_eq!(pick(&wanted, &hits[..2]), None);
        let untimed = Wanted { duration: None, ..wanted };
        assert_eq!(pick(&untimed, &hits).as_deref(), Some("far"));
    }

    #[test]
    fn remembers_hits_and_misses() {
        let file = std::env::temp_dir().join(format!("playback-ids-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let ids = IdCache::open(file.clone());
        assert_eq!(ids.get("A", "T", 0.0), None);
        ids.set("A", "T", Some("id1".into()), 0.0);
        ids.set("B", "U", None, 0.0);
        let again = IdCache::open(file.clone());
        assert_eq!(again.get(" a ", "t", 1.0), Some(Some("id1".into())));
        assert_eq!(again.get("B", "U", 1.0), Some(None));
        assert_eq!(again.get("B", "U", MISS_TTL_MS + 1.0), None);
        let _ = std::fs::remove_file(file);
    }
}
