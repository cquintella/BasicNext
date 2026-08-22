use std::{
    env, fs,
    io::{self, IsTerminal},
    process::ExitCode,
};

use bn::{
    ir::lower,
    lexer::lex,
    module_graph::load,
    parser::parse_named,
    runtime::execute,
    semantic::{analyze, analyze_modules},
    source::SourceFile,
    token::Token,
};

const VERSION: &str = "bn 0.1.0-dev";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Emit {
    Tokens,
    Ast,
    TypedAst,
    Ir,
}
#[derive(Clone, Copy)]
enum Color {
    Auto,
    Always,
    Never,
}
struct Options {
    path: String,
    verbosity: u8,
    emit: Option<Emit>,
    output: Option<String>,
    trace: bool,
    color: Color,
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };

    if matches!(command.as_str(), "-h" | "--help") {
        return help();
    }
    if matches!(command.as_str(), "-V" | "--version") {
        println!("{VERSION}");
        return ExitCode::SUCCESS;
    }
    let options = match parse_options(arguments) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}");
            return usage();
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] reading {}", options.path);
    }
    let text = match fs::read_to_string(&options.path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", options.path);
            return ExitCode::FAILURE;
        }
    };
    let source = SourceFile::new(&options.path, text);
    if options.verbosity > 0 {
        eprintln!("[bn] lexical analysis");
    }
    let tokens = match lex(&source) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(&source));
            return ExitCode::FAILURE;
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] lexer completed: {} tokens", tokens.len());
    }
    match command.as_str() {
        "lex" => emit_output(
            tokens_text(&tokens),
            options.output.as_deref(),
            options.color,
        ),
        "check" => check(&source, &tokens, &options),
        "run" => run(&source, &tokens, &options),
        _ => usage(),
    }
}

fn run(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    if options.verbosity > 0 {
        eprintln!("[bn] syntax analysis");
    }
    let program = match parse_named(tokens, &source.name) {
        Ok(program) => program,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return ExitCode::FAILURE;
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] semantic analysis");
    }
    let semantic_model = match analyze(&program) {
        Ok(model) => model,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return ExitCode::FAILURE;
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] lowering typed BN IR");
    }
    let module = match lower(&program, &semantic_model) {
        Ok(module) => module,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return ExitCode::FAILURE;
        }
    };
    if options.trace {
        eprintln!("[bn] executing IR entry point Start");
    }
    if options.verbosity > 1 {
        print_tokens(tokens);
    }
    let stdin = io::stdin();
    let stdout = io::stdout();
    match execute(&module, &mut stdin.lock(), &mut stdout.lock()) {
        Ok(code) => ExitCode::from(code),
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            ExitCode::FAILURE
        }
    }
}

fn check(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    if options.output.is_some() && options.emit.is_none() {
        eprintln!("error: -o requires --emit with bn check");
        return ExitCode::FAILURE;
    }
    if options.verbosity > 0 {
        eprintln!("[bn] loading module graph");
    }
    let graph = match load(&options.path) {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("{}", error.diagnostic.render(&error.source));
            return ExitCode::FAILURE;
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] syntax analysis");
    }
    let program = match parse_named(tokens, &source.name) {
        Ok(program) => program,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return ExitCode::FAILURE;
        }
    };
    if options.verbosity > 0 {
        eprintln!("[bn] semantic analysis");
    }
    let semantic_models = match analyze_modules(&graph) {
        Ok(models) => models,
        Err(error) => {
            let module = &graph.modules[usize::try_from(error.module.0).expect("module index")];
            eprintln!("{}", error.diagnostic.render(&module.source));
            return ExitCode::FAILURE;
        }
    };
    let semantic_model = &semantic_models[usize::try_from(graph.root.0).expect("root index")];
    if options.verbosity > 1 {
        print_tokens(tokens);
    }
    if let Some(emit) = options.emit {
        let output = match emit {
            Emit::Tokens => tokens_text(tokens),
            Emit::Ast => format!("{program:#?}\n"),
            Emit::TypedAst => format!("{program:#?}\n{semantic_model:#?}\n"),
            Emit::Ir => match lower(&program, semantic_model) {
                Ok(module) => format!("{module:#?}\n"),
                Err(diagnostic) => {
                    eprintln!("{}", diagnostic.render(source));
                    return ExitCode::FAILURE;
                }
            },
        };
        if emit_output(output, options.output.as_deref(), options.color) != ExitCode::SUCCESS {
            return ExitCode::FAILURE;
        }
    }
    if options.trace {
        eprintln!("[bn] check has no execution to trace");
    }
    if options.verbosity > 0 {
        eprintln!(
            "[bn] parser completed: {} top-level items",
            program.items.len()
        );
    }
    println!(
        "{}",
        colorize(
            &format!(
                "{}: lexical, syntax, and semantic checks passed",
                source.name
            ),
            options.color
        )
    );
    ExitCode::SUCCESS
}

