//! TUI settings: the theme, and where it's remembered.
//!
//! Kept in a one-line file rather than a config format, because there is one
//! setting. `theme = <slug>` is greppable, hand-editable, and needs no parser
//! dependency; anything unrecognised falls back to the default rather than
//! failing to start.

use std::fs;
use std::path::{Path, PathBuf};

use ratatui_themes::ThemeName;

/// The user's choices, as loaded from disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeName,
}

impl Settings {
    /// Load, treating anything missing or unreadable as "defaults" — a broken
    /// config file should never stop you seeing the board.
    pub fn load() -> Settings {
        match path() {
            Some(path) => Settings::load_from(&path),
            None => Settings::default(),
        }
    }

    /// Save, reporting failure so the dialog can say so rather than pretending.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        self.save_to(&path)
    }

    /// The half that takes a path, so the tests need no environment fiddling.
    fn load_from(path: &Path) -> Settings {
        let Ok(text) = fs::read_to_string(path) else {
            return Settings::default();
        };
        let mut settings = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "theme"
                && let Ok(theme) = value.trim().parse::<ThemeName>()
            {
                settings.theme = theme;
            }
        }
        settings
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("theme = {}\n", self.theme.slug()))?;
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/agile-md/config`, else `~/.config/agile-md/config`.
fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("agile-md").join("config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_broken_config_gives_the_default_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        assert_eq!(Settings::load_from(&path), Settings::default());

        fs::write(&path, "nonsense\ntheme = not-a-theme\n").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
    }

    #[test]
    fn a_theme_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agile-md/config");
        let settings = Settings {
            theme: ThemeName::Nord,
        };
        settings.save_to(&path).unwrap();

        assert_eq!(Settings::load_from(&path).theme, ThemeName::Nord);
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(
            written.trim(),
            format!("theme = {}", ThemeName::Nord.slug())
        );
    }
}
