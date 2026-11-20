//! Downloads to disk, counted as they come in and checked against the
//! feed's SHA-512 before anything runs them.

use base64::Engine;
use sha2::{Digest, Sha512};
use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

/// The shared client gives a whole request 8 s; a package is ~120 MB, so
/// this one only gives up when the connection stalls.
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .user_agent("Mozilla/5.0")
        .build()
        .expect("HTTP client")
});

/// Downloads `url` to `to`. `progress` gets each chunk's size, and once the
/// whole size when the server says it. With `sha512`, a file that does not
/// match is deleted and refused.
pub async fn fetch(url: &str, to: &Path, sha512: Option<&str>, mut progress: impl FnMut(Progress)) -> Result<(), String> {
    let mut res = CLIENT.get(url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("GitHub answered {}", res.status().as_u16()));
    }
    if let Some(n) = res.content_length() {
        progress(Progress::Size(n));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(to).map_err(|e| format!("could not write {}: {e}", to.display()))?;
    let mut hash = Sha512::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        hash.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        progress(Progress::Got(chunk.len() as u64));
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if let Some(expected) = sha512 {
        let got = base64::engine::general_purpose::STANDARD.encode(hash.finalize());
        if got != expected {
            let _ = std::fs::remove_file(to);
            return Err(format!("{} did not download intact", to.file_name().unwrap_or_default().to_string_lossy()));
        }
    }
    Ok(())
}

pub enum Progress {
    /// the file's whole size
    Size(u64),
    /// bytes just received
    Got(u64),
}
