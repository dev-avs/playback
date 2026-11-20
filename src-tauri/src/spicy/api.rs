//! The Spicy Lyrics API, ported from electron/spicy.cjs, which explains each
//! detail at length. In short:
//!
//! - the token route (`/query`) takes the Spotify token in a
//!   `SpicyLyrics-WebAuth` header, an Origin of Spotify's own page, and a
//!   client version that decides how fine the timing it serves is; the
//!   version is asked of the server, since a stale one silently gets
//!   untimed lyrics;
//! - the public route (`/v1/lyrics/{id}`) takes the user's own key and
//!   answers already unpacked.

use crate::support::http::client;
use serde_json::{json, Value};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const API: &str = "https://api.spicylyrics.org";
const CLIENT_ORIGIN: &str = "https://xpui.app.spotify.com";
/// Only until the server first names the current version.
const FALLBACK_VERSION: &str = "6.3.20";
const VERSION_TTL: Duration = Duration::from_secs(15 * 60);
const VERSION_RETRY: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Version {
    value: Option<String>,
    at: Option<Instant>,
    retry_at: Option<Instant>,
}

static VERSION: LazyLock<Mutex<Version>> = LazyLock::new(Mutex::default);
/// One version lookup at a time; the rest wait for its answer.
static ASKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 2 with syllable timing, 1 with line timing, else 0. Reads the packed
/// API data and the unpacked lyrics alike.
pub fn timing_tier(data: &Value) -> u8 {
    if data.is_null() {
        return 0;
    }
    let s = data.to_string();
    if s.contains("\"Syllable\"") {
        2
    } else if s.contains("\"Line\"") {
        1
    } else {
        0
    }
}

fn query(version: &str, token: Option<&str>, body: Value) -> reqwest::RequestBuilder {
    let mut req = client()
        .post(format!("{API}/query"))
        .header("Content-Type", "application/json")
        .header("SpicyLyrics-Version", version)
        .header("X-mode", "2")
        .header("Origin", CLIENT_ORIGIN)
        .json(&body);
    if let Some(token) = token {
        req = req.header("SpicyLyrics-WebAuth", format!("Bearer {token}"));
    }
    req
}

/// The server gives the client's one query the id "0", as a string.
fn job(body: &Value) -> Option<&Value> {
    body["queries"]
        .as_array()?
        .iter()
        .find(|q| match &q["operationId"] {
            Value::String(s) => s == "0",
            Value::Number(n) => n.as_u64() == Some(0),
            _ => false,
        })
}

fn version_in(body: &Value) -> Option<String> {
    let result = &job(body)?["result"];
    if result["httpStatus"] != 200 {
        return None;
    }
    let re = regex::Regex::new(r"\d+\.\d+\.\d+").expect("a valid pattern");
    Some(re.find(result["data"].as_str()?)?.as_str().to_string())
}

async fn ask_version(token: &str, current: &str) -> Result<String, String> {
    let body =
        json!({ "queries": [{ "operation": "ext_version" }], "client": { "version": current } });
    let res = query(current, Some(token).filter(|t| !t.is_empty()), body)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("version lookup HTTP {}", res.status().as_u16()));
    }
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    version_in(&body).ok_or_else(|| "version lookup returned no valid version".into())
}

/// The version to send: asked again after 15 minutes, or at once (`force`)
/// when an answer came back without syllables.
pub async fn client_version(token: &str, force: bool) -> String {
    let fresh = |v: &Version| {
        v.value
            .clone()
            .filter(|_| v.at.is_some_and(|at| at.elapsed() < VERSION_TTL))
    };
    if !force {
        if let Some(v) = fresh(&VERSION.lock().unwrap()) {
            return v;
        }
    }
    let _one = ASKING.lock().await;
    let current = {
        let v = VERSION.lock().unwrap();
        // someone else asked while this one waited
        if let Some(found) = fresh(&v).filter(|_| !force) {
            return found;
        }
        if v.retry_at.is_some_and(|at| Instant::now() < at) {
            return v.value.clone().unwrap_or(FALLBACK_VERSION.into());
        }
        v.value.clone().unwrap_or(FALLBACK_VERSION.into())
    };
    let answer = ask_version(token, &current).await;
    let mut v = VERSION.lock().unwrap();
    match answer {
        Ok(found) => {
            if v.value.as_deref() != Some(&found) {
                eprintln!("[spicy] client version -> {found}");
            }
            *v = Version {
                value: Some(found),
                at: Some(Instant::now()),
                retry_at: None,
            };
        }
        Err(e) => {
            v.retry_at = Some(Instant::now() + VERSION_RETRY);
            eprintln!("[spicy] could not read the current client version: {e}");
        }
    }
    v.value.clone().unwrap_or(FALLBACK_VERSION.into())
}

