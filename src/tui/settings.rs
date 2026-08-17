//! TUI settings: the theme, and where it's remembered.
//!
//! The theme lives in the shared config file under `theme`. Reading and
//! writing go through `crate::settings::Config`, which keeps the keys it does
//! not understand — otherwise saving a theme here would delete the GUI's
//! settings from the same file.

use std::path::Path;

use ratatui_themes::ThemeName;

use crate::settings::Config;

/// The user's choices, as loaded from disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeName,
}

impl Settings {
    /// Load, treating anything missing or unreadable as "defaults" — a broken
    /// config file should never stop you seeing the board.
    pub fn load() -> Settings {
        match crate::settings::path() {
            Some(path) => Settings::load_from(&path),
            None => Settings::default(),
        }
    }

    /// Save, reporting failure so the dialog can say so rather than pretending.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = crate::settings::path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        self.save_to(&path)
    }

    /// The half that takes a path, so the tests need no environment fiddling.
    fn load_from(path: &Path) -> Settings {
        let config = Config::load_from(path);
        let mut settings = Settings::default();
        if let Some(theme) = config
            .get("theme")
            .and_then(|v| v.parse::<ThemeName>().ok())
        {
            settings.theme = theme;
        }
        settings
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        let mut config = Config::load_from(path);
        config.set("theme", self.theme.slug());
        config.save_to(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

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
