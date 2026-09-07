// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const VERSION: &str = concat!("bnc ", env!("CARGO_PKG_VERSION"));

#[derive(Debug, PartialEq, Eq)]
enum Profile {
    Interpret,
    Compile,
    Check,
}

#[derive(Debug)]
struct BncOptions {
    entry: String,
    profile: Profile,
    target: Option<String>,
    output: Option<String>,
    opt: Option<String>,
    config: Option<String>,
    #[allow(dead_code)]
    programs_dir: Option<String>,
    #[allow(dead_code)]
    module_paths: Vec<String>,
    #[allow(dead_code)]
    plugins_dir: Option<String>,
    log_level: Option<String>,
    log_file: Option<String>,
    log_dir: Option<String>,
    no_log: bool,
    color: Option<String>,
    quiet: bool,
    verbose: u8,
    no_filesystem: bool,
    warnings: Option<String>,
    allows: Vec<String>,
    warns: Vec<String>,
    denies: Vec<String>,
    program_args: Vec<String>,
}

fn tool_error() -> ExitCode {
    ExitCode::from(2)
}

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
Usage: bnc [options] <entry.bn> [-- <program-arguments>...]

Profiles (flag-shaped):
  bnc prog.bn                      Run via interpreter (default)
  bnc -c prog.bn                   Compile for current host
  bnc -c --target <plat> prog.bn   Compile for target platform (e.g. wasm32, native)
  bnc --check prog.bn              Check (analyze only; no run or emit)

Compile options:
  -c, --compile                    Compile instead of interpret
  --target <plat>                  Build target (requires -c; e.g. native, wasm32)
  -o, --output <path>              Output artifact path
  --opt <none|1|2|3|s>             Optimization level

Check options:
  --check                          Analyze only; mutually exclusive with -c

General options:
  --config <file>                  Configuration file path
  --programs-dir <dir>             Project/programs root directory
  --module-path <dir>              Ordered module search directory (repeatable)
  --plugins-dir <dir>              Reserved plugins directory
  --log-level <level>              Syslog level (emerg..debug, error, warn, info)
  --log-file <path>                Explicit process log path
  --log-dir <dir>                  Directory for process logs
  --no-log                         Disable companion process log file
  --no-filesystem                  Deny HOST.FileSystem imports (interpret)
  -q, --quiet                      Quiet mode (equivalent to --log-level warning)
  -v, --verbose                    Verbose mode (can be repeated)
  --color <auto|always|never>      Control ANSI color output
  --warnings errors                Promote all warnings to errors
  --allow <CODE>                   Suppress one warning code
  --warn <CODE>                    Keep one code at warning level
  --deny <CODE>                    Promote one code to error
  -V, --version                    Print bnc version
  -h, --help                       Print this help message
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("Usage: bnc [options] <entry.bn>\nTry: bnc --help");
    tool_error()
}

fn normalize_log_level(level: &str) -> Result<&'static str, String> {
    match level.to_lowercase().as_str() {
        "emerg" | "alert" | "crit" | "err" | "error" => Ok("error"),
        "warning" | "warn" => Ok("warn"),
        "notice" | "info" => Ok("info"),
        "debug" | "trace" => Ok("debug"),
        other => Err(format!(
            "unknown log level '{other}'; expected error, warn, info, or debug"
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_args(args: impl Iterator<Item = String>) -> Result<BncOptions, String> {
    let mut args = args.peekable();
    let mut entry = None;
    let mut compile_flag = false;
    let mut check_flag = false;
    let mut target = None;
    let mut output = None;
    let mut opt = None;
    let mut config = None;
    let mut programs_dir = None;
    let mut module_paths = Vec::new();
    let mut plugins_dir = None;
    let mut log_level = None;
    let mut log_file = None;
    let mut log_dir = None;
    let mut no_log = false;
    let mut color = None;
    let mut quiet = false;
    let mut verbose = 0u8;
    let mut no_filesystem = false;
    let mut warnings = None;
    let mut allows = Vec::new();
    let mut warns = Vec::new();
    let mut denies = Vec::new();
    let mut program_args = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                program_args.extend(args);
                break;
            }
            "-c" | "--compile" => {
                compile_flag = true;
            }
            "--check" => {
                check_flag = true;
            }
            "--target" => {
                let val = args.next().ok_or("--target requires a platform argument")?;
                target = Some(val);
            }
            "-o" | "--output" => {
                let val = args.next().ok_or("-o/--output requires a path argument")?;
                output = Some(val);
            }
            "--opt" => {
                let val = args.next().ok_or("--opt requires an optimization level")?;
                opt = Some(val);
            }
            "--config" => {
                let val = args.next().ok_or("--config requires a file path")?;
                config = Some(val);
            }
            "--programs-dir" => {
                let val = args
                    .next()
                    .ok_or("--programs-dir requires a directory path")?;
                programs_dir = Some(val);
            }
            "--module-path" => {
                let val = args
                    .next()
                    .ok_or("--module-path requires a directory path")?;
                module_paths.push(val);
            }
            "--plugins-dir" => {
                let val = args
                    .next()
                    .ok_or("--plugins-dir requires a directory path")?;
                plugins_dir = Some(val);
            }
            "--log-level" => {
                let raw = args.next().ok_or("--log-level requires a level argument")?;
                let normalized = normalize_log_level(&raw)?;
                log_level = Some(normalized.to_string());
            }
            "--log-file" => {
                let val = args.next().ok_or("--log-file requires a path argument")?;
                log_file = Some(val);
            }
            "--log-dir" => {
                let val = args
                    .next()
                    .ok_or("--log-dir requires a directory argument")?;
                log_dir = Some(val);
            }
            "--no-log" => {
                no_log = true;
            }
            "--color" => {
                let val = args
                    .next()
                    .ok_or("--color requires auto, always, or never")?;
                if !matches!(val.as_str(), "auto" | "always" | "never") {
                    return Err(format!(
                        "--color expects auto, always, or never (got {val})"
                    ));
                }
                color = Some(val);
            }
            "-q" | "--quiet" => {
                quiet = true;
            }
            "-v" | "--verbose" => {
                verbose = verbose.saturating_add(1);
            }
            "-vv" => {
                verbose = 2;
            }
            "--no-filesystem" => {
                no_filesystem = true;
            }
            "--warnings" => {
                let val = args.next().ok_or("--warnings requires a mode")?;
                warnings = Some(val);
            }
            "--allow" => {
                let val = args.next().ok_or("--allow requires a diagnostic code")?;
                allows.push(val);
            }
            "--warn" => {
                let val = args.next().ok_or("--warn requires a diagnostic code")?;
                warns.push(val);
            }
            "--deny" => {
                let val = args.next().ok_or("--deny requires a diagnostic code")?;
                denies.push(val);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option '{other}'"));
            }
            positional => {
                if entry.is_some() {
                    return Err(format!("unexpected additional argument '{positional}'"));
                }
                entry = Some(positional.to_string());
            }
        }
    }

    if compile_flag && check_flag {
        return Err("cannot specify both -c/--compile and --check".into());
    }

    if target.is_some() && !compile_flag {
        return Err("--target requires -c/--compile".into());
    }

    let entry = entry.ok_or_else(|| "missing entry .bn source file".to_string())?;

    let profile = if compile_flag {
        Profile::Compile
    } else if check_flag {
        Profile::Check
    } else {
        Profile::Interpret
    };

    Ok(BncOptions {
        entry,
        profile,
        target,
        output,
        opt,
        config,
        programs_dir,
        module_paths,
        plugins_dir,
        log_level,
        log_file,
        log_dir,
        no_log,
        color,
        quiet,
        verbose,
        no_filesystem,
        warnings,
        allows,
        warns,
        denies,
        program_args,
    })
}

