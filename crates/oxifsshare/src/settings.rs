//! Remembers the last archive path in a one-line text file under the user's
//! config directory. The file contains only a path, never document data.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

const DIR: &str = "oxifsshare";
const FILE: &str = "last_path";

/// Where the settings live. On Windows `%APPDATA%\oxifsshare`; elsewhere
/// `$XDG_CONFIG_HOME/oxifsshare`, else `$HOME/.config/oxifsshare`.
pub fn config_dir_from(get: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    if windows {
        if let Some(a) = get("APPDATA").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(a).join(DIR));
        }
    }
    if let Some(x) = get("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(x);
        if p.is_absolute() {
            return Some(p.join(DIR));
        }
    }
    get("HOME")
        .filter(|v| !v.is_empty())
        .map(|h| PathBuf::from(h).join(".config").join(DIR))
}

fn config_dir() -> Option<PathBuf> {
    config_dir_from(|k| std::env::var_os(k), cfg!(windows))
}

/// Reads the remembered path from `dir`, if any.
pub fn load_from(dir: &Path) -> Option<String> {
    let s = fs::read_to_string(dir.join(FILE)).ok()?;
    let line = s.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Writes the path into `dir`. Errors are returned, never panicked on.
pub fn save_to(dir: &Path, path: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE), format!("{path}\n"))
}

/// The remembered archive path, if any.
pub fn load_last_path() -> Option<String> {
    load_from(&config_dir()?)
}

/// Remembers `path`. Failure is ignored: this is a convenience only.
pub fn save_last_path(path: &str) {
    if let Some(d) = config_dir() {
        let _ = save_to(&d, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(*v))
        }
    }

    #[test]
    fn prefers_xdg_then_home() {
        assert_eq!(
            config_dir_from(env(&[("XDG_CONFIG_HOME", "/x/cfg"), ("HOME", "/h")]), false),
            Some(PathBuf::from("/x/cfg/oxifsshare"))
        );
        assert_eq!(
            config_dir_from(env(&[("HOME", "/h")]), false),
            Some(PathBuf::from("/h/.config/oxifsshare"))
        );
        assert_eq!(
            config_dir_from(
                env(&[("XDG_CONFIG_HOME", "relative"), ("HOME", "/h")]),
                false
            ),
            Some(PathBuf::from("/h/.config/oxifsshare")),
            "relative XDG paths are ignored per the spec"
        );
        assert_eq!(config_dir_from(env(&[]), false), None);
        let win = &[("APPDATA", "C:/AppData"), ("HOME", "/h")];
        assert_eq!(
            config_dir_from(env(win), true),
            Some(PathBuf::from("C:/AppData").join("oxifsshare"))
        );
        assert_eq!(
            config_dir_from(env(win), false),
            Some(PathBuf::from("/h/.config/oxifsshare"))
        );
    }

    #[test]
    fn roundtrip_and_missing() {
        let d = oxifsshare_core::testgen::TempDir::new("settings").unwrap();
        let cfg = d.path().join("a").join("b");
        assert_eq!(load_from(&cfg), None);
        save_to(&cfg, "/some/path with spaces").unwrap();
        assert_eq!(load_from(&cfg).as_deref(), Some("/some/path with spaces"));
        std::fs::write(cfg.join(FILE), "\n").unwrap();
        assert_eq!(load_from(&cfg), None);
    }
}
