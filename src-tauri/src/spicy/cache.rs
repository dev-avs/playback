//! Spicy lyrics on disk, in electron/lyricsCache.cjs' folder and format, so
//! both builds share it: `lyrics-cache/<track id>.json.gz`, each gzipped
//! JSON `{ v: 1, kind, at, data, clientVersion }`.
//!
//! An entry is dropped after 30 days, so a song comes back with the timing
//! it has today; reading one marks it used, and past 10 GB the least
//! recently used go first.

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

const VERSION: u64 = 1;
const MAX_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const REFRESH_AFTER_MS: f64 = 30.0 * 24.0 * 3600.0 * 1000.0;

/// What the resolver keeps: the shape Spicy sent (`packed` from the token
/// route, `lyrics` unpacked), and the client version it was asked with.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: String,
    pub data: Value,
    pub client_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    v: u64,
    kind: String,
    #[serde(default)]
    at: f64,
    data: Value,
    #[serde(rename = "clientVersion", default)]
    client_version: Option<String>,
}

pub struct DiskCache {
    dir: Option<PathBuf>,
    max_bytes: u64,
    total: Mutex<u64>,
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Spotify ids only: nothing else may name a file.
fn safe_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn entries(dir: &Path) -> impl Iterator<Item = (PathBuf, fs::Metadata)> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json.gz"))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?)))
}

impl DiskCache {
    pub fn open(dir: PathBuf, max_bytes: u64) -> DiskCache {
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("[lyrics-cache] unusable, continuing without it: {e}");
            return DiskCache { dir: None, max_bytes, total: Mutex::new(0) };
        }
        let total = entries(&dir).map(|(_, m)| m.len()).sum();
        DiskCache { dir: Some(dir), max_bytes, total: Mutex::new(total) }
    }

    pub fn standard() -> DiskCache {
        DiskCache::open(crate::support::paths::data_file("lyrics-cache"), MAX_BYTES)
    }

    fn file(&self, id: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{id}.json.gz"))).filter(|_| safe_id(id))
    }

    fn remove(&self, file: &Path) {
        let size = fs::metadata(file).map_or(0, |m| m.len());
        if fs::remove_file(file).is_ok() {
            let mut total = self.total.lock().unwrap();
            *total = total.saturating_sub(size);
        }
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        let file = self.file(id)?;
        let raw = fs::read(&file).ok()?;
        let mut text = String::new();
        let parsed = GzDecoder::new(raw.as_slice())
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::from_str::<OnDisk>(&text).map_err(|e| e.to_string()));
        let entry = match parsed {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("[lyrics-cache] discarding unreadable entry {id}: {e}");
                self.remove(&file);
                return None;
            }
        };
        if entry.v != VERSION || entry.kind.is_empty() {
            return None;
        }
        if now_ms() - entry.at > REFRESH_AFTER_MS {
            self.remove(&file);
            return None;
        }
        // marks it used, for eviction
        if let Ok(f) = fs::File::options().write(true).open(&file) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(Entry { kind: entry.kind, data: entry.data, client_version: entry.client_version })
    }

    pub fn put(&self, id: &str, entry: &Entry) {
        let Some(file) = self.file(id) else { return };
        if entry.data.is_null() {
            return;
        }
        let tmp = file.with_extension(format!("gz.{}.tmp", std::process::id()));
        let written = (|| {
            let record = OnDisk {
                v: VERSION,
                kind: entry.kind.clone(),
                at: now_ms(),
                data: entry.data.clone(),
                client_version: entry.client_version.clone(),
            };
            let mut gz = GzEncoder::new(Vec::new(), Compression::default());
            gz.write_all(serde_json::to_string(&record)?.as_bytes())?;
            let body = gz.finish()?;
            let previous = fs::metadata(&file).map_or(0, |m| m.len());
            fs::write(&tmp, &body)?;
            fs::rename(&tmp, &file)?;
            Ok::<_, Box<dyn std::error::Error>>((body.len() as u64, previous))
        })();
        match written {
            Ok((size, previous)) => {
                {
                    let mut total = self.total.lock().unwrap();
                    *total = (*total + size).saturating_sub(previous);
                }
                self.evict();
            }
            Err(e) => {
                eprintln!("[lyrics-cache] could not store {id}: {e}");
                let _ = fs::remove_file(&tmp);
            }
        }
    }

    fn evict(&self) {
        let Some(dir) = &self.dir else { return };
        if *self.total.lock().unwrap() <= self.max_bytes {
            return;
        }
        let mut all: Vec<_> = entries(dir).map(|(p, m)| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), p)).collect();
        all.sort();
        for (_, file) in all {
            if *self.total.lock().unwrap() <= self.max_bytes {
                break;
            }
            self.remove(&file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("playback-cache-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn entry(n: u64) -> Entry {
        Entry { kind: "packed".into(), data: json!({ "n": n, "pad": "x".repeat(400) }), client_version: Some("6.3.20".into()) }
    }

    #[test]
    fn keeps_and_reads_back_in_electrons_format() {
        let dir = temp("format");
        let cache = DiskCache::open(dir.clone(), MAX_BYTES);
        cache.put("abc123", &entry(1));
        assert_eq!(cache.get("abc123"), Some(entry(1)));
        let mut text = String::new();
        GzDecoder::new(fs::read(dir.join("abc123.json.gz")).unwrap().as_slice()).read_to_string(&mut text).unwrap();
        let raw: Value = serde_json::from_str(&text).unwrap();
        assert_eq!((raw["v"].as_u64(), raw["kind"].as_str(), raw["clientVersion"].as_str()), (Some(1), Some("packed"), Some("6.3.20")));
        assert!(cache.get("../abc123").is_none());
        assert!(cache.get("missing").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn drops_old_and_broken_entries() {
        let dir = temp("old");
        let cache = DiskCache::open(dir.clone(), MAX_BYTES);
        let old = OnDisk { v: 1, kind: "lyrics".into(), at: now_ms() - REFRESH_AFTER_MS - 1.0, data: json!(1), client_version: None };
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(serde_json::to_string(&old).unwrap().as_bytes()).unwrap();
        fs::write(dir.join("old.json.gz"), gz.finish().unwrap()).unwrap();
        fs::write(dir.join("broken.json.gz"), b"not gzip").unwrap();
        assert!(cache.get("old").is_none() && !dir.join("old.json.gz").exists());
        assert!(cache.get("broken").is_none() && !dir.join("broken.json.gz").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn evicts_the_least_recently_used_past_its_budget() {
        let dir = temp("evict");
        let probe = DiskCache::open(dir.clone(), MAX_BYTES);
        probe.put("probe", &entry(0));
        let one = fs::metadata(dir.join("probe.json.gz")).unwrap().len();
        fs::remove_file(dir.join("probe.json.gz")).unwrap();

        let cache = DiskCache::open(dir.clone(), one * 2 + one / 2);
        cache.put("a", &entry(1));
        std::thread::sleep(std::time::Duration::from_millis(30));
        cache.put("b", &entry(2));
        std::thread::sleep(std::time::Duration::from_millis(30));
        cache.get("a"); // a is now the most recent
        std::thread::sleep(std::time::Duration::from_millis(30));
        cache.put("c", &entry(3));
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
        let _ = fs::remove_dir_all(dir);
    }
}
