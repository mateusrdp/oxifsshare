//! A small folder browser used by the "Browse…" button, so choosing the archive
//! folder needs no native file dialog (and therefore no extra dependencies).
//! The listing logic is plain code and is tested without a window.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// State of the folder browser.
#[derive(Debug)]
pub struct FolderPicker {
    dir: PathBuf,
    folders: Vec<String>,
    /// Show folders whose names start with a dot.
    pub show_hidden: bool,
    error: Option<String>,
    /// What the current folder looks like.
    hint: Hint,
}

/// Whether the current folder is, or contains, an archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// Nothing archive-like here.
    None,
    /// `fs-share.xml` is in this folder.
    Archive,
    /// There is an `fsshare` sub-folder holding `fs-share.xml`.
    ArchiveInSubfolder,
}

impl FolderPicker {
    /// Opens the browser at `start` (a folder, or a file whose folder is used),
    /// falling back to the home folder and then the filesystem root.
    pub fn new(start: &Path, home: Option<PathBuf>) -> FolderPicker {
        let mut candidates = Vec::new();
        if !start.as_os_str().is_empty() {
            if start.is_dir() {
                candidates.push(start.to_path_buf());
            } else if let Some(p) = start.parent() {
                candidates.push(p.to_path_buf());
            }
        }
        candidates.extend(home);
        candidates.push(PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" }));
        let dir = candidates
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut p = FolderPicker {
            dir,
            folders: Vec::new(),
            show_hidden: false,
            error: None,
            hint: Hint::None,
        };
        p.refresh();
        p
    }

    /// The folder being shown.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Sub-folders of the current folder, sorted case-insensitively.
    pub fn folders(&self) -> &[String] {
        &self.folders
    }

    /// A problem reading the folder, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the current folder looks like an archive.
    pub fn hint(&self) -> Hint {
        self.hint
    }

    /// Re-reads the current folder.
    pub fn refresh(&mut self) {
        self.error = None;
        self.folders.clear();
        match fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    // `metadata` follows symlinks, so links to folders are listed too.
                    let is_dir = fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
                    let name = e.file_name().to_string_lossy().into_owned();
                    if is_dir && (self.show_hidden || !name.starts_with('.')) {
                        self.folders.push(name);
                    }
                }
                self.folders
                    .sort_by_cached_key(|n| (n.to_lowercase(), n.clone()));
            }
            Err(e) => self.error = Some(format!("cannot read this folder ({})", e.kind())),
        }
        self.hint = if has_index(&self.dir) {
            Hint::Archive
        } else if has_index(&self.dir.join("fsshare")) {
            Hint::ArchiveInSubfolder
        } else {
            Hint::None
        };
    }

    /// Goes into the sub-folder `name` (a name from [`folders`](Self::folders)).
    pub fn enter(&mut self, name: &str) {
        if name.contains(['/', '\\']) || name == ".." || name == "." {
            return;
        }
        self.dir = self.dir.join(name);
        self.refresh();
    }

    /// Goes to the parent folder, if there is one.
    pub fn up(&mut self) {
        if let Some(p) = self.dir.parent() {
            self.dir = p.to_path_buf();
            self.refresh();
        }
    }
}

fn has_index(dir: &Path) -> bool {
    dir.join("fs-share.xml").is_file()
        || fs::read_dir(dir).is_ok_and(|rd| {
            rd.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("fs-share.xml")
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxifsshare_core::testgen::{synth_archive, Spec, TempDir};

    #[test]
    fn lists_sorted_folders_and_hides_dot_folders() {
        let t = TempDir::new("picker").unwrap();
        for d in ["beta", "Alpha", ".hidden", "gamma"] {
            fs::create_dir(t.path().join(d)).unwrap();
        }
        fs::write(t.path().join("file.txt"), b"x").unwrap();
        let mut p = FolderPicker::new(t.path(), None);
        assert_eq!(p.folders(), ["Alpha", "beta", "gamma"]);
        p.show_hidden = true;
        p.refresh();
        assert_eq!(p.folders(), [".hidden", "Alpha", "beta", "gamma"]);
        assert_eq!(p.hint(), Hint::None);
    }

    #[test]
    fn navigates_and_refuses_odd_names() {
        let t = TempDir::new("picker-nav").unwrap();
        fs::create_dir_all(t.path().join("a/b")).unwrap();
        let mut p = FolderPicker::new(t.path(), None);
        p.enter("a");
        assert_eq!(p.folders(), ["b"]);
        p.enter("..");
        p.enter("../..");
        p.enter("b/c");
        assert_eq!(p.dir(), t.path().join("a"));
        p.up();
        assert_eq!(p.dir(), t.path());
    }

    #[test]
    fn recognises_archive_folders() {
        let fx = synth_archive(&Spec {
            rows: 5,
            pass_parent: true,
            ..Spec::default()
        })
        .unwrap();
        let parent = FolderPicker::new(fx.dir.path(), None);
        assert_eq!(parent.hint(), Hint::ArchiveInSubfolder);
        let mut inner = FolderPicker::new(fx.dir.path(), None);
        inner.enter("fsshare");
        assert_eq!(inner.hint(), Hint::Archive);
    }

    #[test]
    fn falls_back_when_start_is_missing() {
        let t = TempDir::new("picker-fallback").unwrap();
        let p = FolderPicker::new(
            Path::new("/definitely/not/here"),
            Some(t.path().to_path_buf()),
        );
        assert_eq!(p.dir(), t.path());
        // A file path starts in its folder.
        fs::write(t.path().join("x.xml"), b"x").unwrap();
        let p = FolderPicker::new(&t.path().join("x.xml"), None);
        assert_eq!(p.dir(), t.path());
    }
}
