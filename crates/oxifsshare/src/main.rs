//! Thin command-line / GUI front end for oxifsshare.
//!
//! Milestone 1: placeholder only. `--check` arrives in milestone 2 and the
//! GUI in milestone 3.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("oxifsshare {}", oxifsshare_core::version());
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") | None => {
            println!(
                "oxifsshare {}\n\nUsage:\n  oxifsshare [ARCHIVE]          open the GUI (milestone 3)\n  oxifsshare --check ARCHIVE    print a structure-only report (milestone 2)\n  oxifsshare --version",
                oxifsshare_core::version()
            );
            ExitCode::SUCCESS
        }
        Some(_) => {
            eprintln!("oxifsshare: not implemented yet (milestone 1 scaffold)");
            ExitCode::from(2)
        }
    }
}
