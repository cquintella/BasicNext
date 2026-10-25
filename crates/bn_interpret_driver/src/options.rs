// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The interpreter's own options (`--trace`, `--no-filesystem`,
//! `--jupyter-stdin`, `--format`, `--ffi-path`), parsed as a
//! `bn_cli::options::OptionExtension`. The common parser never learns them,
//! so `bnc` refuses each as an unknown option (known issue K3).

use std::path::PathBuf;

use bn_cli::options::{OptionExtension, OutputFormat};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileFormat {
    Table,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterpreterOptions {
    /// `--trace`: report the execution entry point.
    pub trace: bool,
    /// `--trace-events <file>`: record NDJSON execution event stream to file.
    pub trace_events: Option<PathBuf>,
    /// `--profile table|json`: profile execution timings and display summary upon exit.
    pub profile: Option<ProfileFormat>,
    /// `false` after `--no-filesystem`: deny `HOST.FileSystem` imports.
    pub filesystem: bool,
    /// `--jupyter-stdin`: announce `INPUT` requests on stderr.
    pub jupyter_stdin: bool,
    /// `--format text|json` (`bni eval` only).
    pub output_format: OutputFormat,
    /// `--ffi-path <dir>` (repeatable): ordered dynamic library search directories (`bni` only).
    pub ffi_paths: Vec<PathBuf>,
}

impl Default for InterpreterOptions {
    fn default() -> Self {
        Self {
            trace: false,
            trace_events: None,
            profile: None,
            filesystem: true,
            jupyter_stdin: false,
            output_format: OutputFormat::Text,
            ffi_paths: Vec::new(),
        }
    }
}

impl OptionExtension for InterpreterOptions {
    fn accept(
        &mut self,
        argument: &str,
        rest: &mut dyn Iterator<Item = String>,
    ) -> Result<bool, String> {
        match argument {
            "--trace" => self.trace = true,
            "--trace-events" => {
                let file = rest
                    .next()
                    .filter(|val| !val.starts_with('-'))
                    .ok_or_else(|| bn_catalog::text("cli", "option.trace_events.expects"))?;
                self.trace_events = Some(PathBuf::from(file));
            }
            "--profile" => {
                self.profile = match rest.next().as_deref() {
                    Some("table") => Some(ProfileFormat::Table),
                    Some("json") => Some(ProfileFormat::Json),
                    _ => return Err(bn_catalog::text("cli", "option.profile.expects").into()),
                };
            }
            "--no-filesystem" => self.filesystem = false,
            "--jupyter-stdin" => self.jupyter_stdin = true,
            "--ffi-path" => {
                self.ffi_paths.push(PathBuf::from(
                    rest.next()
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| "--ffi-path expects a directory".to_string())?,
                ));
            }
            "--format" => {
                self.output_format = match rest.next().as_deref() {
                    Some("text") => OutputFormat::Text,
                    Some("json") => OutputFormat::Json,
                    _ => return Err(bn_catalog::text("cli", "option.format.expects").into()),
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::InterpreterOptions;
    use bn_cli::options::{OutputFormat, parse_options};

    fn parse(arguments: &[&str]) -> Result<InterpreterOptions, String> {
        let mut interpreter = InterpreterOptions::default();
        parse_options(arguments.iter().map(ToString::to_string), &mut interpreter)?;
        Ok(interpreter)
    }

    #[test]
    fn interpreter_options_are_parsed_by_the_interpreter_only() {
        let parsed = parse(&[
            "--trace",
            "--no-filesystem",
            "--jupyter-stdin",
            "--format",
            "json",
            "x.bn",
        ])
        .expect("interpreter options");
        assert!(parsed.trace && !parsed.filesystem && parsed.jupyter_stdin);
        assert_eq!(parsed.output_format, OutputFormat::Json);
        assert!(parse(&["--format", "xml", "x.bn"]).is_err());
        // A compiler option is unknown to the interpreter.
        assert!(parse(&["--target", "native", "x.bn"]).is_err());
        assert!(parse(&["--emit", "ir", "x.bn"]).is_err());
        // Without the extension, an interpreter option is unknown (bnc).
        assert!(
            parse_options(["--trace", "x.bn"].iter().map(ToString::to_string), &mut ()).is_err()
        );
        assert!(
            parse_options(
                ["--ffi-path", "/opt/lib", "x.bn"]
                    .iter()
                    .map(ToString::to_string),
                &mut ()
            )
            .is_err()
        );
    }

    #[test]
    fn ffi_path_is_repeatable_and_ordered_in_interpreter() {
        let parsed = parse(&[
            "--ffi-path",
            "/usr/local/lib",
            "--ffi-path",
            "/opt/lib",
            "x.bn",
        ])
        .expect("ffi path flags");
        assert_eq!(
            parsed.ffi_paths,
            vec![
                std::path::PathBuf::from("/usr/local/lib"),
                std::path::PathBuf::from("/opt/lib")
            ]
        );
    }
}