/// The whole reply to one lyrics query, if it came back 2xx.
async fn ask_lyrics(id: &str, token: &str, version: &str) -> Result<Option<Value>, String> {
    let body = json!({
        "queries": [{ "operation": "lyrics", "variables": { "id": id, "auth": "SpicyLyrics-WebAuth" } }],
        "client": { "version": version },
    });
    let res = query(version, Some(token), body)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        eprintln!("[spicy] query failed: HTTP {}", res.status().as_u16());
        return Ok(None);
    }
    Ok(res.json().await.ok())
}

/// One answer from the token route: 200 found, 404 none, 503 queued.
pub struct Queried {
    pub http_status: u64,
    pub data: Value,
    pub client_version: String,
}

fn result_of(body: &Value) -> Option<&Value> {
    job(body).map(|j| &j["result"]).filter(|r| !r.is_null())
}

pub async fn query_lyrics(id: &str, token: &str) -> Result<Option<Queried>, String> {
    let mut version = client_version(token, false).await;
    let Some(mut body) = ask_lyrics(id, token, &version).await? else {
        return Ok(None);
    };

    // An old version can be served line timing: ask for the current one and
    // retry once, keeping the first answer unless the retry is as fine.
    if let Some(first) =
        result_of(&body).filter(|r| r["httpStatus"] == 200 && timing_tier(&r["data"]) < 2)
    {
        let tier = timing_tier(&first["data"]);
        let refreshed = client_version(token, true).await;
        if refreshed != version {
            eprintln!("[spicy] no syllables on {version}, retrying as {refreshed}");
            match ask_lyrics(id, token, &refreshed).await {
                Ok(Some(retry))
                    if result_of(&retry).is_some_and(|r| {
                        r["httpStatus"] == 200 && timing_tier(&r["data"]) >= tier
                    }) =>
                {
                    version = refreshed;
                    body = retry;
                }
                Ok(_) => {}
                Err(e) => eprintln!("[spicy] version retry failed, keeping the first lyrics: {e}"),
            }
        }
    }

    let Some(result) = result_of(&body) else {
        // Refused requests still answer 200, with only the server's notice.
        let jobs = body["queries"].as_array().cloned().unwrap_or_default();
        let only_notice = !jobs.is_empty()
            && jobs
                .iter()
                .all(|q| !q["_notice"].is_null() && q["result"].is_null());
        eprintln!(
            "{}",
            if only_notice {
                "[spicy] request rejected: the Spotify token was missing or not accepted"
            } else {
                "[spicy] no result for operationId 0"
            }
        );
        return Ok(None);
    };
    Ok(Some(Queried {
        http_status: result["httpStatus"].as_u64().unwrap_or(0),
        data: result["data"].clone(),
        client_version: version,
    }))
}

/// One answer from the public route.
pub struct Keyed {
    pub http_status: u16,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub message: Option<String>,
    /// seconds, from a 429
    pub retry_after: Option<u64>,
}

pub async fn get_lyrics(id: &str, key: &str) -> Result<Keyed, String> {
    let url = format!("{API}/v1/lyrics/{}", urlencoding(id));
    let res = client()
        .get(url)
        .header("Authorization", format!("Bearer {key}"))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    let retry_after = res
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok())
        .filter(|s| *s > 0);
    let body: Value = res.json().await.unwrap_or(Value::Null);
    let body = &body["Body"];
    if status.is_success() {
        return Ok(Keyed {
            http_status: status.as_u16(),
            data: Some(body.clone()),
            error: None,
            message: None,
            retry_after: None,
        });
    }
    let text = |k: &str| body[k].as_str().map(String::from);
    Ok(Keyed {
        http_status: status.as_u16(),
        data: None,
        error: text("error"),
        message: text("message"),
        retry_after,
    })
}

/// Track ids are base62; anything else is escaped rather than trusted.
fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_timing() {
        assert_eq!(timing_tier(&Value::Null), 0);
        assert_eq!(timing_tier(&json!({ "Type": "Static" })), 0);
        assert_eq!(timing_tier(&json!({ "Type": "Line" })), 1);
        assert_eq!(timing_tier(&json!(["x", { "Type": "Syllable" }])), 2);
    }

    #[test]
    fn finds_the_one_job_by_its_string_id() {
        let body = json!({ "queries": [{ "_notice": "n" }, { "operationId": "0", "result": { "httpStatus": 200, "data": "Version 6.4.1 (beta)" } }] });
        assert_eq!(version_in(&body).as_deref(), Some("6.4.1"));
        assert!(version_in(&json!({ "queries": [{ "operationId": "0", "result": { "httpStatus": 500, "data": "6.4.1" } }] })).is_none());
        assert!(result_of(&json!({ "queries": [{ "_notice": "n" }] })).is_none());
    }

    #[test]
    fn escapes_odd_ids() {
        assert_eq!(
            urlencoding("1QV6tiMFM6fSOKOGLMHYYg"),
            "1QV6tiMFM6fSOKOGLMHYYg"
        );
        assert_eq!(urlencoding("a/b"), "a%2Fb");
    }
}
