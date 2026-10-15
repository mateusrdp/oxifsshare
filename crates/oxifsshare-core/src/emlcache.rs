//! A private temporary directory for converted `.eml` files.
//!
//! Converted messages contain document content, so they are written only to a
//! directory created for this process: mode 0700 on Unix, files 0600, removed
//! when the cache is dropped. Directories left behind by a crashed run are
//! removed (by name and age) the next time a cache is created.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::msg::{self, Info, MsgError};

const PREFIX: &str = "oxifsshare-eml-";
const STALE_AFTER: Duration = Duration::from_secs(24 * 3600);

/// Why a message could not be prepared for opening.
#[derive(Debug)]
pub enum CacheError {
    /// The message could not be converted.
    Convert(MsgError),
    /// The private directory or file could not be written.
    Io(io::Error),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Convert(e) => write!(f, "{e}"),
            CacheError::Io(e) => {
                write!(f, "the temporary file could not be written ({})", e.kind())
            }
        }
    }
}

impl std::error::Error for CacheError {}

/// Converted `.eml` files for one run of the program.
#[derive(Debug, Default)]
pub struct EmlCache {
    dir: Option<PathBuf>,
    done: HashMap<(PathBuf, u64, u64), PathBuf>,
    counter: u32,
}

impl EmlCache {
    /// An empty cache; nothing is created on disk until the first conversion.
    pub fn new() -> EmlCache {
        EmlCache::default()
    }

    /// Converts `msg_path` (if not done already) and returns the `.eml` path and what was found.
    pub fn convert(&mut self, msg_path: &Path) -> Result<(PathBuf, Option<Info>), CacheError> {
        let meta =
            fs::metadata(msg_path).map_err(|e| CacheError::Convert(MsgError::Io(e.kind())))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let key = (msg_path.to_path_buf(), meta.len(), mtime);
        if let Some(p) = self.done.get(&key) {
            if p.is_file() {
                return Ok((p.clone(), None));
            }
        }
        let converted = msg::convert_file(msg_path).map_err(CacheError::Convert)?;
        let dir = self.dir()?;
        self.counter += 1;
        let stem: String = msg_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(40)
            .collect();
        let out = dir.join(format!(
            "{:04}-{}.eml",
            self.counter,
            if stem.is_empty() { "message" } else { &stem }
        ));
        write_private(&out, &converted.eml).map_err(CacheError::Io)?;
        self.done.insert(key, out.clone());
        Ok((out, Some(converted.info)))
    }

    fn dir(&mut self) -> Result<PathBuf, CacheError> {
        if let Some(d) = &self.dir {
            return Ok(d.clone());
        }
        sweep_stale(&std::env::temp_dir());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let d = std::env::temp_dir().join(format!("{PREFIX}{}-{nanos}", std::process::id()));
        make_private_dir(&d).map_err(CacheError::Io)?;
        self.dir = Some(d.clone());
        Ok(d)
    }

    /// The private directory, if one has been created.
    pub fn dir_path(&self) -> Option<&Path> {
        self.dir.as_deref()
    }
}

impl Drop for EmlCache {
    fn drop(&mut self) {
        if let Some(d) = self.dir.take() {
            let _ = fs::remove_dir_all(d);
        }
    }
}

#[cfg(unix)]
fn make_private_dir(p: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(p)
}

#[cfg(not(unix))]
fn make_private_dir(p: &Path) -> io::Result<()> {
    fs::create_dir(p)
}

#[cfg(unix)]
fn write_private(p: &Path, data: &[u8]) -> io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(p)?;
    f.write_all(data)
}

#[cfg(not(unix))]
fn write_private(p: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)?;
    f.write_all(data)
}

/// Removes our own old cache directories in `tmp` (name prefix, real directory, older than a day).
pub fn sweep_stale(tmp: &Path) {
    let Ok(rd) = fs::read_dir(tmp) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().starts_with(PREFIX) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !meta.is_dir() {
            continue; // symlinks and files are never followed or removed
        }
        let old = meta
            .modified()
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if old {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::TempDir;
    use crate::testmsg::{build_msg, SynthMsg};

    fn write_msg(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        let m = SynthMsg {
            subject: "Cache test".into(),
            body: Some("body".into()),
            ..SynthMsg::default()
        };
        fs::write(&p, build_msg(&m)).unwrap();
        p
    }

    #[test]
    fn converts_reuses_and_cleans_up() {
        let t = TempDir::new("emlcache").unwrap();
        let src = write_msg(t.path(), "000123.MSG");
        let dir;
        {
            let mut c = EmlCache::new();
            assert!(c.dir_path().is_none(), "nothing is created before use");
            let (p1, info) = c.convert(&src).unwrap();
            assert!(info.is_some());
            assert!(p1.is_file());
            assert!(p1
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-000123.eml"));
            let (p2, again) = c.convert(&src).unwrap();
            assert_eq!(p1, p2);
            assert!(again.is_none(), "second call reuses the file");
            dir = c.dir_path().unwrap().to_path_buf();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                    0o700
                );
                assert_eq!(
                    fs::metadata(&p1).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        assert!(!dir.exists(), "removed on drop");
    }

    #[test]
    fn conversion_errors_are_typed() {
        let t = TempDir::new("emlcache-bad").unwrap();
        let p = t.path().join("bad.msg");
        fs::write(&p, b"not a message").unwrap();
        let mut c = EmlCache::new();
        assert!(matches!(
            c.convert(&p),
            Err(CacheError::Convert(MsgError::NotCompoundFile))
        ));
        assert!(
            c.dir_path().is_none(),
            "failed conversions create no directory"
        );
        assert!(matches!(
            c.convert(&t.path().join("missing.msg")),
            Err(CacheError::Convert(MsgError::Io(_)))
        ));
    }

    #[test]
    fn sweep_removes_only_old_own_directories() {
        let t = TempDir::new("emlsweep").unwrap();
        let mine_old = t.path().join(format!("{PREFIX}1-1"));
        let other = t.path().join("something-else");
        fs::create_dir(&mine_old).unwrap();
        fs::create_dir(&other).unwrap();
        // Fresh directories are kept.
        sweep_stale(t.path());
        assert!(mine_old.exists() && other.exists());
    }
}
