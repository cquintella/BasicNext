//! `config.toml` discovery and the settings it contributes to the common
//! options. Precedence: `--config <file>`, then `./config.toml`, then
//! `config.toml` beside the executable; CLI flags override every value.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use bn_diag::WarningPolicy;

use crate::process_log::LogLevel;

pub(crate) fn config_path_from_arguments(arguments: &[String]) -> Result<Option<PathBuf>, String> {
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

pub(crate) struct ConfiguredSettings {
    pub warning_policy: WarningPolicy,
    pub log_level: LogLevel,
    pub log_file: Option<String>,
    pub no_log: bool,
    pub diagnostics_dir: Option<PathBuf>,
    pub module_paths: Vec<PathBuf>,
}

pub(crate) fn configured_settings(path: Option<&Path>) -> Result<ConfiguredSettings, String> {
    let Some(path) = path else {
        return Ok(ConfiguredSettings {
            warning_policy: WarningPolicy::default(),
            log_level: LogLevel::Info,
            log_file: None,
            no_log: false,
            diagnostics_dir: None,
            module_paths: Vec::new(),
        });
    };
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let warning_policy = WarningPolicy::from_config(&text)?;
    let diagnostics_dir = bn_diag::diagnostic_directory_from_config(&text, path)?;
    let module_paths = parse_module_paths(&text, path)?;
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
    Ok(ConfiguredSettings {
        warning_policy,
        log_level,
        log_file,
        no_log,
        diagnostics_dir,
        module_paths,
    })
}

fn parse_module_paths(text: &str, config_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut value = None;
    let mut seen = false;
    let mut section = String::new();
    let mut collecting = false;
    for raw in text.lines() {
        let line = strip_toml_comment(raw);
        let trimmed = line.trim();
        if !collecting && trimmed.starts_with('[') {
            section = trimmed.trim_matches(['[', ']']).trim().to_string();
            continue;
        }
        if !collecting && trimmed.starts_with("module-path") {
            if !section.is_empty() {
                return Err("module-path must be declared at the top level".into());
            }
            let Some((key, rhs)) = trimmed.split_once('=') else {
                return Err("module-path expects an array of paths".into());
            };
            if key.trim() != "module-path" || seen {
                return Err("module-path must be declared once at the top level".into());
            }
            seen = true;
            collecting = true;
            value = Some(rhs.trim().to_string());
        } else if collecting {
            value.as_mut().expect("value initialized").push('\n');
            value.as_mut().expect("value initialized").push_str(&line);
        } else {
            continue;
        }
        let current = value.as_deref().unwrap_or_default();
        let mut quote = false;
        let mut escaped = false;
        let mut depth = 0_i32;
        for ch in current.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote {
                escaped = true;
                continue;
            }
            if ch == '"' {
                quote = !quote;
            }
            if !quote {
                if ch == '[' {
                    depth += 1;
                } else if ch == ']' {
                    depth -= 1;
                }
            }
        }
        if collecting && depth == 0 {
            collecting = false;
        }
    }
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let value = value.trim();
    if !(value.starts_with('[') && value.ends_with(']')) {
        return Err("module-path expects an array of paths".into());
    }
    let mut paths = Vec::new();
    let mut item = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in value[1..value.len() - 1].chars() {
        if escaped {
            item.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        if ch == '\\' && quoted {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            item.push(ch);
            continue;
        }
        if ch == ',' && !quoted {
            paths.push(parse_module_path_item(&item, config_path)?);
            item.clear();
        } else {
            item.push(ch);
        }
    }
    if !item.trim().is_empty() {
        paths.push(parse_module_path_item(&item, config_path)?);
    }
    Ok(paths)
}

fn strip_toml_comment(line: &str) -> String {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quoted {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == '#' && !quoted {
            return line[..index].to_string();
        }
    }
    line.to_string()
}

fn parse_module_path_item(item: &str, config_path: &Path) -> Result<PathBuf, String> {
    let item = item.trim();
    let Some(path) = item.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return Err("module-path entries must be quoted strings".into());
    };
    let path = PathBuf::from(path);
    Ok(if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::{config_path_from_context, configured_settings, parse_module_paths};
    use bn_diag::{DiagId, Level};

    #[test]
    fn module_path_parser_accepts_toml_quotes_comments_and_multiline_arrays() {
        let config = std::path::Path::new("/tmp/project/config.toml");
        let paths = parse_module_paths(
            "module-path = [\n  \"mods#one\", # comment\n  \"mods,two\",\n  \"escaped\\\"name\"\n]\n",
            config,
        )
        .expect("valid module-path array");
        assert_eq!(paths[0], config.parent().unwrap().join("mods#one"));
        assert_eq!(paths[1], config.parent().unwrap().join("mods,two"));
        assert_eq!(paths[2], config.parent().unwrap().join("escaped\"name"));
        let with_section = parse_module_paths(
            "module-path = [\"vendor\\\"name\"]\n[logging]\nlevel = \"debug\"\n",
            config,
        )
        .expect("escaped quote before another section");
        assert_eq!(
            with_section[0],
            config.parent().unwrap().join("vendor\"name")
        );
    }

    #[test]
    fn module_path_parser_rejects_duplicates_sections_and_non_strings() {
        let config = std::path::Path::new("/tmp/project/config.toml");
        assert!(parse_module_paths("module-path=[]\nmodule-path=[]", config).is_err());
        assert!(parse_module_paths("[logging]\nmodule-path=[]", config).is_err());
        assert!(parse_module_paths("module-path=[true]", config).is_err());
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
            "[warnings]\ndefault = \"error\"\n[logging]\nlevel = \"debug\"\nfile = \"build.log\"\nenabled = true\n[diagnostics]\ndir = \"messages/en-US\"\n",
        )
        .expect("write config fixture");
        let configured = configured_settings(Some(&path)).expect("config");
        assert_eq!(
            configured.warning_policy.level(DiagId::UNUSED_IMPORT),
            Level::Error
        );
        assert_eq!(format!("{:?}", configured.log_level), "Debug");
        assert_eq!(configured.log_file.as_deref(), Some("build.log"));
        assert!(!configured.no_log);
        assert_eq!(
            configured.diagnostics_dir,
            Some(path.parent().expect("parent").join("messages/en-US"))
        );
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