fn parse_options(mut arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut path = None;
    let mut verbosity = 0;
    let mut emit = None;
    let mut output = None;
    let mut trace = false;
    let mut color = Color::Auto;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Err("help is available as bn --help".into()),
            "-v" if verbosity == 0 => verbosity = 1,
            "-vv" if verbosity == 0 => verbosity = 2,
            "--trace" => trace = true,
            "--emit" => {
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
    })
    .ok_or_else(|| "missing source file".into())
}

fn print_tokens(tokens: &[Token]) {
    print!("{}", tokens_text(tokens));
}
fn tokens_text(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| {
            format!(
                "{}:{}:{} {:?}",
                token.span.start.line, token.span.start.column, token.span.end.column, token.kind
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
fn emit_output(output: String, path: Option<&str>, color: Color) -> ExitCode {
    if let Some(path) = path {
        match fs::write(path, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: cannot write {path}: {error}");
                ExitCode::FAILURE
            }
        }
    } else {
        print!("{}", colorize_debug(&output, color));
        ExitCode::SUCCESS
    }
}
fn colorize_debug(text: &str, color: Color) -> String {
    if !color_enabled(color) {
        return text.into();
    }

    let mut output = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut identifier_strings = false;
    while index < bytes.len() {
        if bytes[index] == b'\"' {
            let end = text[index + 1..]
                .find('\"')
                .map_or(text.len(), |offset| index + offset + 2);
            output.push_str(if identifier_strings {
                "\x1b[34m"
            } else {
                "\x1b[32m"
            });
            output.push_str(&text[index..end]);
            output.push_str("\x1b[0m");
            index = end;
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let word = &text[start..index];
            let is_field = bytes.get(index) == Some(&b':');
            if is_field {
                identifier_strings = is_identifier_field(word);
            }
            let ansi = if is_field {
                "\x1b[33m"
            } else if word.starts_with(|character: char| character.is_ascii_uppercase()) {
                "\x1b[36m"
            } else {
                ""
            };
            output.push_str(ansi);
            output.push_str(word);
            if !ansi.is_empty() {
                output.push_str("\x1b[0m");
            }
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    output
}

fn is_identifier_field(field: &str) -> bool {
    matches!(
        field,
        "alias" | "interfaces" | "name" | "parts" | "path" | "type_name"
    )
}

fn color_enabled(color: Color) -> bool {
    matches!(color, Color::Always)
        || (matches!(color, Color::Auto) && std::io::stdout().is_terminal())
}

fn colorize(text: &str, color: Color) -> String {
    if color_enabled(color) {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.into()
    }
}
fn help() -> ExitCode {
    println!(
        "{VERSION}\nusage: bn <check|lex|run> [options] <file.bn>\n\noptions:\n  -v, -vv                    show stages; -vv also prints tokens\n  --emit tokens|ast|typed-ast|ir print a frontend artifact\n  -o, --output <file>        write lex output or an emitted artifact to <file>\n  --trace                    report the bn run execution entry point\n  --color auto|always|never  control ANSI output color\n  -V, --version              print version\n  -h, --help                 print this help"
    );
    ExitCode::SUCCESS
}
fn usage() -> ExitCode {
    eprintln!("usage: bn <check|lex|run> [options] <file.bn>\ntry: bn --help");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::{Color, colorize_debug};

    #[test]
    fn debug_output_uses_distinct_colors() {
        let output = colorize_debug("Name { name: \"hello\", value: \"text\" }", Color::Always);
        assert!(output.contains("\x1b[36mName\x1b[0m"));
        assert!(output.contains("\x1b[33mname\x1b[0m"));
        assert!(output.contains("\x1b[34m\"hello\"\x1b[0m"));
        assert!(output.contains("\x1b[32m\"text\"\x1b[0m"));
    }

    #[test]
    fn never_color_preserves_debug_output() {
        assert_eq!(colorize_debug("Program", Color::Never), "Program");
    }
}
