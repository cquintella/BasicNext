use std::{env, path::PathBuf, process::ExitCode};

use bn_support_matrix::build_report;

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn execute() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let json = match arguments.next().as_deref() {
        None => false,
        Some("--json") => true,
        Some(argument) => return Err(format!("unknown argument `{argument}`; expected --json")),
    };
    if let Some(argument) = arguments.next() {
        return Err(format!("unexpected argument `{argument}`"));
    }

    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("support-matrix crate lives under crates/")
        .to_owned();
    let report = build_report(&repository_root).map_err(|error| error.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
    } else {
        println!("{}", report.summary());
    }
    Ok(())
}
