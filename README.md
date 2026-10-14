# oxifsshare

A small, offline, pure-Rust desktop app for Linux that opens and browses a
document archive exported by a discontinued, Windows-only viewer.

> **Status: milestone 1 (scaffold).** Nothing is usable yet. See the roadmap below.

## Not affiliated

oxifsshare is an independent, clean-room project written for interoperability
with an archive format. It is **not affiliated with, endorsed by, or derived
from** the vendor of the original software. It contains no vendor code,
binaries, icons or images. Product and company names belong to their owners.

## What it will do

- Load the XML index and list every entry with its metadata.
- Search (multi-term AND), filter (document type, file kind, date range) and sort.
- Open the selected document with your system's default application (`xdg-open`).
  The app itself never renders PDF, TIFF or e-mail and never runs documents.
- Work fully offline: no network access, no telemetry, no logging of document contents.
- Provide `oxifsshare --check <archive>`, a diagnostic mode that prints only
  counts and percentages so problems can be reported without leaking data.

## Privacy warning for contributors

Archives like this contain sensitive personal and financial information.

- **Never** commit, attach to an issue, or paste real archive data, file names,
  names, addresses, IDs or notes. The `.gitignore` blocks common archive files
  on purpose; do not work around it.
- Tests and examples use **synthetic data only**.
- When reporting a bug, use `oxifsshare --check` (milestone 2) and share its output only.

## Roadmap

1. Repo scaffold (this milestone)
2. Core library, `--check`, synthetic fixture generator
3. GUI (table, search, filters, details, open/reveal)
4. Documentation pass and release workflow
5. Later: `.msg` to `.eml`, `.deb` packaging, UX polish

## Building

See `docs/BUILDING.md` (milestone 4). Short version: Rust toolchain + `cargo build --release`.

## MSRV policy

The minimum supported Rust version is declared as `rust-version` in the root
`Cargo.toml` and checked in CI. It may be raised in a minor release and every
raise is recorded in `CHANGELOG.md`.

## Licence

MIT. See `LICENSE`. This is not legal advice.
