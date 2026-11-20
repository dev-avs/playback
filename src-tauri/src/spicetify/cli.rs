//! The spicetify command line, behind a trait so the tests can stand in a
//! fake one that edits config-xpui.ini the way the real one does.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

/// `spicetify apply` restarts Spotify and can take a while.
const TIMEOUT: Duration = Duration::from_secs(180);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub trait Cli {
    /// Runs `cli args…`; the error says what failed, with its output.
    fn run(&self, cli: &Path, args: &[&str]) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// The real one.
pub struct Spicetify;

impl Cli for Spicetify {
    async fn run(&self, cli: &Path, args: &[&str]) -> Result<(), String> {
        let what = format!("spicetify {}", args.join(" "));
        // nobody can answer a prompt here: no input, so one takes its default
        // at once instead of waiting out the timeout
        let mut cmd = tokio::process::Command::new(cli);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true);
        let out = tokio::time::timeout(TIMEOUT, cmd.output())
            .await
            .map_err(|_| format!("{what}: timed out"))?
            .map_err(|e| format!("{what}: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
        let detail = [text(&out.stderr), text(&out.stdout)]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| out.status.to_string());
        Err(format!("{what}: {detail}"))
    }
}
