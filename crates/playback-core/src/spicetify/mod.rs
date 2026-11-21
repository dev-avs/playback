//! Gets the Spicetify bridge into Spotify, and keeps it current. Ported
//! from electron/spicetifySetup.cjs.
//!
//! The bridge (`spicetify/player-bridge.js`) runs inside Spotify and hands
//! over what the widget cannot read itself. For it to run, four things must
//! hold, and on a new PC none of them do:
//!   1. Spicetify is installed (only the user can do that: it modifies Spotify);
//!   2. the bridge is in Spicetify's Extensions folder, as the current version;
//!   3. it is listed in config-xpui.ini (`spicetify config extensions …`);
//!   4. Spicetify has been applied since, putting a copy into Spotify itself.
//!
//! `check` reports each; `ensure` does 2 and 3, refreshes an outdated copy
//! in place, and does 4 only when allowed, because `spicetify apply`
//! restarts Spotify and so cuts off whatever is playing. Before 1.3.3 the
//! bridge was called player-token.js; `ensure` removes that name too.

mod cli;
pub mod setup;

pub use cli::{Cli, Spicetify};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const BRIDGE: &str = "player-bridge.js";
/// Its name before 1.3.3.
const LEGACY: [&str; 1] = ["player-token.js"];
/// The bridge as this build ships it.
pub const SOURCE: &[u8] = include_bytes!("../../../../spicetify/player-bridge.js");

/// Where Spicetify and Spotify keep things, per their installers' defaults.
pub struct Paths {
    cli: PathBuf,
    config_dir: PathBuf,
    spotify_exe: PathBuf,
    applied_dir: PathBuf,
}

impl Paths {
    pub fn of_this_pc() -> Paths {
        let env = |k| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
        let (local, roaming) = (env("LOCALAPPDATA"), env("APPDATA"));
        Paths {
            cli: find_cli(local.join("spicetify").join("spicetify.exe")),
            config_dir: roaming.join("spicetify"),
            spotify_exe: roaming.join("Spotify").join("Spotify.exe"),
            applied_dir: roaming
                .join("Spotify")
                .join("Apps")
                .join("xpui")
                .join("extensions"),
        }
    }

    fn extension(&self, name: &str) -> PathBuf {
        self.config_dir.join("Extensions").join(name)
    }

    fn in_spotify(&self, name: &str) -> PathBuf {
        self.applied_dir.join(name)
    }
}

/// The official installer uses %LOCALAPPDATA%\spicetify; scoop and winget
/// only put it on PATH.
fn find_cli(preferred: PathBuf) -> PathBuf {
    if preferred.exists() {
        return preferred;
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("spicetify.exe"))
        .find(|exe| exe.exists())
        .unwrap_or(preferred)
}

