#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn load_frontend(
    source: &SourceFile,
    _tokens: &[Token],
    options: &Options,
) -> Result<Frontend, ExitCode> {
    log(options.verbosity, 1, "loading module graph");
    let mut session = bn::frontend_session::FrontendSession::default();
    let graph = load_with_session(&options.path, &mut session).map_err(|error| {
        eprintln!(
            "{}",
            render_diagnostic(&error.diagnostic, &error.source, &options.warning_policy)
        );
        language_error()
    })?;
    log(
        options.verbosity,
        1,
        "syntax analysis (module graph root reused)",
    );
    log(options.verbosity, 1, "semantic analysis");
    let analysis = match analyze_modules_with_warnings(&graph) {
        Ok(analysis) => analysis,
        Err(error) => {
            let rendered = graph.modules.get(module_index(error.module.0)).map_or_else(
                || render_diagnostic(&error.diagnostic, source, &options.warning_policy),
                |module| {
                    render_diagnostic(&error.diagnostic, &module.source, &options.warning_policy)
                },
            );
            eprintln!("{rendered}");
            return Err(language_error());
        }
    };
    let models = analysis.models;
    let warnings = analysis.warnings;
    log(options.verbosity, 1, "lowering and validating IR");
    let validated = bn::ir::lower_graph_validated(&graph, &models).map_err(|diagnostic| {
        eprintln!(
            "{}",
            render_diagnostic(&diagnostic, source, &options.warning_policy)
        );
        language_error()
    })?;
    log(
        options.verbosity,
        1,
        format!(
            "parser completed: {} top-level items",
            root_program(&graph).items.len()
        ),
    );
    Ok(Frontend {
        graph,
        models,
        warnings,
        validated,
    })
}

#[allow(clippy::too_many_lines)] // CLI options remain auditable in one parser.
pub(crate) fn parse_options(arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let arguments = arguments.collect::<Vec<_>>();
    let config_path = config_path_from_arguments(&arguments)?;
    let (mut warning_policy, configured_log_level, configured_log_file, configured_no_log) =
        configured_settings(config_path.as_deref())?;
    let mut arguments = arguments.into_iter();
    let mut path = None;
    let mut verbosity = 0u8;
    let mut emit = None;
    let mut output = None;
    let mut trace = false;
    let mut color = Color::Auto;
    let mut target = Target::Native;
    let mut filesystem = true;
    let mut jupyter_stdin = false;
    let mut program_arguments = Vec::new();
    let mut optimization = Optimization::Level(2);
    let mut log_level = configured_log_level;
    let mut log_file = configured_log_file;
    let mut log_file_from_cli = false;
    let mut no_log = configured_no_log;
    while let Some(argument) = arguments.next() {
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
            "--jupyter-stdin" => jupyter_stdin = true,
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
                let id = bn::diagnostic::DiagId::from_code(&code)
                    .ok_or_else(|| format!("unknown diagnostic code '{code}'"))?;
                let level = match option {
                    "--allow" => bn::diagnostic::Level::Allow,
                    "--deny" => bn::diagnostic::Level::Error,
                    "--warn" => bn::diagnostic::Level::Warn,
                    _ => unreachable!("matched warning option"),
                };
                warning_policy.set_cli(id, level)?;
            }
            "--opt" => {
                optimization = match arguments.next().as_deref() {
                    Some("none") => Optimization::None,
                    Some("1") => Optimization::Level(1),
                    Some("2") => Optimization::Level(2),
                    Some("3") => Optimization::Level(3),
                    Some("s") => Optimization::Size,
                    _ => return Err("--opt expects none, 1, 2, 3, or s".into()),
                };
            }
            "--target" => {
                target = match arguments.next().as_deref() {
                    Some("native") => Target::Native,
                    Some("wasm32") => Target::Wasm32,
                    _ => return Err("--target expects native or wasm32".into()),
                };
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
    path.map(|path| Options {
        path,
        verbosity,
        emit,
        output,
        trace,
        color,
        target,
        filesystem,
        jupyter_stdin,
        program_arguments,
        optimization,
        warning_policy,
        log_level,
        log_file,
        no_log,
    })
    .ok_or_else(|| "missing source file".into())
}

fn config_path_from_arguments(arguments: &[String]) -> Result<Option<PathBuf>, String> {
    config_path_from_context(
        arguments,
        env::current_dir().ok().as_deref(),
        env::current_exe().ok().as_deref(),
    )
}

fn config_path_from_context(
    arguments: &[String],
    working_directory: Option<&Path>,
    executable: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let mut config_path = None;
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == "--" {
            break;
        }
        if arguments[index] == "--config" {
            let path = arguments
                .get(index + 1)
                .filter(|value| !value.starts_with('-'))
                .ok_or_else(|| "--config expects a file path".to_string())?;
            if config_path.replace(PathBuf::from(path)).is_some() {
                return Err("--config was specified more than once".into());
            }
            index += 1;
        }
        index += 1;
    }
    Ok(config_path.or_else(|| {
        let working_directory_config = working_directory?.join("config.toml");
        if working_directory_config.exists() {
            return Some(working_directory_config);
        }
        let executable_config = executable?.parent()?.join("config.toml");
        executable_config.exists().then_some(executable_config)
    }))
}

fn configured_settings(
    path: Option<&Path>,
) -> Result<(WarningPolicy, LogLevel, Option<String>, bool), String> {
    let Some(path) = path else {
        return Ok((WarningPolicy::default(), LogLevel::Info, None, false));
    };
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let warning_policy = WarningPolicy::from_config(&text)?;
    let mut section = "";
    let mut log_level = LogLevel::Info;
    let mut log_file = None;
    let mut no_log = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section != "logging" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "level" => log_level = LogLevel::parse(value)?,
            "file" => log_file = Some(value.to_string()),
            "enabled" => {
                no_log = match value {
                    "true" => false,
                    "false" => true,
                    _ => {
                        return Err(format!(
                            "logging.enabled expects true or false (got {value})"
                        ));
                    }
                };
            }
            _ => {}
        }
    }
    Ok((warning_policy, log_level, log_file, no_log))
}

