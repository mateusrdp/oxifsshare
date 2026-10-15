# Architecture

Status: milestone 3 (core library, `--check` and the GUI).

## Goals

- Read-only browser for a document archive; nothing is modified.
- Fully offline. No network, no telemetry, no logging of document contents.
- Pure Rust, few dependencies, easy to audit.
- Everything except the UI is testable headless.

## Workspace

```
crates/oxifsshare-core   library: no GUI dependencies
  error      typed errors (positions and structure only, never values)
  date       ISO 8601 / xs:dateTime parsing without a date crate
  model      Table, Column, Row, `_xHHHH_` name decoding, kind inference
  xml        streaming parser (quick-xml) -> Table
  index      one-pass folder index and key normalisation
  keydetect  default key, 50% rule, all-column fallback
  archive    locate + open + resolve rows to files + status counts
  search     query (terms AND, filters) and sort, returning row indices
  open       policy check before handing a file to the OS
  mime       base64, quoted-printable, RFC 2047/2231 helpers
  msg        .msg (OLE) -> .eml conversion using `cfb` (see MSG.md)
  emlcache   private temp directory for converted .eml files
  check      the --check report (includes a [Messages] section)
  testgen    synthetic fixture generator (fake data only)
  testmsg    synthetic .msg builder (fake data only)
crates/oxifsshare        thin binary
  main       CLI parsing: --check, --show-ids, --version, or launch the GUI
  gui        egui front end (feature `gui`, on by default)
  settings   last archive path in $XDG_CONFIG_HOME/oxifsshare/last_path (plain text)
  textfmt    display helpers (whitespace collapse, truncation, sizes, date fields)
```

## Data flow

```
path -> locate() -> index XML bytes -> parse_index() -> Table
                 \-> FolderIndex::scan()  (one read_dir pass)
Table + FolderIndex -> keydetect::detect() -> key column
                    -> per-row Resolution { file, candidates }
                    -> ArchiveStats, per-file FileState, lower-cased search haystack
Archive::query(Filter, Sort) -> Vec<row index>      (what the table shows)
Archive::open_target(row)    -> validated path      (what the OS opener receives)
```

## Key decisions

| Decision | Why | Rejected |
|---|---|---|
| `quick-xml` 0.42 | streaming, pure Rust, one transitive dep (`memchr`), handles huge inputs | `roxmltree` (builds a DOM, extra memory at 100k rows), `xml-rs` (slow), serde deserialisation (columns are not known in advance), hand-written parser (risk) |
| Generic columns | schemas differ per archive | fixed struct |
| No date crate | only parsing, comparing and day filtering are needed | `chrono`, `time` |
| No `thiserror`/`anyhow` | one small enum with hand-written `Display` | extra dependencies for little gain |
| No `clap` | two modes and three flags | argument-parser dependency |
| Values never trimmed on load | a leading space exists in real data | trimming |
| Symlinks ignored | an archive has no need for them; avoids escapes | following links |
| Allow-list plus canonical-path check before opening | the OS opener could run a `.desktop` file or script | opening anything |
| Search haystack precomputed lower-case | a query is a substring scan over `Vec<String>` | per-query lower-casing |

### quick-xml 0.42 behaviour verified here

Entity references arrive as separate `GeneralRef` events between `Text` events
(`a &amp; b` is `Text("a ")`, `GeneralRef("amp")`, `Text(" b")`), and text keeps
its raw CRLF. The parser appends text and references into one value and
normalises line endings. Tests cover predefined entities, numeric references
at the start, middle and end of a value, an unknown entity, CDATA and CRLF.

## GUI (milestone 3)

Toolkit: `eframe`/`egui` 0.33.3 with `egui_extras` for the virtualised table,
`glow` (OpenGL) rendering, X11 and Wayland back ends. Versions are pinned with
`=` so a Dependabot bump is a deliberate, reviewed change (see below).

- **Layout:** top bar (archive path, search, type/kind/date filters), table in
  a horizontally scrollable area, details pane on the right, status bar.
- **State flow:** the UI holds a `Filter` and an optional `Sort`. Each frame it
  compares them with the previous query and calls `Archive::query` only when
  they changed, so idle frames cost nothing. The table draws only visible rows.
- **Loading:** `Archive::open` runs on a background thread; the window shows a
  spinner and stays responsive. A failed load keeps the previous archive visible
  and shows the error (with line and column for malformed XML).
- **Opening:** double-click or Enter calls `Archive::open_target` (allow-list,
  canonical path inside the archive), then the OS opener (`open` crate, which
  runs `xdg-open` detached). Unusual extensions ask for confirmation first.
  A missing `xdg-open` is reported with a hint to use Copy path.
- **Reveal:** opens the containing folder. Selecting the file inside the file
  manager would need D-Bus, which we avoid.
