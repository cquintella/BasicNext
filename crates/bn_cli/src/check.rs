//! The `check` command: prepare (lex, parse, semantic, lower, validate) and
//! optionally emit one frontend artifact. The IDE Problems baseline is
//! defined as this command's diagnostics.

use std::process::ExitCode;

use bn_frontend::token::Token;
use bn_source::SourceFile;

use crate::{
    diagnostics::emit_frontend_warnings,
    frontend::{Frontend, load_frontend},
    options::{Emit, Options},
    output::{colorize, emit_output, log, module_index, tokens_text, tool_error},
};

/// The `--emit` artifact of a prepared program (tokens, AST, typed AST or
/// validated BN IR), shared by `check` and the compiler's `--emit`.
///
/// # Errors
///
/// Prints the message and returns the tool-error exit code when the
/// executable module has no semantic model.
pub fn frontend_artifact(
    frontend: &Frontend,
    tokens: &[Token],
    emit: Emit,
) -> Result<String, ExitCode> {
    let Some(semantic_model) = frontend.models.get(module_index(frontend.graph.root.0)) else {
        eprintln!("error: missing semantic model for the executable module");
        return Err(tool_error());
    };
    Ok(match emit {
        Emit::Tokens => tokens_text(tokens),
        Emit::Ast => format!("{:#?}\n", frontend.root_program()),
        Emit::TypedAst => format!("{:#?}\n{semantic_model:#?}\n", frontend.root_program()),
        Emit::Ir => format!("{:#?}\n", frontend.validated.as_module()),
    })
}

#[must_use]
pub fn check(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    if options.output.is_some() && options.emit.is_none() {
        eprintln!("error: -o requires --emit with bn check");
        return tool_error();
    }
    let frontend = match load_frontend(source, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    if emit_frontend_warnings(&frontend, source, options) {
        return crate::output::language_error();
    }
    if options.verbosity > 1 {
        print!("{}", tokens_text(tokens));
    }
    if let Some(emit) = options.emit {
        let output = match frontend_artifact(&frontend, tokens, emit) {
            Ok(output) => output,
            Err(code) => return code,
        };
        if emit_output(output, options.output.as_deref()) != ExitCode::SUCCESS {
            return tool_error();
        }
    }
    if options.trace {
        log(
            options.verbosity.max(1),
            1,
            "check has no execution to trace",
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
