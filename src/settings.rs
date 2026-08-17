//! The config file the front ends share.
//!
//! One `key = value` per line: greppable, hand-editable, no parser dependency.
//! Anything unrecognised is kept as it was rather than dropped, so the TUI
//! writing its theme cannot delete the GUI's settings and vice versa — the
//! whole reason this is a shared type rather than a `fs::write` in each.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The file as a list of key/value pairs, in the order they were read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    entries: Vec<(String, String)>,
}

impl Config {
    pub fn load() -> Config {
        match path() {
            Some(path) => Config::load_from(&path),
            None => Config::default(),
        }
    }

    /// A missing or unreadable file is an empty config, never an error: a
    /// broken config should not stop you seeing the board.
    pub fn load_from(path: &Path) -> Config {
        let Ok(text) = fs::read_to_string(path) else {
            return Config::default();
        };
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                entries.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
        Config { entries }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Set a key, keeping its position if it was already there so the file
    /// doesn't reshuffle every time it's written.
    pub fn set(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(name, _)| name == key) {
            Some((_, existing)) => *existing = value.to_string(),
            None => self.entries.push((key.to_string(), value.to_string())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        for (key, value) in &self.entries {
            text.push_str(&format!("{key} = {value}\n"));
        }
        fs::write(path, text)?;
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/agile-md/config`, else `~/.config/agile-md/config`.
pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("agile-md").join("config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load_from(&dir.path().join("nope"));
        assert_eq!(config, Config::default());
        assert_eq!(config.get("anything"), None);
    }

    #[test]
    fn writing_one_key_keeps_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        fs::write(&path, "theme = nord\ngui.font = large\n").unwrap();

        // The TUI saving its theme must not delete the GUI's settings.
        let mut config = Config::load_from(&path);
        config.set("theme", "gruvbox");
        config.save_to(&path).unwrap();

        let reread = Config::load_from(&path);
        assert_eq!(reread.get("theme"), Some("gruvbox"));
        assert_eq!(
            reread.get("gui.font"),
            Some("large"),
            "the GUI key survived"
        );
    }

    #[test]
    fn comments_and_nonsense_are_ignored_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        fs::write(&path, "# a note\nnonsense\ngui.theme = dark\n").unwrap();
        assert_eq!(Config::load_from(&path).get("gui.theme"), Some("dark"));
    }

    #[test]
    fn a_new_key_appends_and_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agile-md/config");
        let mut config = Config::default();
        config.set("gui.font", "medium");
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).get("gui.font"), Some("medium"));
    }
}
