//! The `build` command: shared frontend preparation, target support check
//! (`validate_for`), policy carrier, LLVM emission and artifact linking,
//! with every stage mirrored into the companion process log.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use bn_cli::{
    diagnostics::emit_frontend_warnings,
    frontend::load_frontend,
    options::Options,
    output::{language_error, tool_error},
    process_log::{LogLevel, ProcessLog},
};
use bn_llvm::{
    CompiledPolicy, Target as LlvmTarget, lower_validated_module_for_target_with_policy,
    validate_for,
};
use bn_source::SourceFile;

use crate::{
    artifact::emit_build_output,
    options::{BuildOptions, Target},
};

fn process_log_path(options: &Options) -> Option<PathBuf> {
    if options.no_log {
        return None;
    }
    options.log_file.as_ref().map_or_else(
        || {
            options
                .output
                .as_ref()
                .map(|output| Path::new(output).with_extension("log"))
        },
        |path| Some(PathBuf::from(path)),
    )
}

/// Compiles `options.path` and writes the companion process log.
#[must_use]
pub fn build(source: &SourceFile, options: &Options, build_options: BuildOptions) -> ExitCode {
    let mut process_log = ProcessLog::new(options.log_level);
    process_log.event(
        LogLevel::Info,
        "pipeline",
        "start",
        format!(
            "target={:?} output={:?}",
            build_options.target, options.output
        ),
    );
    let result = build_inner(source, options, build_options, &mut process_log);
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Warn
        } else {
            LogLevel::Error
        },
        "diagnostic",
        "summary",
        format!(
            "errors={} warnings={} exit={result:?}",
            usize::from(result != ExitCode::SUCCESS),
            process_log.warning_count()
        ),
    );
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        "pipeline",
        "end",
        format!("exit={result:?}"),
    );
    if process_log.finish(process_log_path(options).as_deref()) {
        return tool_error();
    }
    result
}

#[allow(clippy::too_many_lines)] // Build stage events stay adjacent to their real transitions.
fn build_inner(
    source: &SourceFile,
    options: &Options,
    build_options: BuildOptions,
    process_log: &mut ProcessLog,
) -> ExitCode {
    process_log.event(
        LogLevel::Info,
        "frontend",
        "start",
        "load and analyze modules",
    );
    let frontend = match load_frontend(source, options) {
        Ok(frontend) => frontend,
        Err(code) => {
            process_log.event(
                LogLevel::Error,
                "frontend",
                "fail",
                format!("exit={code:?}"),
            );
            return code;
        }
    };
    process_log.event(
        LogLevel::Info,
        "frontend",
        "success",
        "semantic analysis complete",
    );
    process_log.mirror_frontend_diagnostics(&frontend);
    let roots = frontend
        .graph
        .roots
        .iter()
        .map(|root| format!("{} ({})", root.path.display(), root.provenance.label()))
        .collect::<Vec<_>>();
    process_log.event(
        LogLevel::Debug,
        "config",
        "snapshot",
        format!(
            "target={:?} opt={:?} log_level={:?} no_log={} bn_home={} module_roots={roots:?}",
            build_options.target,
            build_options.optimization,
            options.log_level,
            options.no_log,
            std::env::var_os("BN_HOME").is_some(),
        ),
    );
    let resolved = frontend
        .graph
        .modules
        .iter()
        .filter(|module| module.id != frontend.graph.root)
        .map(|module| {
            let origin = module.root.as_ref().map_or_else(
                || "unresolved".to_string(),
                |root| format!("{} ({})", root.path.display(), root.provenance.label()),
            );
            format!("{} <- {origin}", module.source.name)
        })
        .collect::<Vec<_>>();
    process_log.event(
        LogLevel::Debug,
        "frontend",
        "module-roots",
        format!("resolved={resolved:?}"),
    );
    process_log.event(
        LogLevel::Debug,
        "frontend",
        "modules",
        format!(
            "count={} names={:?}",
            frontend.graph.modules.len(),
            frontend
                .graph
                .modules
                .iter()
                .map(|module| module.source.name.as_str())
                .collect::<Vec<_>>()
        ),
    );
    if emit_frontend_warnings(&frontend, source, options) {
        process_log.event(
            LogLevel::Error,
            "frontend",
            "fail",
            "warning promoted to error",
        );
        return language_error();
    }
    process_log.event(LogLevel::Info, "lower", "start", "lower and validate IR");
    let module = &frontend.validated;
    process_log.event(LogLevel::Info, "lower", "success", "validated IR ready");
    let llvm_target = if build_options.target == Target::Wasm32 {
        LlvmTarget::Wasm32
    } else {
        LlvmTarget::Native
    };
    if let Err(error) = validate_for(module, llvm_target) {
        process_log.event(
            LogLevel::Error,
            "validate_for",
            "fail",
            format!(
                "code={} source={} line={} column={}",
                error.code,
                error.span.start.source_id.0,
                error.span.start.line,
                error.span.start.column
            ),
        );
        eprintln!("error[{}]: {}", error.code, error.message);
        return tool_error();
    }
    process_log.event(
        LogLevel::Info,
        "validate_for",
        "success",
        "target support accepted",
    );
    process_log.event(LogLevel::Info, "llvm_emit", "start", "emit target LLVM");
    let policy = CompiledPolicy {
        sandboxed: options.sandbox,
        read_roots: options
            .read_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        write_roots: options
            .write_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    };
    let result = match lower_validated_module_for_target_with_policy(
        module,
        build_options.target == Target::Wasm32,
        &policy,
    ) {
        Ok(llvm) => {
            process_log.event(LogLevel::Info, "llvm_emit", "success", "LLVM emitted");
            process_log.event(LogLevel::Info, "link", "start", "write artifact");
            emit_build_output(llvm, options, build_options, process_log)
        }
        Err(message) => {
            process_log.event(
                LogLevel::Error,
                "llvm_emit",
                "fail",
                format!("error={message}"),
            );
            eprintln!("error[{message}]");
            tool_error()
        }
    };
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        "link",
        if result == ExitCode::SUCCESS {
            "success"
        } else {
            "fail"
        },
        format!("exit={result:?}"),
    );
    result
}
