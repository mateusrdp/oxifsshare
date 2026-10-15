//! Typed errors. Messages never contain values read from the archive
//! (only positions, structure and user-supplied paths), so they are safe to
//! paste into a bug report.

use std::{fmt, io, path::PathBuf};

/// Everything that can go wrong while opening or reading an archive.
#[derive(Debug)]
pub enum Error {
    /// A file-system operation failed.
    Io {
        /// The path that was being accessed.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// The index is not well-formed XML.
    Xml {
        /// 1-based line.
        line: usize,
        /// 1-based column, counted in bytes.
        column: usize,
        /// Parser message (structure only).
        message: String,
    },
    /// The index text could not be decoded.
    Encoding {
        /// 1-based line.
        line: usize,
        /// 1-based column, counted in bytes.
        column: usize,
        /// What was wrong.
        message: String,
    },
    /// No index file was found at or below the given path.
    IndexNotFound {
        /// The path the user supplied.
        path: PathBuf,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "cannot access {}: {source}", path.display()),
            Error::Xml { line, column, message } => {
                write!(f, "malformed index XML at line {line}, column {column}: {message}")
            }
            Error::Encoding { line, column, message } => {
                write!(f, "index encoding problem at line {line}, column {column}: {message}")
            }
            Error::IndexNotFound { path } => write!(
                f,
                "no index file found at {} (looked for fs-share.xml, a single .xml file, or an fsshare subfolder)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
