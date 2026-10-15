//! One-pass index of the archive folder.
//!
//! Matching rules (derived from observed behaviour of the original viewer,
//! adapted for a case-sensitive file system):
//! * a file belongs to a key when its name is `<key>.<anything>`;
//! * keys compare case-insensitively, and all-digit keys compare numerically,
//!   so zero padding never matters (`000123` equals `123`);
//! * when several files share a key the choice is deterministic: lowest
//!   extension rank (pdf, tif, tiff, jpg, jpeg, msg, others) and then file name.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::error::Error;

/// Broad file category, derived from the extension only. Contents are never read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileKind {
    /// `.pdf`
    Pdf,
    /// `.tif` / `.tiff`
    Tiff,
    /// `.jpg` / `.jpeg`
    Jpeg,
    /// Outlook `.msg` (opening is delegated to the OS; no conversion in v1).
    Msg,
    /// Anything else.
    Other,
    /// A row whose file was not found.
    Missing,
}

impl FileKind {
    /// Classifies a lower-case extension.
    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "pdf" => FileKind::Pdf,
            "tif" | "tiff" => FileKind::Tiff,
            "jpg" | "jpeg" => FileKind::Jpeg,
            "msg" => FileKind::Msg,
            _ => FileKind::Other,
        }
    }

    /// Short label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            FileKind::Pdf => "pdf",
            FileKind::Tiff => "tif",
            FileKind::Jpeg => "jpg",
            FileKind::Msg => "msg",
            FileKind::Other => "other",
            FileKind::Missing => "missing",
        }
    }
}

fn ext_rank(ext: &str) -> u8 {
    match ext {
        "pdf" => 0,
        "tif" => 1,
        "tiff" => 2,
        "jpg" => 3,
        "jpeg" => 4,
        "msg" => 5,
        _ => 9,
    }
}

/// Normalises a key for matching: lower-case, and all-digit keys lose leading zeros.
pub fn norm_key(s: &str) -> String {
    let t = s.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
        let trimmed = t.trim_start_matches('0');
        return if trimmed.is_empty() {
            "0".into()
        } else {
            trimmed.into()
        };
    }
    t.to_lowercase()
}

/// One regular file in the archive folder.
#[derive(Clone, Debug)]
pub struct FileEntry {
    /// Full path.
    pub path: PathBuf,
    /// File name (lossy UTF-8).
    pub name: String,
    /// Name up to the first dot.
    pub stem: String,
    /// Lower-case text after the last dot.
    pub ext: String,
    /// Category from the extension.
    pub kind: FileKind,
    /// Size in bytes.
    pub size: u64,
}

/// Index of the folder holding the documents.
#[derive(Debug, Default)]
pub struct FolderIndex {
    /// All indexable files, excluding the index XML itself.
    pub files: Vec<FileEntry>,
    by_key: HashMap<String, Vec<usize>>,
    /// Sub-folders (ignored).
    pub subdirs: usize,
    /// Symbolic links (ignored, for safety).
    pub symlinks: usize,
    /// Files without an extension (cannot match a key).
    pub no_extension: usize,
    /// Hidden files (ignored).
    pub hidden: usize,
    /// Entries that could not be read.
    pub unreadable: usize,
}

impl FolderIndex {
    /// Scans `folder` once. `index_file_name` is excluded from the result.
    pub fn scan(folder: &Path, index_file_name: Option<&str>) -> Result<Self, Error> {
        let rd = fs::read_dir(folder).map_err(|e| Error::io(folder, e))?;
        let mut idx = FolderIndex::default();
        for entry in rd {
            let Ok(entry) = entry else {
                idx.unreadable += 1;
                continue;
            };
            let Ok(ft) = entry.file_type() else {
                idx.unreadable += 1;
                continue;
            };
            if ft.is_symlink() {
                idx.symlinks += 1;
                continue;
            }
            if ft.is_dir() {
                idx.subdirs += 1;
                continue;
            }
            if !ft.is_file() {
                idx.unreadable += 1;
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if index_file_name.is_some_and(|n| n.eq_ignore_ascii_case(&name)) {
                continue;
            }
            if name.starts_with('.') {
                idx.hidden += 1;
                continue;
            }
            let Some(first_dot) = name.find('.') else {
                idx.no_extension += 1;
                continue;
            };
            let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
            if ext.is_empty() {
                idx.no_extension += 1;
                continue;
            }
            let size = entry.metadata().map_or(0, |m| m.len());
            idx.files.push(FileEntry {
                path: entry.path(),
                stem: name[..first_dot].to_string(),
                kind: FileKind::from_ext(&ext),
                ext,
                name,
                size,
            });
        }
        idx.files.sort_by(|a, b| a.name.cmp(&b.name));
        for (i, f) in idx.files.iter().enumerate() {
            idx.by_key.entry(norm_key(&f.stem)).or_default().push(i);
        }
        let files = &idx.files;
        for v in idx.by_key.values_mut() {
            v.sort_by(|&a, &b| {
                (ext_rank(&files[a].ext), &files[a].name)
                    .cmp(&(ext_rank(&files[b].ext), &files[b].name))
            });
        }
        Ok(idx)
    }

    /// Candidate files for a raw key value, best first. Empty if none.
    pub fn lookup(&self, raw_key: &str) -> &[usize] {
        if raw_key.trim().is_empty() {
            return &[];
        }
        self.by_key
            .get(&norm_key(raw_key))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether a normalised key has any file.
    pub fn has_key(&self, normalised: &str) -> bool {
        self.by_key.contains_key(normalised)
    }

    /// Number of distinct keys among the files.
    pub fn key_count(&self) -> usize {
        self.by_key.len()
    }
}
