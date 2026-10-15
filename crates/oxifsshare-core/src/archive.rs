//! Opening an archive and resolving rows to files.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    error::Error,
    index::{norm_key, FileEntry, FileKind, FolderIndex},
    keydetect::{self, KeyReport},
    model::{ColumnKind, Table},
    xml,
};

/// Where the index and documents were found.
#[derive(Clone, Debug)]
pub struct Location {
    /// The index XML file.
    pub index_path: PathBuf,
    /// Folder holding the documents (the index's parent folder).
    pub folder: PathBuf,
    /// True when the user pointed at a parent and the `fsshare` subfolder was used.
    pub via_subfolder: bool,
}

fn find_index_in(dir: &Path) -> Result<Option<PathBuf>, Error> {
    let mut xmls: Vec<(String, PathBuf)> = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| Error::io(dir, e))?.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("fs-share.xml") {
            return Ok(Some(e.path()));
        }
        if name.to_ascii_lowercase().ends_with(".xml") {
            xmls.push((name, e.path()));
        }
    }
    xmls.sort();
    Ok(if xmls.len() == 1 {
        xmls.pop().map(|x| x.1)
    } else {
        None
    })
}

/// Finds the index for `path`: an XML file, a folder containing `fs-share.xml`
/// (or exactly one `.xml`), or a parent folder with an `fsshare` sub-folder.
pub fn locate(path: &Path) -> Result<Location, Error> {
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.is_file() {
        let folder = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let folder = if folder.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            folder
        };
        return Ok(Location {
            index_path: path.to_path_buf(),
            folder,
            via_subfolder: false,
        });
    }
    if let Some(index_path) = find_index_in(path)? {
        return Ok(Location {
            index_path,
            folder: path.to_path_buf(),
            via_subfolder: false,
        });
    }
    let mut subs: Vec<PathBuf> = fs::read_dir(path)
        .map_err(|e| Error::io(path, e))?
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir())
                && e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("fsshare")
        })
        .map(|e| e.path())
        .collect();
    subs.sort();
    for sub in subs {
        if let Some(index_path) = find_index_in(&sub)? {
            return Ok(Location {
                index_path,
                folder: sub,
                via_subfolder: true,
            });
        }
    }
    Err(Error::IndexNotFound {
        path: path.to_path_buf(),
    })
}

/// How a row resolved to a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    /// Index into [`Archive::files`] of the chosen file.
    pub file: Option<usize>,
    /// Number of files that matched (more than 1 means ambiguous).
    pub candidates: u32,
}

/// Counts shown in the status bar and the check report.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArchiveStats {
    /// Rows in the index.
    pub entries: usize,
    /// Rows with a file.
    pub matched: usize,
    /// Rows without a file.
    pub missing: usize,
    /// Rows whose key was empty.
    pub empty_key: usize,
    /// Rows with more than one matching file.
    pub ambiguous: usize,
    /// Files no row refers to.
    pub orphans: usize,
    /// Files whose key is used by a row but that lost the deterministic choice.
    pub shadowed: usize,
}

/// Relationship of a file to the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    /// Chosen by at least one row.
    Matched,
    /// Its key is used by a row, but another file won the deterministic choice.
    Shadowed,
    /// No row refers to it.
    Orphan,
}

/// A loaded archive.
#[derive(Debug)]
pub struct Archive {
    /// Where it was found.
    pub location: Location,
    /// The parsed index.
    pub table: Table,
    /// The folder index.
    pub folder: FolderIndex,
    /// Key detection result.
    pub key: KeyReport,
    pub(crate) resolutions: Vec<Resolution>,
    pub(crate) file_states: Vec<FileState>,
    pub(crate) haystack: Vec<String>,
    pub(crate) doc_type_col: Option<usize>,
    pub(crate) date_col: Option<usize>,
    stats: ArchiveStats,
}

