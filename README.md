# oxifsshare

A small, offline, pure-Rust desktop app for Linux that opens and browses a
document archive exported by a discontinued, Windows-only viewer.

> **Status: milestone 4.** Core library, `--check`, the GUI and release workflows are in place. See the roadmap below.

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
- When reporting a bug, use `oxifsshare --check` and share its output only.

## Roadmap

1. Repo scaffold (done)
2. Core library, `--check`, synthetic fixture generator (done)
3. GUI (table, search, filters, details, open/reveal) (done)
4. Documentation pass and release workflow (done, pending first run on GitHub)
5. `.msg` to `.eml` conversion (done, see `docs/MSG.md`)
6. Later: UX polish

## Using the GUI

```
oxifsshare /path/to/fsshare      # or run it and type the path
```

- Type in the search box to filter on every column (several words must all match; Ctrl+F focuses it).
- Filter by document type, file kind and date range (`YYYY-MM-DD`). Click a column header to sort (click again to reverse, once more to clear).
- Click a row for its details; double-click or press Enter to open it with your default application. Up/Down move the selection.
- `.msg` rows carry an orange marker. Opening one converts it to `.eml` in a private temporary folder (removed on exit) and hands that to your mail application; "Open original .msg" skips the conversion. Bodies stored only as RTF are not converted; see `docs/MSG.md`.
- The app never renders documents itself. It needs `xdg-open` (package `xdg-utils`) to open files.
- Running the GUI needs the usual desktop libraries (X11 or Wayland, OpenGL/EGL, xkbcommon); see `docs/BUILDING.md`.

## Try it today (no real data needed)

```
cargo run -p oxifsshare-core --example gen_fixture -- /tmp/demo 555
cargo run -p oxifsshare -- --check /tmp/demo
cargo run -p oxifsshare -- /tmp/demo        # the GUI, on fake data
```

`--check ARCHIVE` prints counts and match rates only. Add `--show-ids` to also
print the numeric IDs of unmatched rows and files. See `docs/FORMAT.md` and
`docs/ARCHITECTURE.md`.

## Install

Download from the GitHub Releases page (each file is listed in `SHA256SUMS`):

- **Debian sid:** `oxifsshare-<version>-debian-sid-x86_64.tar.gz`, built on Debian sid.
- **AppImage:** `oxifsshare-<version>-x86_64.AppImage`, built on Debian 12, so it runs on Debian 12 and newer systems with glibc 2.36 or newer.
- **Windows (portable):** `oxifsshare-<version>-windows-x86_64.zip`, unzip and run. Not tested on Windows by the author yet.

Opening documents needs `xdg-utils` on Linux. Releases tagged `-build.N` are automatic
pre-releases of every change; plain `v<version>` releases are the clean ones.

## Building

See `docs/BUILDING.md`. Short version: Rust toolchain + `cargo build --release --locked`.

## MSRV policy

The minimum supported Rust version is declared as `rust-version` in the root
`Cargo.toml` and checked in CI. It may be raised in a minor release and every
raise is recorded in `CHANGELOG.md`.

## Licence

MIT. See `LICENSE`. This is not legal advice.
