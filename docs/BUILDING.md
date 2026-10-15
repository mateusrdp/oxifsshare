# Building and running oxifsshare

> **Verification status.** The sandbox used to develop this project has no Docker
> and runs Ubuntu 24.04, not Debian sid. The commands below are therefore
> **UNVERIFIED on a clean `debian:sid`**. What *was* verified (on Ubuntu 24.04 with
> Rust 1.91 and 1.89): the cargo commands, the tests, and running the GUI under a
> virtual X display with software OpenGL after installing `libgl1-mesa-dri`,
> `libglx-mesa0`, `libegl1`, `libxkbcommon-x11-0` and `x11-utils`. The package
> *names* for sid are the same names Debian uses, but please confirm them with the
> container recipe at the end and report differences.

Most people do not need to build: download the Debian sid `.deb` (`sudo apt install ./oxifsshare_*.deb`), the AppImage or
the Windows zip from the GitHub Releases page.

## 1. Rust toolchain: apt or rustup?

| | `apt install cargo rustc` | rustup |
|---|---|---|
| Version | Whatever sid ships; must be at least the MSRV in `Cargo.toml` (1.89) | Always current stable |
| Updates | With the system | `rustup update` |
| Fits Debian policy | Yes | Installs into `~/.cargo` |

**Recommendation:** use the apt packages on Debian sid, because sid tracks recent
Rust. Check with `rustc --version`; if it is older than 1.89, use rustup:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The release workflow builds the sid `.deb` with the apt toolchain, so if apt's
Rust ever falls below the MSRV, that job fails visibly.

## 2. Packages

Build (linker and C toolchain; no `-dev` packages are needed because the
graphics, windowing and keyboard libraries are loaded at run time):

```
sudo apt install build-essential cargo rustc git
```

Run (needed by the GUI at run time; `--check` needs none of these):

```
sudo apt install xdg-utils libgl1 libegl1 libgl1-mesa-dri libxkbcommon0 libxkbcommon-x11-0 \
                 libwayland-client0 libx11-6 libxcursor1 libxrandr2 libxi6
```

`xdg-utils` provides `xdg-open`, which opens documents. A normal desktop install
already has all of these.

## 3. Build, test, run

```
git clone https://github.com/OWNER/oxifsshare.git
cd oxifsshare
cargo build --release --locked
cargo test --workspace --locked
./target/release/oxifsshare /path/to/fsshare      # GUI
```

The path may be the folder holding `fs-share.xml`, its parent, or the XML file.
Without a path the GUI opens empty (use Browse… or type a path); the last path you used is only filled in, not loaded.

A GUI-less build (CLI and `--check` only, 4 crates instead of about 150):

```
cargo build --release --locked -p oxifsshare --no-default-features
```

## 4. The `--check` report

Prints counts, percentages, column names and file extensions only, never values or file names:

```
./target/release/oxifsshare --check /path/to/fsshare
./target/release/oxifsshare --check /path/to/fsshare --show-ids   # also numeric IDs of unmatched rows and files
```

Try it on fake data:

```
cargo run -p oxifsshare-core --example gen_fixture -- /tmp/demo 555 financial
cargo run -p oxifsshare -- --check /tmp/demo
cargo run -p oxifsshare -- /tmp/demo
```

## 5. AppImage and Windows

- **AppImage:** `chmod +x oxifsshare-*-x86_64.AppImage && ./oxifsshare-*-x86_64.AppImage /path/to/fsshare`. Built on Debian 12, so it needs glibc 2.36 or newer. Without FUSE, run it with `APPIMAGE_EXTRACT_AND_RUN=1`. It still needs `xdg-utils` and the graphics libraries from section 2 on the host.
- **Windows portable:** unzip and run `oxifsshare.exe`. Nothing is installed; the last-used path is stored in `%APPDATA%\oxifsshare\last_path`. **The Windows build has been cross-checked in CI configuration only and has not been run on Windows by the author.**

## 6. Recipe to verify section 2 in a clean container

```
docker run --rm -it -v "$PWD":/src -w /src debian:sid bash -c '
  apt-get update &&
  apt-get install -y --no-install-recommends ca-certificates build-essential cargo rustc git &&
  rustc --version &&
  cargo build --release --locked &&
  cargo test --workspace --locked &&
  ./target/release/oxifsshare --version'
```

If that works, the build list is confirmed. For the run list, add the run packages
and start the GUI with an X server or `xvfb-run` and report back.
