#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn load_frontend(
    source: &SourceFile,
    _tokens: &[Token],
    options: &Options,
) -> Result<Frontend, ExitCode> {
    log(options.verbosity, 1, "loading module graph");
    let graph = load(&options.path).map_err(|error| {
        eprintln!("{}", error.diagnostic.render(&error.source));
        language_error()
    })?;
    log(
        options.verbosity,
        1,
        "syntax analysis (module graph root reused)",
    );
    log(options.verbosity, 1, "semantic analysis");
    let models = match analyze_modules(&graph) {
        Ok(models) => models,
        Err(error) => {
            let rendered = graph.modules.get(module_index(error.module.0)).map_or_else(
                || error.diagnostic.render(source),
                |module| error.diagnostic.render(&module.source),
            );
            eprintln!("{rendered}");
            return Err(language_error());
        }
    };
    log(
        options.verbosity,
        1,
        format!(
            "parser completed: {} top-level items",
            root_program(&graph).items.len()
        ),
    );
    Ok(Frontend { graph, models })
}

pub(crate) fn parse_options(
    mut arguments: impl Iterator<Item = String>,
) -> Result<Options, String> {
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
    })
    .ok_or_else(|| "missing source file".into())
}

#[cfg(test)]
mod tests {
    use super::parse_options;
    use crate::{Optimization, Target};

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
}
