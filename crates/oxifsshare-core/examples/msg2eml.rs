//! Converts one .msg to .eml: `msg2eml IN.msg OUT.eml`. Prints only counts.
use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: msg2eml IN.msg OUT.eml");
        return ExitCode::from(2);
    };
    match oxifsshare_core::msg::convert_file(Path::new(input)) {
        Ok(c) => {
            if let Err(e) = fs::write(output, &c.eml) {
                eprintln!("write failed: {e}");
                return ExitCode::from(1);
            }
            println!("ok {:?}", c.info);
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("error {} ({})", e.category(), e);
            ExitCode::from(1)
        }
    }
}
