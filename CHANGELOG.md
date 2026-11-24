# Changelog

## Unreleased

### Added
- Milestone 1: workspace scaffold, MIT licence, strict `.gitignore`, CI, Dependabot.
- Milestone 2: core library (generic index parser, folder index, key detection, search, sort, open policy), `--check` mode, synthetic fixture generator and example, tests.

- Milestone 3: GUI (egui): virtualised sortable table, search, type/kind/date filters, details pane, Open/Reveal/Copy path, status bar, background loading.
- `docs/FORMAT.md` lists the dataset-specific column names (names only).

- Milestone 4: release workflow (Debian sid tarball, AppImage built on Debian 12, Windows portable zip, `SHA256SUMS`, generated notes), third-party licence notices, weekly `cargo audit`, `BUILDING.md`, `CONTRIBUTING.md`, `SECURITY.md`.
- GUI: Ctrl + mouse wheel (or pinch) zooms the whole interface; Ctrl +/-/0 also work. Not remembered between runs.
- Windows: `\\?\` path prefix stripped before opening.

- Milestone 5: `.msg` to `.eml` conversion (pure Rust, `cfb`), converted on open into a private temporary folder, "Open original .msg" fallback, `[Messages]` section in `--check`, `docs/MSG.md`, synthetic `.msg` builder and tests.

- Debian sid release is now an installable `.deb` (replaces the tarball): installs `/usr/bin/oxifsshare`, an **OxiFSShare** menu entry under *Office*, icons and licence notices; the workflow test-installs it with apt in `debian:sid`.
- Built-in folder browser ("Browse…").

### Changed
- Without an ARCHIVE path the GUI starts completely empty. The remembered-path feature was removed: the app no longer reads or writes any settings. "Browse…" always opens in the home folder. Window title and menu name are "OxiFSShare".
- MSRV set to 1.89 (the lowest toolchain verified).
- Upgraded the GUI toolkit to eframe/egui_extras 0.37.0 (completes Dependabot PRs #2 and #3, which must land together); MSRV raised to 1.95, which egui 0.37 requires. Ported to the new `egui::Panel` and `eframe::App::ui` APIs.
- Toolchain: everything (local builds, CI, the Debian sid release job) now uses rustup; added `rust-toolchain.toml`; `BUILDING.md` no longer recommends apt `cargo`/`rustc`.
- Refreshed `Cargo.lock` (clears RUSTSEC-2026-0592 `url`, RUSTSEC-2026-0486 `rustix`, and the yanked `clipboard-win`).
