//! Headless core of **oxifsshare**: index parsing, folder indexing, key
//! detection, search and the `--check` report. No GUI dependencies.
//!
//! Typical use:
//! ```no_run
//! let archive = oxifsshare_core::Archive::open(std::path::Path::new("/path/to/fsshare"))?;
//! println!("{} entries", archive.table.rows.len());
//! # Ok::<(), oxifsshare_core::Error>(())
//! ```
//!
//! Privacy: nothing in this crate logs or prints archive values, and errors
//! carry only positions and structure.

pub mod archive;
pub mod check;
pub mod date;
pub mod emlcache;
mod error;
pub mod index;
pub mod keydetect;
pub mod mime;
pub mod model;
pub mod msg;
pub mod open;
pub mod search;
pub mod testgen;
pub mod testmsg;
pub mod xml;

pub use archive::{locate, Archive, ArchiveStats, FileState, Location, Resolution};
pub use error::Error;
pub use index::{norm_key, FileEntry, FileKind, FolderIndex};
pub use keydetect::KeyReport;
pub use model::{decode_name, Column, ColumnKind, Row, Table};
pub use open::{checked_path, OpenError, OPEN_ALLOWLIST};
pub use search::{Filter, Sort};

/// Returns the crate version, for `--version` output.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_like() {
        let v = version();
        assert_eq!(v.split('.').count(), 3, "unexpected version string: {v}");
    }
}