fn locate_bn_binary() -> PathBuf {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("bn");
        if sibling.is_file() {
            return sibling;
        }
        let sibling_exe = dir.join("bn.exe");
        if sibling_exe.is_file() {
            return sibling_exe;
        }
    }
    PathBuf::from("bn")
}

fn build_bn_command(options: &BncOptions) -> Command {
    let bn_bin = locate_bn_binary();
    let mut cmd = Command::new(bn_bin);

    // Profile command
    match options.profile {
        Profile::Interpret => {
            cmd.arg("run");
        }
        Profile::Compile => {
            cmd.arg("build");
        }
        Profile::Check => {
            cmd.arg("check");
        }
    }

    // Target (compile only)
    if let Some(target) = &options.target {
        cmd.arg("--target").arg(target);
    }

    // Output
    if let Some(output) = &options.output {
        cmd.arg("-o").arg(output);
    }

    // Opt
    if let Some(opt) = &options.opt {
        cmd.arg("--opt").arg(opt);
    }

    // Config
    if let Some(config) = &options.config {
        cmd.arg("--config").arg(config);
    }

    // Color
    if let Some(color) = &options.color {
        cmd.arg("--color").arg(color);
    }

    // Verbosity
    if options.verbose == 1 {
        cmd.arg("-v");
    } else if options.verbose >= 2 {
        cmd.arg("-vv");
    }

    // Logging options
    if options.no_log {
        cmd.arg("--no-log");
    } else {
        if let Some(log_file) = &options.log_file {
            cmd.arg("--log-file").arg(log_file);
        } else if let Some(log_dir) = &options.log_dir {
            let entry_name = Path::new(&options.entry)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("program");
            let candidate = Path::new(log_dir).join(format!("{entry_name}.bnbuild.log"));
            cmd.arg("--log-file").arg(candidate);
        } else if options.profile == Profile::Compile && options.output.is_none() {
            // Default companion log for compile
            let candidate = Path::new(&options.entry).with_extension("bnbuild.log");
            cmd.arg("--log-file").arg(candidate);
        }

        if let Some(level) = &options.log_level {
            cmd.arg("--log-level").arg(level);
        } else if options.quiet {
            cmd.arg("--log-level").arg("warn");
        }
    }

    // Filesystem capability
    if options.no_filesystem {
        cmd.arg("--no-filesystem");
    }

    // Warning policy
    if let Some(warnings) = &options.warnings {
        cmd.arg("--warnings").arg(warnings);
    }
    for allow in &options.allows {
        cmd.arg("--allow").arg(allow);
    }
    for warn in &options.warns {
        cmd.arg("--warn").arg(warn);
    }
    for deny in &options.denies {
        cmd.arg("--deny").arg(deny);
    }

    // Entry point file
    cmd.arg(&options.entry);

    // Forward program args
    if !options.program_args.is_empty() {
        cmd.arg("--");
        for arg in &options.program_args {
            cmd.arg(arg);
        }
    }

    cmd
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        return usage();
    };

    match first.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let all_args = std::iter::once(first).chain(args);
    let options = match parse_args(all_args) {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("error[BNC]: {err}");
            return usage();
        }
    };

    let mut command = build_bn_command(&options);
    match command.status() {
        Ok(status) => {
            if let Some(code) = status.code() {
                ExitCode::from(u8::try_from(code).unwrap_or(1))
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("error[BNC_ENGINE]: failed to execute bn engine: {err}");
            tool_error()
        }
    }
}