impl Archive {
    /// Opens an archive from a path (see [`locate`]).
    pub fn open(path: &Path) -> Result<Archive, Error> {
        let location = locate(path)?;
        let bytes =
            fs::read(&location.index_path).map_err(|e| Error::io(&location.index_path, e))?;
        let table = xml::parse_index(&bytes)?;
        drop(bytes);
        let index_name = location
            .index_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let folder = FolderIndex::scan(&location.folder, index_name.as_deref())?;
        Ok(Archive::from_parts(location, table, folder))
    }

    /// Builds an archive from already-loaded parts.
    pub fn from_parts(location: Location, table: Table, folder: FolderIndex) -> Archive {
        let key = keydetect::detect(&table, &folder);
        let mut resolutions = Vec::with_capacity(table.rows.len());
        let mut stats = ArchiveStats {
            entries: table.rows.len(),
            ..ArchiveStats::default()
        };
        let mut used_keys: HashSet<String> = HashSet::new();
        let mut chosen_files: HashSet<usize> = HashSet::new();
        for row in &table.rows {
            let raw = key.chosen.and_then(|c| row.get(c)).unwrap_or("");
            if raw.trim().is_empty() {
                stats.empty_key += 1;
            }
            let cands = folder.lookup(raw);
            let res = Resolution {
                file: cands.first().copied(),
                candidates: cands.len() as u32,
            };
            if let Some(f) = res.file {
                stats.matched += 1;
                chosen_files.insert(f);
                used_keys.insert(norm_key(raw));
            }
            if res.candidates > 1 {
                stats.ambiguous += 1;
            }
            resolutions.push(res);
        }
        stats.missing = stats.entries - stats.matched;
        let mut file_states = Vec::with_capacity(folder.files.len());
        for (i, f) in folder.files.iter().enumerate() {
            let k = norm_key(&f.stem);
            if !used_keys.contains(&k) {
                stats.orphans += 1;
                file_states.push(FileState::Orphan);
            } else if !chosen_files.contains(&i) {
                stats.shadowed += 1;
                file_states.push(FileState::Shadowed);
            } else {
                file_states.push(FileState::Matched);
            }
        }
        let haystack = table
            .rows
            .iter()
            .map(|r| {
                let mut s = String::new();
                for v in r.values.iter().flatten() {
                    s.push_str(&v.to_lowercase());
                    s.push('\u{1f}');
                }
                s
            })
            .collect();
        let doc_type_col = table.find_column("Doc Type");
        let date_col = table.find_column("Date").or_else(|| {
            table
                .columns
                .iter()
                .position(|c| c.kind == ColumnKind::DateTime)
        });
        Archive {
            location,
            table,
            folder,
            key,
            resolutions,
            file_states,
            haystack,
            doc_type_col,
            date_col,
            stats,
        }
    }

    /// Status-bar counts.
    pub fn stats(&self) -> ArchiveStats {
        self.stats
    }

    /// How file number `i` of [`Archive::folder`] relates to the index.
    pub fn file_state(&self, i: usize) -> FileState {
        self.file_states
            .get(i)
            .copied()
            .unwrap_or(FileState::Orphan)
    }

    /// Resolution of a row.
    pub fn resolution(&self, row: usize) -> Resolution {
        self.resolutions.get(row).copied().unwrap_or_default()
    }

    /// The file chosen for a row.
    pub fn row_file(&self, row: usize) -> Option<&FileEntry> {
        self.resolution(row)
            .file
            .and_then(|f| self.folder.files.get(f))
    }

    /// File kind of a row ([`FileKind::Missing`] if no file).
    pub fn row_kind(&self, row: usize) -> FileKind {
        self.row_file(row).map_or(FileKind::Missing, |f| f.kind)
    }

    /// Distinct `Doc Type` values for the filter drop-down (empty if there is no such column).
    pub fn doc_type_values(&self) -> Vec<String> {
        self.doc_type_col
            .map(|c| self.distinct_values(c))
            .unwrap_or_default()
    }

    /// Sorted distinct values of a column (for filter drop-downs).
    pub fn distinct_values(&self, col: usize) -> Vec<String> {
        let mut v: Vec<String> = self
            .table
            .rows
            .iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        v.sort_by_key(|s| s.to_lowercase());
        v
    }
}
