//! Filesystem locations under the user config directory (`~/.config/mfp`).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns `<config_dir>/mfp`, failing with `missing_msg` if the platform
/// config directory cannot be resolved. Does not create anything.
fn mfp_dir(missing_msg: &'static str) -> Result<PathBuf> {
    Ok(dirs::config_dir().context(missing_msg)?.join("mfp"))
}

/// Path of the favorites file (`<config_dir>/mfp/favorites.json`).
///
/// Creates the `mfp` config directory if it does not exist.
pub fn favorites_path() -> Result<PathBuf> {
    let config_dir = mfp_dir("Failed to find config directory")?;

    fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

    Ok(config_dir.join("favorites.json"))
}

/// Path of the offline downloads directory (`<config_dir>/mfp/downloads`).
///
/// The directory is not created here; the caller creates it on demand.
pub fn downloads_dir() -> Result<PathBuf> {
    Ok(mfp_dir("No se pudo obtener el directorio de configuración")?.join("downloads"))
}

/// Path of the config file (`<config_dir>/mfp/config.toml`). Nothing is created.
pub fn config_file() -> Result<PathBuf> {
    Ok(mfp_dir("Failed to find config directory")?.join("config.toml"))
}

/// Directory of user themes (`<config_dir>/mfp/themes`). Nothing is created.
pub fn themes_dir() -> Result<PathBuf> {
    Ok(mfp_dir("Failed to find config directory")?.join("themes"))
}

/// Whether `name` is safe to use as a theme file stem: not empty, no path
/// separators, no `..`, no leading dot and no NUL, so a value such as
/// `../x` can never point outside the themes directory.
pub fn is_valid_theme_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains(['/', '\\', '\0'])
}

/// Path of the user theme `name` inside `dir`, or `None` if the name is not
/// valid (see [`is_valid_theme_name`]).
pub fn theme_file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    is_valid_theme_name(name).then(|| dir.join(format!("{}.toml", name)))
}

/// Path of the user theme `name` (`<themes_dir>/<name>.toml`), or `None` if
/// the name is not valid or the config directory cannot be resolved.
#[cfg(test)]
pub fn theme_file(name: &str) -> Option<PathBuf> {
    theme_file_in(&themes_dir().ok()?, name)
}

/// Sorted names (`.toml` stems) of the user themes found in `dir`. A missing
/// or unreadable directory yields an empty list; other files are ignored.
#[cfg(test)]
pub fn custom_theme_names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_string();
            is_valid_theme_name(&stem).then_some(stem)
        })
        .collect();
    names.sort();
    names
}

/// Contents of `config.toml`. Every section and field is optional: a partial
/// or empty file yields the defaults for whatever is missing.
///
/// ```toml
/// [theme]
/// active = "nord"
///
/// [visualizer]
/// style = "bars"
/// enabled = true
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Theme selection.
    pub theme: ThemeConfig,
    /// Visualizer preferences (read once when the TUI starts).
    pub visualizer: VisualizerConfig,
}

/// `[theme]` section.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Name of a built-in preset or of a user theme in `themes/<name>.toml`.
    pub active: String,
}

/// `[visualizer]` section.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct VisualizerConfig {
    /// Visualizer style name (`bars`, `mirror`, ...).
    pub style: String,
    /// Whether the visualizer starts enabled.
    pub enabled: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active: "default".to_string(),
        }
    }
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        Self {
            style: "bars".to_string(),
            enabled: true,
        }
    }
}

impl Config {
    /// Loads `<config_dir>/mfp/config.toml`. Never fails: a missing file gives
    /// the defaults and an unreadable or malformed one gives the defaults plus
    /// a line in `mfp.log`.
    pub fn load() -> Self {
        let Ok(path) = config_file() else {
            return Self::default();
        };
        let (config, problem) = Self::load_from(&path);
        if let Some(problem) = problem {
            crate::logging::log(&problem);
        }
        config
    }

