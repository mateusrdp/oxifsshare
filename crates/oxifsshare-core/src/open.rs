//! Policy for handing a document to the operating system.
//!
//! The app never renders or executes documents. Before delegating to
//! `xdg-open` it checks that the target is a regular file that really lives
//! inside the archive folder and has a document-like extension.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::archive::Archive;

/// Extensions opened without confirmation.
pub const OPEN_ALLOWLIST: &[&str] = &["pdf", "tif", "tiff", "jpg", "jpeg", "msg", "eml", "txt"];

/// Why a document may not be opened.
#[derive(Debug)]
pub enum OpenError {
    /// The row has no file.
    NoFile,
    /// The file vanished or cannot be read.
    Io(io::Error),
    /// Not a regular file.
    NotRegular,
    /// The resolved path escapes the archive folder (e.g. via a symbolic link).
    OutsideArchive,
    /// The extension is not on the allow-list; the user must confirm explicitly.
    NeedsConfirmation(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NoFile => write!(f, "document not found: no file matches this entry"),
            OpenError::Io(e) => write!(f, "cannot read the document file: {e}"),
            OpenError::NotRegular => write!(f, "refusing to open: not a regular file"),
            OpenError::OutsideArchive => write!(
                f,
                "refusing to open: the file resolves outside the archive folder"
            ),
            OpenError::NeedsConfirmation(ext) => {
                write!(
                    f,
                    "files of type .{ext} are not opened automatically; confirm to open it anyway"
                )
            }
        }
    }
}

impl std::error::Error for OpenError {}

/// Removes the `\\?\` prefix that Windows canonicalisation adds, because the
/// shell open call can reject such paths. Network (`UNC`) paths are kept.
/// Does nothing for other paths.
pub fn strip_verbatim(p: PathBuf) -> PathBuf {
    let Some(s) = p.to_str() else { return p };
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => p,
    }
}

/// Validates `file` against `root` and returns its canonical path.
/// With `confirmed`, extensions outside the allow-list are accepted.
pub fn checked_path(root: &Path, file: &Path, confirmed: bool) -> Result<PathBuf, OpenError> {
    let root = fs::canonicalize(root).map_err(OpenError::Io)?;
    let real = fs::canonicalize(file).map_err(OpenError::Io)?;
    if !real.starts_with(&root) {
        return Err(OpenError::OutsideArchive);
    }
    if !fs::metadata(&real).map_err(OpenError::Io)?.is_file() {
        return Err(OpenError::NotRegular);
    }
    let ext = real
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !confirmed && !OPEN_ALLOWLIST.contains(&ext.as_str()) {
        return Err(OpenError::NeedsConfirmation(ext));
    }
    Ok(if cfg!(windows) {
        strip_verbatim(real)
    } else {
        real
    })
}

impl Archive {
    /// The validated path to hand to the OS for a row.
    pub fn open_target(&self, row: usize, confirmed: bool) -> Result<PathBuf, OpenError> {
        let f = self.row_file(row).ok_or(OpenError::NoFile)?;
        checked_path(&self.location.folder, &f.path, confirmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefix_is_stripped_only_for_drive_paths() {
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\a\b.pdf")),
            PathBuf::from(r"C:\a\b.pdf")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\srv\x")),
            PathBuf::from(r"\\?\UNC\srv\x")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from("/usr/x")),
            PathBuf::from("/usr/x")
        );
    }
}