/// The extensions listed in config-xpui.ini.
fn registered(config_dir: &Path) -> Vec<String> {
    let Ok(ini) = fs::read_to_string(config_dir.join("config-xpui.ini")) else {
        return vec![];
    };
    ini.lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "extensions").then(|| {
                value
                    .split('|')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn same_as(file: &Path, source: &[u8]) -> bool {
    fs::read(file).is_ok_and(|bytes| bytes == source)
}

fn write(file: &Path, source: &[u8]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(file, source).map_err(|e| format!("could not write {}: {e}", file.display()))
}

fn delete(file: &Path) -> bool {
    fs::remove_file(file).is_ok()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub spotify: bool,
    pub spicetify: bool,
    pub bridge_copied: bool,
    pub registered: bool,
    pub applied: bool,
    pub applied_current: bool,
    pub legacy: Vec<String>,
}

impl Status {
    /// The current bridge is live in Spotify and nothing old is left behind.
    pub fn ready(&self) -> bool {
        self.spotify
            && self.spicetify
            && self.bridge_copied
            && self.registered
            && self.applied_current
            && self.legacy.is_empty()
    }
}

pub fn check(source: &[u8], paths: &Paths) -> Status {
    let list = registered(&paths.config_dir);
    let in_spotify = paths.in_spotify(BRIDGE);
    Status {
        spotify: paths.spotify_exe.exists(),
        spicetify: paths.cli.exists(),
        bridge_copied: same_as(&paths.extension(BRIDGE), source),
        registered: list.iter().any(|n| n == BRIDGE),
        applied: in_spotify.exists(),
        applied_current: same_as(&in_spotify, source),
        legacy: LEGACY
            .iter()
            .filter(|n| {
                list.iter().any(|l| l == *n)
                    || paths.extension(n).exists()
                    || paths.in_spotify(n).exists()
            })
            .map(|n| n.to_string())
            .collect(),
    }
}

/// What `ensure` did and where it left things. `needs_apply`: everything is
/// set but Spotify has not picked it up, because applying (a Spotify
/// restart) was not allowed this time.
#[derive(Debug)]
pub struct Ensured {
    pub status: Status,
    pub actions: Vec<String>,
    pub needs_apply: bool,
}

/// Puts the bridge in place as far as allowed.
pub async fn ensure(
    source: &[u8],
    allow_apply: bool,
    paths: &Paths,
    cli: &impl Cli,
) -> Result<Ensured, String> {
    let mut actions = vec![];
    let s = check(source, paths);
    if !s.spicetify {
        return Ok(Ensured {
            status: s,
            actions,
            needs_apply: false,
        });
    }

    let mut changed_list = false;
    for name in &s.legacy {
        delete(&paths.extension(name));
        delete(&paths.in_spotify(name));
        if registered(&paths.config_dir).contains(name) {
            cli.run(&paths.cli, &["config", "extensions", &format!("{name}-")])
                .await?;
            changed_list = true;
        }
        actions.push(format!("Removed the old {name}"));
    }

    if !s.bridge_copied {
        write(&paths.extension(BRIDGE), source)?;
        actions.push("Copied the bridge into Spicetify".into());
    }

    if !s.registered {
        cli.run(&paths.cli, &["config", "extensions", BRIDGE])
            .await?;
        changed_list = true;
        actions.push("Turned the bridge on in Spicetify".into());
    }

    // Spotify already has a copy, just an older one: overwrite it. Spotify
    // reads it on its next start, so no restart is forced for an update.
    let in_spotify = paths.in_spotify(BRIDGE);
    if s.applied && !s.applied_current {
        write(&in_spotify, source)?;
        actions.push("Updated the bridge inside Spotify".into());
    }

    let needs_apply = changed_list || !in_spotify.exists();
    if needs_apply && allow_apply {
        if cli.run(&paths.cli, &["apply"]).await.is_err() {
            // a Spicetify that has never been applied needs its backup first
            cli.run(&paths.cli, &["backup", "apply"]).await?;
        }
        actions.push("Applied Spicetify (Spotify restarted)".into());
    }

    Ok(Ensured {
        status: check(source, paths),
        actions,
        needs_apply: needs_apply && !allow_apply,
    })
}

/// Takes the bridge (and any old name of it) back out of Spicetify and
/// Spotify, for uninstalling. Applies only when something was registered,
/// since that is what restarts Spotify. Returns what was done.
pub async fn remove(paths: &Paths, cli: &impl Cli) -> Result<Vec<String>, String> {
    let mut actions = vec![];
    let mut changed_list = false;
    for name in std::iter::once(BRIDGE).chain(LEGACY) {
        for file in [paths.extension(name), paths.in_spotify(name)] {
            if delete(&file) {
                actions.push(format!("Deleted {}", file.display()));
            }
        }
        if paths.cli.exists() && registered(&paths.config_dir).iter().any(|n| n == name) {
            cli.run(&paths.cli, &["config", "extensions", &format!("{name}-")])
                .await?;
            changed_list = true;
            actions.push(format!("Turned {name} off in Spicetify"));
        }
    }
    if changed_list {
        cli.run(&paths.cli, &["apply"]).await?;
        actions.push("Applied Spicetify (Spotify restarted)".into());
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const SOURCE: &[u8] = b"// bridge v2\n";

    /// A pretend machine: Spotify, an optional Spicetify, and a fake
    /// `spicetify` that edits config-xpui.ini and "applies" as the real one.
    struct Machine {
        root: PathBuf,
        paths: Paths,
        calls: Mutex<Vec<String>>,
        backed_up: Mutex<bool>,
    }

    #[derive(Default)]
    struct Setup<'a> {
        no_spicetify: bool,
        extensions: &'a [&'a str],
        files: &'a [(&'a str, &'a [u8])],
        applied: &'a [(&'a str, &'a [u8])],
        apply_fails_until_backup: bool,
    }

    impl Machine {
        fn new(name: &str, s: Setup) -> Machine {
            let root = std::env::temp_dir()
                .join(format!("playback-spicetify-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let paths = Paths {
                cli: root.join("local").join("spicetify.exe"),
                config_dir: root.join("roaming").join("spicetify"),
                spotify_exe: root.join("roaming").join("Spotify").join("Spotify.exe"),
                applied_dir: root
                    .join("roaming")
                    .join("Spotify")
                    .join("Apps")
                    .join("xpui")
                    .join("extensions"),
            };
            write(&paths.spotify_exe, b"").unwrap();
            let m = Machine {
                root,
                paths,
                calls: Mutex::default(),
                backed_up: Mutex::new(!s.apply_fails_until_backup),
            };
            if !s.no_spicetify {
                write(&m.paths.cli, b"").unwrap();
                m.save(
                    &s.extensions
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>(),
                );
            }
            for (n, body) in s.files {
                write(&m.paths.extension(n), body).unwrap();
            }
            for (n, body) in s.applied {
                write(&m.paths.in_spotify(n), body).unwrap();
            }
            m
        }

        fn save(&self, list: &[String]) {
            write(
                &self.paths.config_dir.join("config-xpui.ini"),
                format!(
                    "[AdditionalOptions]\nextensions            = {}\n",
                    list.join("|")
                )
                .as_bytes(),
            )
            .unwrap();
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    impl Cli for Machine {
        async fn run(&self, cli: &Path, args: &[&str]) -> Result<(), String> {
            assert_eq!(cli, self.paths.cli);
            self.calls.lock().unwrap().push(args.join(" "));
            let mut list = registered(&self.paths.config_dir);
            match args {
                ["config", "extensions", name] => {
                    match name.strip_suffix('-') {
                        Some(gone) => list.retain(|n| n != gone),
                        None if !list.iter().any(|n| n == name) => list.push(name.to_string()),
                        None => {}
                    }
                    self.save(&list);
                }
                ["apply"] | ["backup", "apply"] => {
                    let mut backed_up = self.backed_up.lock().unwrap();
                    if args == ["apply"] && !*backed_up {
                        return Err("backup first".into());
                    }
                    *backed_up = true;
                    let _ = fs::remove_dir_all(&self.paths.applied_dir);
                    for n in &list {
                        if let Ok(body) = fs::read(self.paths.extension(n)) {
                            write(&self.paths.in_spotify(n), &body).unwrap();
                        }
                    }
                }
                _ => panic!("unexpected spicetify {args:?}"),
            }
            Ok(())
        }
    }

    fn block<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn without_spicetify_changes_nothing() {
        let m = Machine::new(
            "none",
            Setup {
                no_spicetify: true,
                ..Default::default()
            },
        );
        let r = block(ensure(SOURCE, true, &m.paths, &m)).unwrap();
        assert!(!r.status.spicetify && r.status.spotify);
        assert!(m.calls().is_empty());
    }

    #[test]
    fn fresh_spicetify_waits_to_apply_until_allowed() {
        let m = Machine::new("fresh", Setup::default());
        let r = block(ensure(SOURCE, false, &m.paths, &m)).unwrap();
        assert!(r.status.bridge_copied && r.status.registered && r.needs_apply);
        assert_eq!(m.calls(), [format!("config extensions {BRIDGE}")]);
    }

    #[test]
    fn first_run_applies_and_the_bridge_is_live() {
        let m = Machine::new("first", Setup::default());
        let r = block(ensure(SOURCE, true, &m.paths, &m)).unwrap();
        assert!(!r.needs_apply && r.status.ready());
        assert_eq!(
            m.calls(),
            [format!("config extensions {BRIDGE}"), "apply".into()]
        );
    }

    #[test]
    fn a_never_applied_spicetify_backs_up_first() {
        let m = Machine::new(
            "backup",
            Setup {
                apply_fails_until_backup: true,
                ..Default::default()
            },
        );
        let r = block(ensure(SOURCE, true, &m.paths, &m)).unwrap();
        assert!(r.status.ready());
        assert_eq!(m.calls()[1..], ["apply".to_string(), "backup apply".into()]);
    }

    #[test]
    fn the_old_name_is_replaced() {
        let old: &[(&str, &[u8])] = &[("player-token.js", b"old")];
        let m = Machine::new(
            "legacy",
            Setup {
                extensions: &["adblock.js", "player-token.js"],
                files: old,
                applied: old,
                ..Default::default()
            },
        );
        assert_eq!(check(SOURCE, &m.paths).legacy, ["player-token.js"]);
        let r = block(ensure(SOURCE, true, &m.paths, &m)).unwrap();
        assert!(r.status.legacy.is_empty() && r.status.ready());
        assert!(!m.paths.extension("player-token.js").exists());
        assert_eq!(registered(&m.paths.config_dir), ["adblock.js", BRIDGE]);
    }

    #[test]
    fn an_outdated_copy_in_spotify_is_refreshed_without_a_restart() {
        let m = Machine::new(
            "outdated",
            Setup {
                extensions: &[BRIDGE],
                files: &[(BRIDGE, SOURCE)],
                applied: &[(BRIDGE, b"// bridge v1\n")],
                ..Default::default()
            },
        );
        let r = block(ensure(SOURCE, false, &m.paths, &m)).unwrap();
        assert!(r.status.applied_current && !r.needs_apply);
        assert!(m.calls().is_empty());
    }

    #[test]
    fn everything_current_does_nothing() {
        let m = Machine::new(
            "current",
            Setup {
                extensions: &[BRIDGE],
                files: &[(BRIDGE, SOURCE)],
                applied: &[(BRIDGE, SOURCE)],
                ..Default::default()
            },
        );
        let r = block(ensure(SOURCE, false, &m.paths, &m)).unwrap();
        assert!(r.status.ready() && r.actions.is_empty());
        assert!(m.calls().is_empty());
    }

    #[test]
    fn remove_takes_every_name_out_and_applies() {
        let m = Machine::new(
            "remove",
            Setup {
                extensions: &["adblock.js", BRIDGE, "player-token.js"],
                files: &[(BRIDGE, SOURCE), ("player-token.js", b"old")],
                applied: &[(BRIDGE, SOURCE)],
                ..Default::default()
            },
        );
        block(remove(&m.paths, &m)).unwrap();
        assert_eq!(registered(&m.paths.config_dir), ["adblock.js"]);
        assert!(!m.paths.extension(BRIDGE).exists() && !m.paths.in_spotify(BRIDGE).exists());
        assert_eq!(m.calls().last().map(String::as_str), Some("apply"));
    }

    #[test]
    fn remove_without_anything_registered_leaves_spotify_alone() {
        let m = Machine::new("remove-none", Setup::default());
        assert!(block(remove(&m.paths, &m)).unwrap().is_empty());
        assert!(m.calls().is_empty());
    }
}
