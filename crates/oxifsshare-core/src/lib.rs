//! Headless core of **oxifsshare**.
//!
//! This crate will hold everything that is not user interface: the generic
//! index parser, the archive folder index, key-column detection, search and
//! the `--check` report. It has no GUI dependencies so it can be tested and
//! run on a machine without a display.
//!
//! Milestone 1 only provides the crate skeleton.

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
