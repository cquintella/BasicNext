//! Common command-line options shared by every Basic Next executable.
//! Backend-specific flags are parsed through [`OptionExtension`], supplied by
//! the driver, so this parser never learns about targets or interpreters.

use std::path::PathBuf;

use bn_diag::{Catalog, DiagId, Level, WarningPolicy};
use bn_frontend::module_graph::{ModuleRoot, RootProvenance};

use crate::{
    config::{config_path_from_arguments, configured_settings},
    process_log::LogLevel,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Emit {
    Tokens,
    Ast,
    TypedAst,
    Ir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug)]
pub enum Color {
    Auto,
    Always,
    Never,
}

/// Source-coordinate remapping applied to diagnostics when the driver wrapped
/// the user's text (e.g. `eval` snippets) before preparation.
#[derive(Clone, Debug)]
pub struct EvalMapping {
    pub source_name: String,
    pub insertion_offset: usize,
    pub inserted_length: usize,
    pub insertion_line: usize,
    pub expression_prefix: Option<(usize, usize)>,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // CLI flags map directly to independent policies.
pub struct Options {
    pub path: String,
    pub verbosity: u8,
    pub emit: Option<Emit>,
    pub output: Option<String>,
    pub output_format: OutputFormat,
    pub eval_mapping: Option<EvalMapping>,
    pub eval_source_text: Option<String>,
    pub eval_promotion_warning: bool,
    pub eval_promotion_span: Option<bn_source::Span>,
    pub trace: bool,
    pub color: Color,
    pub filesystem: bool,
    pub sandbox: bool,
    pub module_paths: Vec<ModuleRoot>,
    pub read_roots: Vec<PathBuf>,
    pub write_roots: Vec<PathBuf>,
    pub jupyter_stdin: bool,
    pub program_arguments: Vec<String>,
    pub warning_policy: WarningPolicy,
    pub diagnostic_catalog: Catalog,
    pub log_level: LogLevel,
    pub log_file: Option<String>,
    pub no_log: bool,
}

/// Driver-supplied parser for flags the common set does not know.
pub trait OptionExtension {
    /// Consumes `argument` (pulling values from `rest` as needed) and returns
    /// `Ok(true)`, or returns `Ok(false)` to let the common parser decide.
    ///
    /// # Errors
    ///
    /// Returns the usage message for a malformed extension flag.
    fn accept(
        &mut self,
        argument: &str,
        rest: &mut dyn Iterator<Item = String>,
    ) -> Result<bool, String>;
}

/// No extension flags.
impl OptionExtension for () {
    fn accept(&mut self, _: &str, _: &mut dyn Iterator<Item = String>) -> Result<bool, String> {
        Ok(false)
    }
}

/// Parses the common options (after the command word), consulting `extension`
/// for anything unknown before rejecting it.
///
/// # Errors
///
/// Returns the usage message; `CONFIG_INVALID: ` prefixes configuration errors.
#[allow(clippy::too_many_lines)] // CLI options remain auditable in one parser.
pub fn parse_options(
    arguments: impl Iterator<Item = String>,
    extension: &mut dyn OptionExtension,
) -> Result<Options, String> {
    let arguments = arguments.collect::<Vec<_>>();
    let config_path = config_path_from_arguments(&arguments)?;
    let configured = configured_settings(config_path.as_deref())?;
    let mut warning_policy = configured.warning_policy;
    let diagnostic_catalog = Catalog::selected(configured.diagnostics_dir.as_deref())
        .map_err(|error| format!("CONFIG_INVALID: cannot load diagnostic catalog: {error}"))?;
    let mut arguments = arguments.into_iter();
    let mut path = None;
    let mut verbosity = 0u8;
    let mut emit = None;
    let mut output = None;
    let mut output_format = OutputFormat::Text;
    let mut trace = false;
    let mut color = Color::Auto;
    let mut filesystem = true;
    let mut sandbox = false;
    let mut read_roots = Vec::new();
    let mut write_roots = Vec::new();
    let mut jupyter_stdin = false;
    let mut program_arguments = Vec::new();
    let mut log_level = configured.log_level;
    let mut log_file = configured.log_file;
    let mut log_file_from_cli = false;
    let mut no_log = configured.no_log;
    let mut module_paths: Vec<ModuleRoot> = configured
        .module_paths
        .into_iter()
        .map(|path| ModuleRoot {
            path,
            provenance: RootProvenance::Config,
        })
        .collect();
    while let Some(argument) = arguments.next() {
        if extension.accept(&argument, &mut arguments)? {
            continue;
        }
        match argument.as_str() {
            "--" => {
                program_arguments.extend(arguments);
                break;
            }
            "-h" | "--help" => return Err("help is available as bn --help".into()),
            "-v" | "--verbose" => verbosity = verbosity.saturating_add(1).min(2),
            "-vv" => verbosity = 2,
            "--trace" => trace = true,
            "--no-filesystem" => filesystem = false,
            "--sandbox" => sandbox = true,
            "--read-root" => read_roots.push(PathBuf::from(
                arguments
                    .next()
                    .filter(|v| !v.starts_with('-'))
                    .ok_or("--read-root expects a directory".to_string())?,
            )),
            "--write-root" => write_roots.push(PathBuf::from(
                arguments
                    .next()
                    .filter(|v| !v.starts_with('-'))
                    .ok_or("--write-root expects a directory".to_string())?,
            )),
            "--jupyter-stdin" => jupyter_stdin = true,
            "--module-path" => module_paths.push(ModuleRoot {
                path: PathBuf::from(
                    arguments
                        .next()
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| "--module-path expects a directory".to_string())?,
                ),
                provenance: RootProvenance::CliFlag,
            }),
            "--config" => {
                let _ = arguments
                    .next()
                    .ok_or_else(|| "--config expects a file path".to_string())?;
            }
            "--log-level" => {
                log_level = LogLevel::parse(
                    &arguments
                        .next()
                        .ok_or_else(|| "--log-level expects a value".to_string())?,
                )?;
            }
            "--log-file" => {
                if log_file_from_cli {
                    return Err("--log-file was specified more than once".into());
                }
                log_file_from_cli = true;
                no_log = false;
                log_file = Some(
                    arguments
                        .next()
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| "--log-file expects a file path".to_string())?,
                );
            }
            "--no-log" => no_log = true,
            "--warnings" => {
                if arguments.next().as_deref() != Some("errors") {
                    return Err("--warnings expects errors".into());
                }
                warning_policy.set_warnings_as_errors(true);
            }
            "--allow" | "--deny" | "--warn" => {
                let option = argument.as_str();
                let code = arguments
                    .next()
                    .ok_or_else(|| format!("{option} expects a diagnostic code"))?;
                let id = DiagId::from_code(&code)
                    .ok_or_else(|| format!("unknown diagnostic code '{code}'"))?;
                let level = match option {
                    "--allow" => Level::Allow,
                    "--deny" => Level::Error,
                    "--warn" => Level::Warn,
                    _ => unreachable!("matched warning option"),
                };
                warning_policy.set_cli(id, level)?;
            }
            "--emit" => {
                if emit.is_some() {
                    return Err("--emit was specified more than once".into());
                }
                emit = Some(match arguments.next().as_deref() {
                    Some("tokens") => Emit::Tokens,
                    Some("ast") => Emit::Ast,
                    Some("typed-ast") => Emit::TypedAst,
                    Some("ir") => Emit::Ir,
                    _ => return Err("--emit expects tokens, ast, typed-ast, or ir".into()),
                });
            }
            "-o" | "--output" => {
                if output.is_some() {
                    return Err("output file was specified more than once".into());
                }
                output = Some(
                    arguments
                        .next()
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| "-o expects an output file path".to_string())?,
                );
            }
            "--format" => {
                output_format = match arguments.next().as_deref() {
                    Some("text") => OutputFormat::Text,
                    Some("json") => OutputFormat::Json,
                    _ => return Err("--format expects text or json".into()),
                };
            }
            "--color" => {
                color = match arguments.next().as_deref() {
                    Some("auto") => Color::Auto,
                    Some("always") => Color::Always,
                    Some("never") => Color::Never,
                    _ => return Err("--color expects auto, always, or never".into()),
                }
            }
            _ if path.is_none() && !argument.starts_with('-') => path = Some(argument),
            _ => return Err(format!("unknown or repeated option '{argument}'")),
        }
    }
    if sandbox && !filesystem {
        return Err("--sandbox cannot be combined with --no-filesystem".into());
    }
    if (!read_roots.is_empty() || !write_roots.is_empty()) && !sandbox {
        return Err("--read-root/--write-root require --sandbox".into());
    }
    path.map(|path| Options {
        path,
        verbosity,
        emit,
        output,
        output_format,
        eval_mapping: None,
        eval_source_text: None,
        eval_promotion_warning: false,
        eval_promotion_span: None,
        trace,
        color,
        filesystem,
        sandbox,
        read_roots,
        write_roots,
        module_paths,
        jupyter_stdin,
        program_arguments,
        warning_policy,
        diagnostic_catalog,
        log_level,
        log_file,
        no_log,
    })
    .ok_or_else(|| "missing source file".into())
}

