//! The one HTTP client every lookup shares.

use std::sync::LazyLock;
use std::time::Duration;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("Mozilla/5.0")
        .build()
        .expect("HTTP client")
});

pub fn client() -> &'static reqwest::Client {
    &CLIENT
}
