# Changelog

## Unreleased

### Added
- Milestone 1: workspace scaffold, MIT licence, strict `.gitignore`, CI, Dependabot.
- Milestone 2: core library (generic index parser, folder index, key detection, search, sort, open policy), `--check` mode, synthetic fixture generator and example, tests.

- Milestone 3: GUI (egui 0.33.3): virtualised sortable table, search, type/kind/date filters, details pane, Open/Reveal/Copy path, status bar, remembered path, background loading.
- `docs/FORMAT.md` lists the dataset-specific column names (names only).

- Milestone 4: release workflow (Debian sid tarball, AppImage built on Debian 12, Windows portable zip, `SHA256SUMS`, generated notes), third-party licence notices, weekly `cargo audit`, `BUILDING.md`, `CONTRIBUTING.md`, `SECURITY.md`.
- Windows: settings stored under `%APPDATA%`, `\\?\` path prefix stripped before opening.

- Milestone 5: `.msg` to `.eml` conversion (pure Rust, `cfb`), converted on open into a private temporary folder, "Open original .msg" fallback, `[Messages]` section in `--check`, `docs/MSG.md`, synthetic `.msg` builder and tests.

### Changed
- MSRV set to 1.89 (the lowest toolchain verified).
