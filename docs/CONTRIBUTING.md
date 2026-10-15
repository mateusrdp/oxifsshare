# Contributing

## The one rule: no real data

Never commit, paste or attach real archive data (documents, file names, names,
addresses, notes, IDs) or any file from the original vendor's software. Tests and
examples use **synthetic data only**; generate it with
`oxifsshare_core::testgen` or `cargo run -p oxifsshare-core --example gen_fixture -- OUT_DIR`.
The `.gitignore` blocks archive-like files on purpose; do not work around it.

## Clean-room

Do not include or copy the vendor's code, binaries, icons or images, and do not
use the vendor's names or trademarks. Describe the format from observed
structure and behaviour only. oxifsshare is not affiliated with the vendor.

## Before you open a pull request

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p oxifsshare --no-default-features --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Rules of the codebase

- Pure Rust, minimal dependencies. A new dependency needs a justification in `docs/ARCHITECTURE.md`; crates wrapping C/C++ (`-sys`) need an explicit reason. No GTK/Qt, web views or Tauri.
- All logic that is not drawing lives in `oxifsshare-core` and is tested headless.
- No panics on bad input; return typed errors. Errors and the `--check` report must never contain archive values or file names (a canary test enforces this).
- Update `docs/FORMAT.md` and `docs/ARCHITECTURE.md` when behaviour changes, and add a line to `CHANGELOG.md`.

## MSRV policy

`rust-version` in the root `Cargo.toml` is the lowest Rust verified by running the
tests (currently 1.89). CI builds with it. Raising it is allowed in a minor release
and must be recorded in `CHANGELOG.md`. GUI crate upgrades (`eframe`/`egui_extras`
are pinned with `=`) are what usually force a raise.

## Releases

Every push to `main` that changes code publishes a build. Bump `version` in the
root `Cargo.toml` to publish a clean `v<version>` release; other pushes publish the
pre-release `v<version>-build.<run_number>`.
