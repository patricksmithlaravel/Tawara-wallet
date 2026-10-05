//! What the application remembers between runs (docs/DECISIONS.md D27,
//! item 10): the node the person chose, the store they last made or
//! opened, the unit amounts are shown in, and the auto-lock period.
//!
//! Nothing secret is kept here, and nothing the library reads: the file is
//! `preferences` in the application's folder, beside the store's own
//! `keystore` folder and never inside it. It is a few `key = value` lines,
//! so a person can read it. A line this version does not know, or a value
//! it cannot use, is ignored and the default stands; a damaged file costs
//! the person their choices, never the application its start.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::location::{Environment, NoDefaultLocation, Platform, default_store_dir};

/// The file's name in the application's folder.
pub const FILE_NAME: &str = "preferences";

/// The auto-lock periods offered, in minutes (docs/DECISIONS.md D24, D27
/// item 10). There is no "never": an unlocked store holds the seed.
pub const IDLE_LOCK_MINUTES: [u64; 5] = [1, 2, 5, 10, 15];

/// The unit amounts are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AmountUnit {
    /// MCM, with nine decimal places.
    #[default]
    Mcm,
    /// Whole nanoMCM.
    NanoMcm,
}

/// What is remembered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    /// The node last chosen, as typed. The worker checks it again when it is
    /// set (`Command::SetNode`), so a stored value is never trusted.
    pub node: Option<String>,
    /// The folder of the store last made or opened, as an absolute path, so
    /// the application offers to unlock it at the next start wherever it
    /// is. A path, nothing more: the library checks the folder again when
    /// it is opened.
    pub store: Option<String>,
    pub unit: AmountUnit,
    /// One of [`IDLE_LOCK_MINUTES`].
    pub idle_lock: Duration,
}

impl Default for Preferences {
    fn default() -> Preferences {
        Preferences {
            node: None,
            store: None,
            unit: AmountUnit::Mcm,
            idle_lock: crate::DEFAULT_IDLE_LOCK,
        }
    }
}

/// The preferences file for the default location: `preferences` beside the
/// default store's folder.
pub fn default_file(platform: Platform, env: &Environment) -> Result<PathBuf, NoDefaultLocation> {
    let store = default_store_dir(platform, env)?;
    Ok(store
        .parent()
        .map_or_else(|| PathBuf::from(FILE_NAME), |app| app.join(FILE_NAME)))
}

/// Read preferences from text, line by line. Anything not understood is
/// left at its default.
#[must_use]
pub fn parse(text: &str) -> Preferences {
    let mut out = Preferences::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "node" if !value.is_empty() => out.node = Some(value.to_owned()),
            "store" if !value.is_empty() => out.store = Some(value.to_owned()),
            "unit" => match value {
                "mcm" => out.unit = AmountUnit::Mcm,
                "nanomcm" => out.unit = AmountUnit::NanoMcm,
                _ => {}
            },
            "auto-lock-minutes" => {
                if let Ok(m) = value.parse::<u64>()
                    && IDLE_LOCK_MINUTES.contains(&m)
                {
                    out.idle_lock = Duration::from_secs(m * 60);
                }
            }
            _ => {}
        }
    }
    out
}

/// The file's text for `prefs`.
#[must_use]
pub fn render(prefs: &Preferences) -> String {
    let mut out = String::from(
        "# Tawara's preferences. Nothing secret is kept here, and nothing the wallet\n\
         # library reads; deleting this file resets them.\n",
    );
    // One line each: a line break would end the value and start a new key.
    let one_line = |s: &str| -> String { s.chars().filter(|c| !c.is_control()).collect() };
    if let Some(node) = &prefs.node {
        out.push_str(&format!("node = {}\n", one_line(node)));
    }
    if let Some(store) = &prefs.store {
        out.push_str(&format!("store = {}\n", one_line(store)));
    }
    out.push_str(match prefs.unit {
        AmountUnit::Mcm => "unit = mcm\n",
        AmountUnit::NanoMcm => "unit = nanomcm\n",
    });
    out.push_str(&format!(
        "auto-lock-minutes = {}\n",
        prefs.idle_lock.as_secs() / 60
    ));
    out
}

/// The preferences in `path`, or the defaults when it does not exist or
/// cannot be read.
#[must_use]
pub fn load(path: &Path) -> Preferences {
    std::fs::read_to_string(path).map_or_else(|_| Preferences::default(), |text| parse(&text))
}

/// Write `prefs` to `path`, replacing the file whole: written to a new file
/// beside it, flushed, then renamed over it, so a crash leaves the old file
/// or the new one and never half of one. The folder is made when missing,
/// private to the user on Unix, as the worker makes it for a store.
pub fn save(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let mut folders = std::fs::DirBuilder::new();
        folders.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            folders.mode(0o700);
        }
        folders.create(parent)?;
    }
    let mut staging = path.as_os_str().to_owned();
    staging.push(".new");
    let staging = PathBuf::from(staging);
    let mut file = std::fs::File::create(&staging)?;
    file.write_all(render(prefs).as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&staging, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back() {
        let prefs = Preferences {
            node: Some("https://node.example".into()),
            store: Some("/home/p/wallets/savings".into()),
            unit: AmountUnit::NanoMcm,
            idle_lock: Duration::from_secs(10 * 60),
        };
        assert_eq!(parse(&render(&prefs)), prefs);
        assert_eq!(
            parse(&render(&Preferences::default())),
            Preferences::default()
        );
    }

    #[test]
    fn what_is_not_understood_keeps_the_default() {
        let prefs = parse(
            "garbage\nnode =\nunit = parsecs\nauto-lock-minutes = 0\nauto-lock-minutes = \
             never\nauto-lock-minutes = 7\ncolour = green\n",
        );
        assert_eq!(prefs, Preferences::default());
    }

    #[test]
    fn a_node_with_a_line_break_cannot_add_a_key() {
        let prefs = Preferences {
            node: Some("https://a.example\nunit = nanomcm".into()),
            ..Preferences::default()
        };
        let back = parse(&render(&prefs));
        assert_eq!(back.unit, AmountUnit::Mcm);
        assert_eq!(
            back.node.as_deref(),
            Some("https://a.exampleunit = nanomcm")
        );
    }

    #[test]
    fn the_file_sits_beside_the_default_store() {
        let env = Environment::of(&[("HOME", "/home/p")]);
        assert_eq!(
            default_file(Platform::Linux, &env).ok(),
            Some(PathBuf::from("/home/p/.local/share/tawara/preferences"))
        );
    }

    #[test]
    fn saving_replaces_the_file_whole() {
        let dir = std::env::temp_dir().join(format!("tawara-prefs-{}", std::process::id()));
        let path = dir.join("app").join(FILE_NAME);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            load(&path),
            Preferences::default(),
            "no file is the defaults"
        );
        let prefs = Preferences {
            node: Some("https://node.example".into()),
            ..Preferences::default()
        };
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path), prefs);
        save(&path, &Preferences::default()).unwrap();
        assert_eq!(load(&path), Preferences::default());
        assert!(!path.with_file_name("preferences.new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