#[cfg(test)]
mod tests {
    use super::parse_options;
    use bn_diag::{DiagId, Level};

    fn parse(arguments: &[&str]) -> Result<super::Options, String> {
        parse_options(arguments.iter().map(ToString::to_string), &mut ())
    }

    #[test]
    fn warning_flags_use_typed_codes_and_cli_precedence() {
        let options = parse(&[
            "--warnings",
            "errors",
            "--warn",
            "UNUSED_BINDING",
            "file.bn",
        ])
        .expect("warning flags");
        assert_eq!(
            options.warning_policy.level(DiagId::UNUSED_BINDING),
            Level::Warn
        );
        assert_eq!(
            options.warning_policy.level(DiagId::UNUSED_IMPORT),
            Level::Error
        );
        assert!(parse(&["--allow", "TYPE_MISMATCH", "file.bn"]).is_err());
        assert!(parse(&["--warn", "UNKNOWN", "file.bn"]).is_err());
    }

    #[test]
    fn unknown_flags_are_rejected_unless_an_extension_accepts_them() {
        struct Target(Option<String>);
        impl super::OptionExtension for Target {
            fn accept(
                &mut self,
                argument: &str,
                rest: &mut dyn Iterator<Item = String>,
            ) -> Result<bool, String> {
                if argument != "--target" {
                    return Ok(false);
                }
                self.0 = Some(rest.next().ok_or("--target expects a value")?);
                Ok(true)
            }
        }
        assert!(parse(&["--target", "wasm32", "file.bn"]).is_err());
        let mut target = Target(None);
        let arguments = ["--target", "wasm32", "file.bn"].map(String::from);
        let options = parse_options(arguments.into_iter(), &mut target).expect("extension");
        assert_eq!(options.path, "file.bn");
        assert_eq!(target.0.as_deref(), Some("wasm32"));
    }

