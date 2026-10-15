# Changelog

## Unreleased

### Added
- Milestone 1: workspace scaffold, MIT licence, strict `.gitignore`, CI, Dependabot.
- Milestone 2: core library (generic index parser, folder index, key detection, search, sort, open policy), `--check` mode, synthetic fixture generator and example, tests.

- Milestone 3: GUI (egui 0.33.3): virtualised sortable table, search, type/kind/date filters, details pane, Open/Reveal/Copy path, status bar, remembered path, background loading.
- `docs/FORMAT.md` lists the dataset-specific column names (names only).

- Milestone 4: release workflow (Debian sid tarball, AppImage built on Debian 12, Windows portable zip, `SHA256SUMS`, generated notes), third-party licence notices, weekly `cargo audit`, `BUILDING.md`, `CONTRIBUTING.md`, `SECURITY.md`.
- GUI: Ctrl + mouse wheel (or pinch) zooms the whole interface; Ctrl +/-/0 also work. Not remembered between runs.
- Windows: settings stored under `%APPDATA%`, `\\?\` path prefix stripped before opening.

- Milestone 5: `.msg` to `.eml` conversion (pure Rust, `cfb`), converted on open into a private temporary folder, "Open original .msg" fallback, `[Messages]` section in `--check`, `docs/MSG.md`, synthetic `.msg` builder and tests.

- Debian sid release is now an installable `.deb` (replaces the tarball): installs `/usr/bin/oxifsshare`, an **OxiFSShare** menu entry under *Office*, icons and licence notices; the workflow test-installs it with apt in `debian:sid`.
- Built-in folder browser ("Browse…").

### Changed
- Without an ARCHIVE path the GUI now starts empty. The last-used path is only pre-filled, never loaded automatically. Window title and menu name are "OxiFSShare".
- MSRV set to 1.89 (the lowest toolchain verified).
