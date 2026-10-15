//! Writes a synthetic archive (fake data only) so you can try the app and `--check`.
//!
//! cargo run -p oxifsshare-core --example gen_fixture -- OUT_DIR [ROWS] [plain|financial|property]

use std::{env, path::PathBuf, process::ExitCode};

use oxifsshare_core::testgen::{synth_archive_in, Spec, Variant};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(out) = args.next().map(PathBuf::from) else {
        eprintln!("usage: gen_fixture OUT_DIR [ROWS] [plain|financial|property]");
        return ExitCode::from(2);
    };
    let rows = args.next().and_then(|s| s.parse().ok()).unwrap_or(555);
    let variant = match args.next().as_deref() {
        Some("financial") => Variant::Financial,
        Some("property") => Variant::Property,
        _ => Variant::Plain,
    };
    let spec = Spec {
        rows,
        variant,
        missing_every: 40,
        orphans: 3,
        txt_file: true,
        duplicates: 2,
        upper_ext: true,
        crlf: true,
        ..Spec::default()
    };
    match synth_archive_in(&out, &spec) {
        Ok((root, _, exp)) => {
            println!(
                "wrote {} rows ({} with files, {} missing, {} orphans) under {}",
                exp.rows,
                exp.with_file,
                exp.missing,
                exp.orphans,
                root.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gen_fixture: {e}");
            ExitCode::from(1)
        }
    }
}