#[cfg(test)]
mod tests {
    use super::{config_path_from_context, configured_settings, parse_options};
    use crate::{DiagId, Level, Optimization, Target};

    #[test]
    fn optimization_option_has_explicit_levels_and_default() {
        assert_eq!(
            parse_options(["file.bn".into()].into_iter())
                .expect("default")
                .optimization,
            Optimization::Level(2)
        );
        assert_eq!(
            parse_options(["--opt".into(), "none".into(), "file.bn".into()].into_iter())
                .expect("none")
                .optimization,
            Optimization::None
        );
        assert_eq!(
            parse_options(["--opt".into(), "s".into(), "file.bn".into()].into_iter())
                .expect("size")
                .optimization,
            Optimization::Size
        );
        assert_eq!(
            parse_options(["--target".into(), "wasm32".into(), "file.bn".into()].into_iter())
                .expect("target")
                .target,
            Target::Wasm32
        );
        assert!(parse_options(["--opt".into(), "4".into(), "file.bn".into()].into_iter()).is_err());
    }

    #[test]
    fn warning_flags_use_typed_codes_and_cli_precedence() {
        let options = parse_options(
            [
                "--warnings".into(),
                "errors".into(),
                "--warn".into(),
                "UNUSED_BINDING".into(),
                "file.bn".into(),
            ]
            .into_iter(),
        )
        .expect("warning flags");
        assert_eq!(
            options.warning_policy.level(DiagId::UnusedBinding),
            Level::Warn
        );
        assert_eq!(
            options.warning_policy.level(DiagId::UnusedImport),
            Level::Error
        );
        assert!(
            parse_options(["--allow".into(), "TYPE_MISMATCH".into(), "file.bn".into()].into_iter())
                .is_err()
        );
        assert!(
            parse_options(["--warn".into(), "UNKNOWN".into(), "file.bn".into()].into_iter())
                .is_err()
        );
    }

    #[test]
    fn explicit_config_loads_warning_and_logging_settings() {
        let path = std::env::temp_dir().join(format!(
            "bn-config-{}-{}.toml",
            std::process::id(),
            "warning-policy"
        ));
        std::fs::write(
            &path,
            "[warnings]\ndefault = \"error\"\n[logging]\nlevel = \"debug\"\nfile = \"build.log\"\nenabled = true\n",
        )
        .expect("write config fixture");
        let (policy, level, file, no_log) = configured_settings(Some(&path)).expect("config");
        assert_eq!(policy.level(DiagId::UnusedImport), Level::Error);
        assert_eq!(format!("{level:?}"), "Debug");
        assert_eq!(file.as_deref(), Some("build.log"));
        assert!(!no_log);
        std::fs::remove_file(path).expect("remove config fixture");
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
        let options = parse_options(
            [
                "--config".into(),
                path.to_str().expect("UTF-8 config path").into(),
                "--warn".into(),
                "UNUSED_BINDING".into(),
                "--log-level".into(),
                "error".into(),
                "--log-file".into(),
                "cli.log".into(),
                "source.bn".into(),
            ]
            .into_iter(),
        )
        .expect("CLI overrides");
        assert_eq!(
            options.warning_policy.level(DiagId::UnusedBinding),
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

        let enabled = parse_options(
            [
                "--config".into(),
                path.to_str().expect("UTF-8 config path").into(),
                "--log-file".into(),
                "cli.log".into(),
                "source.bn".into(),
            ]
            .into_iter(),
        )
        .expect("CLI log file override");
        assert!(!enabled.no_log);
        assert_eq!(enabled.log_file.as_deref(), Some("cli.log"));

        let disabled = parse_options(
            [
                "--config".into(),
                path.to_str().expect("UTF-8 config path").into(),
                "--log-file".into(),
                "cli.log".into(),
                "--no-log".into(),
                "source.bn".into(),
            ]
            .into_iter(),
        )
        .expect("CLI no-log override");
        assert!(disabled.no_log);

        std::fs::remove_file(path).expect("remove config fixture");
    }

    #[test]
    fn config_discovery_has_explicit_cli_working_directory_and_executable_precedence() {
        let directory = std::env::temp_dir().join(format!(
            "bn-config-{}-{}-discovery",
            std::process::id(),
            "warning-policy"
        ));
        let working_directory = directory.join("working");
        let executable_directory = directory.join("executable");
        std::fs::create_dir_all(&working_directory).expect("working config directory");
        std::fs::create_dir_all(&executable_directory).expect("executable config directory");
        let working_config = working_directory.join("config.toml");
        let executable = executable_directory.join("bn");
        let executable_config = executable_directory.join("config.toml");
        let explicit_config = directory.join("explicit.toml");
        for path in [&working_config, &executable_config, &explicit_config] {
            std::fs::write(path, "[logging]\nenabled = true\n").expect("write config fixture");
        }

        assert_eq!(
            config_path_from_context(&[], Some(&working_directory), Some(&executable)),
            Ok(Some(working_config.clone()))
        );
        assert_eq!(
            config_path_from_context(&[], Some(&directory), Some(&executable)),
            Ok(Some(executable_config))
        );
        assert_eq!(
            config_path_from_context(
                &["--config".into(), explicit_config.display().to_string()],
                Some(&working_directory),
                Some(&executable)
            ),
            Ok(Some(explicit_config.clone()))
        );

        std::fs::remove_dir_all(directory).expect("remove config discovery directory");
    }
}
