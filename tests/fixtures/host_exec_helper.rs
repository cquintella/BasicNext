//! Deterministic executable used by HOST.Exec acceptance fixtures.
//! It deliberately has no shell semantics: each mode is an argv token.
use std::io::{self, Read, Write};

#[cfg(unix)]
unsafe extern "C" {
    fn raise(sig: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("stdout") => print!("{}", args.next().unwrap_or_default()),
        Some("stderr") => eprint!("{}", args.next().unwrap_or_default()),
        Some("status") => std::process::exit(args.next().and_then(|v| v.parse().ok()).unwrap_or(0)),
        Some("argv") => {
            let mut out = io::stdout().lock();
            for (index, value) in args.enumerate() {
                let _ = writeln!(out, "{index}={value}");
            }
        }
        Some("echo-stdin") => {
            let mut input = String::new();
            let _ = io::stdin().read_to_string(&mut input);
            print!("{input}");
        }
        Some("invalid-utf8") => {
            let _ = io::stdout().write_all(&[0xff, 0xfe]);
        }
        Some("invalid-utf8-stderr") => {
            let _ = io::stderr().write_all(&[0xff, 0xfe]);
        }
        Some("block") => std::thread::sleep(std::time::Duration::from_secs(120)),
        Some("touch") => {
            let path = args.next().expect("touch path");
            std::fs::write(&path, b"marker\n").expect("write marker");
        }
        Some("both") => {
            let size: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let payload = vec![b'A'; size];
            let err_payload = vec![b'B'; size];
            let out = std::thread::spawn(move || {
                let _ = io::stdout().write_all(&payload);
            });
            let err = std::thread::spawn(move || {
                let _ = io::stderr().write_all(&err_payload);
            });
            let _ = out.join();
            let _ = err.join();
        }
        Some("stdout-bytes") => {
            let size: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let payload = vec![b'X'; size];
            let _ = io::stdout().write_all(&payload);
        }
        Some("cwd") => {
            let cwd = std::env::current_dir().expect("cwd");
            print!("{}", cwd.display());
        }
        Some("env") => {
            let key = args.next().unwrap_or_default();
            match std::env::var(&key) {
                Ok(value) => print!("{value}"),
                Err(_) => eprint!("missing:{key}"),
            }
        }
        Some("signal") => {
            #[cfg(unix)]
            {
                unsafe {
                    raise(SIGTERM);
                }
                std::process::exit(99);
            }
            #[cfg(not(unix))]
            {
                std::process::exit(99);
            }
        }
        _ => std::process::exit(64),
    }
}
