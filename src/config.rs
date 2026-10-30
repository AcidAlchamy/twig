//! Settings, stored as TOML in %APPDATA%\Twig\settings.toml.
//!
//! The resident reads this file at start and whenever the settings window says
//! it changed; the settings window writes it. Unknown or missing keys fall back
//! to the defaults, so old files keep working.

use crate::tree::{self, Style};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+T";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The hotkey does nothing while paused (the right-click menu still works).
    pub paused: bool,
    /// Like "Ctrl+Alt+T"; empty = no hotkey.
    pub hotkey: String,
    /// With one folder selected in Explorer, the hotkey copies that folder.
    pub use_selection: bool,
    /// How the tree is written.
    pub style: Style,
    /// Levels below the folder; 0 = all the way down.
    pub depth: u32,
    pub folders_only: bool,
    /// Leave out what .gitignore leaves out.
    pub gitignore: bool,
    /// Include dot files and hidden files.
    pub hidden: bool,
    /// Names always left out; `*` works.
    pub skip: Vec<String>,
    pub sizes: bool,
    /// At most this many entries per folder; 0 = no limit.
    pub per_folder: u32,
    /// At most this many lines; 0 = no limit.
    pub max_lines: u32,
    /// The card in the corner after each copy.
    pub card: bool,
    /// Show the donationware hello card when Twig starts.
    pub hello_card: bool,
}

impl Default for Config {
    fn default() -> Self {
        let t = tree::Options::default();
        Self {
            paused: false,
            hotkey: DEFAULT_HOTKEY.into(),
            use_selection: true,
            style: Style::Tree,
            depth: t.depth,
            folders_only: t.folders_only,
            gitignore: t.gitignore,
            hidden: t.hidden,
            skip: t.skip,
            sizes: t.sizes,
            per_folder: t.per_folder as u32,
            max_lines: t.max_lines as u32,
            card: true,
            hello_card: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg: Config =
            std::fs::read_to_string(path()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
        cfg.depth = cfg.depth.min(64);
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(
            &tmp,
            format!("# Twig settings. Edit in the app: right-click the tray icon, Settings.\n\n{text}"),
        )?;
        std::fs::rename(tmp, path)
    }

    pub fn options(&self) -> tree::Options {
        tree::Options {
            depth: self.depth,
            folders_only: self.folders_only,
            gitignore: self.gitignore,
            hidden: self.hidden,
            skip: self.skip.clone(),
            sizes: self.sizes,
            per_folder: self.per_folder as usize,
            max_lines: self.max_lines as usize,
        }
    }
}

pub fn dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Twig")
}

pub fn path() -> PathBuf {
    dir().join("settings.toml")
}

pub fn exists() -> bool {
    path().exists()
}

/// The folder copied last (the settings window previews it).
pub fn last_folder() -> Option<PathBuf> {
    let s = std::fs::read_to_string(dir().join("last.txt")).ok()?;
    let p = PathBuf::from(s.trim());
    p.is_dir().then_some(p)
}

pub fn set_last_folder(p: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(dir().join("last.txt"), p.display().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_and_partial_files() {
        let c = Config { style: Style::Markdown, depth: 3, skip: vec!["dist".into()], ..Config::default() };
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(text.contains("style = \"markdown\""));
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, c);

        let partial: Config = toml::from_str("sizes = true\nstyle = \"paths\"").unwrap();
        assert!(partial.sizes && partial.gitignore);
        assert_eq!(partial.style, Style::Paths);
        assert_eq!(partial.hotkey, DEFAULT_HOTKEY);
        assert_eq!(partial.options().skip, tree::default_skip());
    }
}
