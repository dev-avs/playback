//! What the Spotify client last said it was playing, pushed by the
//! Spicetify extension. Exact, so it beats searching by name, as long as it
//! is plainly the song the media session reports.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How long a pushed track is trusted without a fresh push.
pub const TTL_MS: f64 = 15.0 * 60_000.0;

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct Next {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub artist: String,
    pub image: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Track {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub artists: Vec<String>,
    pub image: Option<String>,
    pub next: Option<Next>,
    /// when it was pushed, epoch ms
    #[serde(default)]
    pub at: f64,
}

/// The same title, give or take decoration; a track with no name matches
/// nothing.
pub fn loose_match(name: Option<&str>, title: &str) -> bool {
    name.is_some_and(|n| crate::support::text::loose_match(n, title))
}

impl Track {
    pub fn fresh(&self, now: f64) -> bool {
        now - self.at <= TTL_MS
    }

    /// The Spotify id for the session's title: this track, or the queued one
    /// (on a change the session names the new song before the next push).
    /// A wrong id fetches confidently wrong lyrics, so no match is None.
    pub fn id_for(&self, title: &str, now: f64) -> Option<&str> {
        if !self.fresh(now) {
            return None;
        }
        if loose_match(self.name.as_deref(), title) {
            return Some(&self.id);
        }
        self.next
            .as_ref()
            .filter(|n| loose_match(n.name.as_deref(), title))
            .map(|n| n.id.as_str())
    }

    /// What plays after the session's song, if this is that song.
    pub fn next_for(&self, title: &str, now: f64) -> Option<Value> {
        self.id_for(title, now)?;
        let n = self.next.as_ref().filter(|n| n.id != self.id)?;
        Some(json!({ "id": n.id, "name": n.name, "artist": n.artist, "image": n.image }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: "A".into(),
            name: Some("Song (feat. Someone)".into()),
            artists: vec![],
            image: None,
            next: Some(Next {
                id: "B".into(),
                name: Some("Other [Remastered]".into()),
                artist: "X".into(),
                image: None,
            }),
            at: 1000.0,
        }
    }

    #[test]
    fn matches_titles_loosely() {
        assert!(loose_match(Some("Song (feat. Someone)"), "song"));
        assert!(loose_match(Some("Don't Stop"), "Dont Stop - Live"));
        assert!(!loose_match(Some("Song"), ""));
        assert!(!loose_match(None, "Song"));
        assert!(loose_match(Some("夜に駆ける"), "夜に駆ける"));
    }

    #[test]
    fn finds_this_song_or_the_queued_one() {
        let t = track();
        assert_eq!(t.id_for("Song", 2000.0), Some("A"));
        assert_eq!(t.id_for("Other", 2000.0), Some("B"));
        assert_eq!(t.id_for("Unrelated", 2000.0), None);
        assert_eq!(t.id_for("Song", 1000.0 + TTL_MS + 1.0), None);
        assert_eq!(t.next_for("Song", 2000.0).unwrap()["id"], "B");
    }
}