    #[test]
    fn cli_settings_override_values_loaded_from_config() {
        let path = std::env::temp_dir().join(format!(
            "bn-config-{}-{}-precedence.toml",
            std::process::id(),
            "warning-policy"
        ));
        std::fs::write(
            &path,
            "[warnings]\ndefault = \"error\"\n[warnings.levels]\nUNUSED_BINDING = \"allow\"\n[logging]\nlevel = \"debug\"\nfile = \"config.log\"\n",
        )
        .expect("write config fixture");
        let options = parse(&[
            "--config",
            path.to_str().expect("UTF-8 config path"),
            "--warn",
            "UNUSED_BINDING",
            "--log-level",
            "error",
            "--log-file",
            "cli.log",
            "source.bn",
        ])
        .expect("CLI overrides");
        assert_eq!(
            options.warning_policy.level(DiagId::UNUSED_BINDING),
            Level::Warn
        );
        assert_eq!(format!("{:?}", options.log_level), "Error");
        assert_eq!(options.log_file.as_deref(), Some("cli.log"));
        std::fs::remove_file(path).expect("remove config fixture");
    }

    #[test]
    fn cli_log_file_reenables_logging_and_no_log_can_disable_it_afterward() {
        let path = std::env::temp_dir().join(format!(
            "bn-config-{}-{}-logging-precedence.toml",
            std::process::id(),
            "warning-policy"
        ));
        std::fs::write(&path, "[logging]\nenabled = false\nfile = \"config.log\"\n")
            .expect("write config fixture");
        let config = path.to_str().expect("UTF-8 config path");

        let enabled = parse(&["--config", config, "--log-file", "cli.log", "source.bn"])
            .expect("CLI log file override");
        assert!(!enabled.no_log);
        assert_eq!(enabled.log_file.as_deref(), Some("cli.log"));

        let disabled = parse(&[
            "--config",
            config,
            "--log-file",
            "cli.log",
            "--no-log",
            "source.bn",
        ])
        .expect("CLI no-log override");
        assert!(disabled.no_log);

        std::fs::remove_file(path).expect("remove config fixture");
    }
}
