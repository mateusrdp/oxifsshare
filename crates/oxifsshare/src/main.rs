//! Thin command-line front end for oxifsshare.
//!
//! * `oxifsshare --check ARCHIVE [--show-ids]` prints a structure-only report.
//! * `oxifsshare [ARCHIVE]` opens the GUI (needs the default `gui` feature).

#[cfg(feature = "gui")]
mod gui;
#[cfg(feature = "gui")]
mod picker;
#[cfg(feature = "gui")]
mod settings;
#[cfg(feature = "gui")]
mod textfmt;

use std::{path::PathBuf, process::ExitCode};

const USAGE: &str = "\
Usage:
  oxifsshare [ARCHIVE]                    open the GUI
  oxifsshare --check ARCHIVE [--show-ids] print a report of counts and match rates only
  oxifsshare --version | --help

ARCHIVE is the folder holding fs-share.xml, its parent folder, or the XML file.
--check never prints values from the archive or file names. --show-ids additionally
prints the numeric IDs of rows without a file and of files without a row.";

fn main() -> ExitCode {
    let mut check = false;
    let mut show_ids = false;
    let mut path: Option<PathBuf> = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("--version" | "-V") => {
                println!("oxifsshare {}", oxifsshare_core::version());
                return ExitCode::SUCCESS;
            }
            Some("--help" | "-h") => {
                println!("oxifsshare {}\n\n{USAGE}", oxifsshare_core::version());
                return ExitCode::SUCCESS;
            }
            Some("--check") => check = true,
            Some("--show-ids") => show_ids = true,
            Some(s) if s.starts_with("--") => return usage(&format!("unknown option {s}")),
            _ => {
                if path.replace(PathBuf::from(arg)).is_some() {
                    return usage("only one ARCHIVE path may be given");
                }
            }
        }
    }
    if show_ids && !check {
        return usage("--show-ids only works together with --check");
    }
    if check {
        let Some(path) = path else {
            return usage("--check needs an ARCHIVE path");
        };
        return match oxifsshare_core::check::run(&path, show_ids) {
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("oxifsshare: {e}");
                ExitCode::from(1)
            }
        };
    }
    launch_gui(path)
}

#[cfg(feature = "gui")]
fn launch_gui(path: Option<PathBuf>) -> ExitCode {
    match gui::run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oxifsshare: cannot open a window: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(feature = "gui"))]
fn launch_gui(_path: Option<PathBuf>) -> ExitCode {
    eprintln!(
        "oxifsshare: this build has no GUI (built without the `gui` feature); use --check ARCHIVE"
    );
    ExitCode::from(2)
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("oxifsshare: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}