- **Remembered path:** one plain-text line under the XDG config directory.
- **Folder picker:** not included. The only pure-Rust route (`rfd` with the
  portal backend) pulls in a large async stack. Type or paste the path.
- **Testing without a display:** `App::ui` takes an `egui::Context`, so tests run
  real frames headless (no window, no GL) for loading, filtering, sorting,
  keyboard selection, the open/confirm/reveal flows with a recording opener,
  error paths and a 100,000-row archive.

### Runtime libraries

The binary is linked only against libc, libm and libgcc. X11, Wayland, EGL/GL
and xkbcommon are loaded at run time (`dlopen`), so building needs no `-dev`
packages (verified here: no `-dev` package was installed for the build except
what the sandbox already had; a pristine `debian:sid` container is still
unverified) but running needs the libraries installed. The `*-sys` crates in
the tree are those loaders; no C code is compiled (the one build script that
could, `wayland-backend`, only does so with its `log` feature, which is off).
The full GUI build is 147 crates; the headless build (`--no-default-features`)
is 4.

### Why egui

egui is the most widely used pure-Rust GUI toolkit (about 20 million `eframe`
downloads against about 3 million for `iced`), has an official virtualised table,
and its immediate-mode model keeps filter and sort state simple. Version
0.33.3 is the newest release that builds on the Rust 1.91 toolchain used for
verification; 0.34 and newer need Rust 1.92 or later.

## Release pipeline (milestone 4)

`.github/workflows/release.yml` runs on every push to `main` that touches more than
docs and workflows. Jobs: `licenses` (cargo-about builds `THIRD_PARTY_LICENSES.html`),
then in parallel `linux-sid` (`debian:sid` container, apt Rust, tests in release mode),
`appimage` (`debian:bookworm`, rustup stable, pinned and checksum-verified
`appimagetool` 1.9.1 and `type2-runtime` 20251108, `--version` smoke test) and
`windows` (MSVC target, static C runtime, zip). A final `release` job, the only one
with `contents: write`, writes `SHA256SUMS` and calls `gh release create`.

- **Tags:** the first run for a Cargo version creates `v<version>`; later runs create
  the pre-release `v<version>-build.<run_number>`. Tags made with `GITHUB_TOKEN` do not
  trigger workflows, so there is no loop; a concurrency group queues runs.
- **Pre-release cleanup:** only the 15 newest `-build.N` releases are kept.
- **Actions** are pinned by commit SHA (checkout, upload/download-artifact); Dependabot
  updates them. Tools downloaded in the AppImage job are pinned by version and SHA-256.
- **Not included:** `.deb`, aarch64, macOS (possible later).
- **AppImage caveat:** it bundles only the app. Graphics, windowing and `xdg-utils`
  come from the host, as with the other builds.

## Error handling

No panics on bad input: tests feed truncated, garbage, empty, UTF-16 and
invalid-UTF-8 inputs. A malformed index gives `Error::Xml { line, column, .. }`
(columns in bytes). Open failures are typed (`OpenError`): no file, unreadable,
not regular, outside the archive, or a type needing confirmation.

## Messages (milestone 5)

Opening a `.msg` row converts it with `msg::convert_file`, writes the `.eml` to
`EmlCache` (private directory, removed on drop) and opens that. Why convert at
all: the original viewer's mail files are Outlook-only on Linux. Why `cfb` (one
new pure-Rust dependency; it brings `fnv`, `uuid` and `web-time`; no `-sys` crates, no C): reading the
compound-file container correctly (FAT/mini-FAT, directory tree) is the hard
part and is well covered there; everything above it is ours. Rejected:
shelling out to external converters (breaks pure-Rust and offline rules),
rendering messages inside the app (the app never renders documents).
The converter never panics on bad input and enforces size and depth limits;
conversion errors carry a category, never content. Details: `docs/MSG.md`.

## Privacy contract

`--check` prints counts, percentages, column names, extensions and length
histograms only. It never prints values or file names. IDs of unmatched rows and
orphan files print only with `--show-ids`, and only if they are all digits and
at most 12 characters. A test generates data containing a canary string and
fails if the report or an error message contains it. The `[Messages]` section
reports counts and failure categories only; a test checks that message text does
not appear. Converted `.eml` files exist only in the private temporary folder.

## Performance (release build, this sandbox)

100,000 rows with 100,000 files: open (parse, folder scan, matching) about 1.3 s;
search with two terms plus a date sort about 50 ms; unfiltered query about 1 ms.
Run `cargo test --release -p oxifsshare-core -- --ignored --nocapture` to measure on your machine.

## Toolchain

MSRV 1.89 (lowest version verified by running the tests). The GUI crate may raise it in milestone 3.