    /// Loads the config at `path`. Returns the config (defaults on any
    /// problem) and, when something went wrong, a message for the log. A
    /// missing file is normal and reports nothing.
    pub fn load_from(path: &Path) -> (Self, Option<String>) {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => {
                let msg = format!(
                    "config: cannot read {}: {}; using defaults",
                    path.display(),
                    e
                );
                return (Self::default(), Some(msg));
            }
        };
        match toml::from_str::<Config>(&content) {
            Ok(config) => (config, None),
            Err(e) => {
                let msg = format!("config: invalid {}: {}; using defaults", path.display(), e);
                (Self::default(), Some(msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_dir_is_under_mfp_config_dir() {
        if let Some(base) = dirs::config_dir() {
            assert_eq!(downloads_dir().unwrap(), base.join("mfp").join("downloads"));
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mfp-config-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn config_paths_are_under_mfp_config_dir() {
        if let Some(base) = dirs::config_dir() {
            let root = base.join("mfp");
            assert_eq!(config_file().unwrap(), root.join("config.toml"));
            assert_eq!(themes_dir().unwrap(), root.join("themes"));
            assert_eq!(
                theme_file("mine").unwrap(),
                root.join("themes").join("mine.toml")
            );
        }
    }

    #[test]
    fn theme_names_that_could_escape_the_dir_are_rejected() {
        for bad in [
            "", "..", "../x", "a/b", "a\\b", ".hidden", "x..y", "/abs", "a\0b",
        ] {
            assert!(!is_valid_theme_name(bad), "{bad:?}");
            assert_eq!(theme_file_in(Path::new("/t"), bad), None, "{bad:?}");
        }
        for good in ["mine", "my-theme", "solarized_2", "Tema Nuevo"] {
            assert!(is_valid_theme_name(good), "{good:?}");
        }
        assert_eq!(
            theme_file_in(Path::new("/t"), "mine"),
            Some(PathBuf::from("/t/mine.toml"))
        );
    }

    #[test]
    fn custom_theme_names_are_sorted_and_only_toml() {
        let dir = temp_dir("names");
        for f in ["zeta.toml", "alpha.toml", "notes.txt", "beta.toml.bak"] {
            fs::write(dir.join(f), "").unwrap();
        }
        fs::create_dir(dir.join("sub.toml")).unwrap();
        assert_eq!(
            custom_theme_names_in(&dir),
            vec!["alpha".to_string(), "sub".to_string(), "zeta".to_string()]
        );
        assert!(custom_theme_names_in(&dir.join("missing")).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_defaults() {
        let c = Config::default();
        assert_eq!(c.theme.active, "default");
        assert_eq!(c.visualizer.style, "bars");
        assert!(c.visualizer.enabled);
    }

    #[test]
    fn missing_config_gives_defaults_without_a_message() {
        let dir = temp_dir("missing");
        let (c, msg) = Config::load_from(&dir.join("config.toml"));
        assert_eq!(c, Config::default());
        assert_eq!(msg, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_and_partial_configs_fill_in_defaults() {
        let dir = temp_dir("partial");
        let path = dir.join("config.toml");
        fs::write(&path, "").unwrap();
        assert_eq!(Config::load_from(&path), (Config::default(), None));

        fs::write(&path, "[theme]\nactive = \"nord\"\n").unwrap();
        let (c, msg) = Config::load_from(&path);
        assert_eq!(msg, None);
        assert_eq!(c.theme.active, "nord");
        assert_eq!(c.visualizer, VisualizerConfig::default());

        fs::write(&path, "[visualizer]\nenabled = false\n").unwrap();
        let (c, _) = Config::load_from(&path);
        assert_eq!(c.theme.active, "default");
        assert!(!c.visualizer.enabled);
        assert_eq!(c.visualizer.style, "bars");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_config_gives_defaults_and_a_message() {
        let dir = temp_dir("malformed");
        let path = dir.join("config.toml");
        for bad in ["[theme\nactive = ", "theme = 3\n", "[theme]\nactive = 5\n"] {
            fs::write(&path, bad).unwrap();
            let (c, msg) = Config::load_from(&path);
            assert_eq!(c, Config::default(), "{bad:?}");
            assert!(msg.is_some_and(|m| m.contains("config.toml")), "{bad:?}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_config_gives_defaults_and_a_message() {
        let dir = temp_dir("unreadable");
        // A directory cannot be read as a file.
        let (c, msg) = Config::load_from(&dir);
        assert_eq!(c, Config::default());
        assert!(msg.is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
