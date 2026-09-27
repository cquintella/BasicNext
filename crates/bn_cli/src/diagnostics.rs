//! Diagnostic presentation shared by every executable: text rendering through
//! the catalog and warning policy, the JSON v1 diagnostic object, and the
//! warning-policy pass over frontend warnings. Eval coordinate remapping is
//! applied here from `Options::eval_mapping`.

use std::process::ExitCode;

use bn_diag::{DiagId, Diagnostic, Level};
use bn_frontend::prepare::Prepared;
use bn_source::{Position, SourceFile};

use crate::{
    options::{Options, OutputFormat},
    output::{language_error, module_index},
};

#[must_use]
pub fn render_diagnostic(
    diagnostic: &Diagnostic,
    source: &SourceFile,
    options: &Options,
) -> String {
    let diagnostic = remap_eval_diagnostic(diagnostic, options);
    let display_source = options
        .eval_source_text
        .as_ref()
        .map_or_else(|| source.clone(), |text| SourceFile::new("<eval>", text));
    diagnostic.render_with_catalog_and_policy(
        &display_source,
        &options.diagnostic_catalog,
        &options.warning_policy,
    )
}

fn remap_eval_diagnostic(diagnostic: &Diagnostic, options: &Options) -> Diagnostic {
    let Some(mapping) = options.eval_mapping.as_ref() else {
        return Diagnostic {
            code: diagnostic.code,
            message: diagnostic.message.clone(),
            span: diagnostic.span,
            structured: diagnostic
                .structured
                .as_ref()
                .map(|spec| Box::new((**spec).clone())),
        };
    };
    let map_position = |mut position: Position| {
        let original_offset = position.offset;
        if mapping.inserted_length > 0 {
            if position.line > mapping.insertion_line {
                position.line = position.line.saturating_sub(1);
            }
            let inserted_end = mapping.insertion_offset + mapping.inserted_length;
            if position.offset >= inserted_end {
                position.offset = position.offset.saturating_sub(mapping.inserted_length);
            }
        }
        if let Some((prefix_offset, prefix_length)) = mapping.expression_prefix
            && original_offset >= prefix_offset + prefix_length
        {
            position.offset = position.offset.saturating_sub(prefix_length);
            position.column = position.column.saturating_sub(prefix_length);
        }
        position.source_id = Position::UNKNOWN_SOURCE;
        position.revision = Position::UNKNOWN_REVISION;
        position
    };
    let mut mapped = Diagnostic {
        code: diagnostic.code,
        message: diagnostic.message.clone(),
        span: diagnostic.span,
        structured: diagnostic
            .structured
            .as_ref()
            .map(|spec| Box::new((**spec).clone())),
    };
    mapped.span.start = map_position(mapped.span.start);
    mapped.span.end = map_position(mapped.span.end);
    if let Some(spec) = mapped.structured.as_mut() {
        for label in &mut spec.labels {
            label.span.start = map_position(label.span.start);
            label.span.end = map_position(label.span.end);
        }
    }
    mapped
}

/// One diagnostic object of the JSON v1 envelope.
#[must_use]
pub fn diagnostic_json(
    diagnostic: &Diagnostic,
    source: &SourceFile,
    options: &Options,
    phase: &str,
) -> serde_json::Value {
    let mapping = options.eval_mapping.as_ref();
    let identity_source = options
        .eval_source_text
        .as_ref()
        .map(|text| SourceFile::new("<eval>", text));
    let rendered = diagnostic
        .spec()
        .and_then(|spec| options.diagnostic_catalog.render(&spec).ok());
    let labels = rendered
        .as_ref()
        .map(|value| {
            value
                .labels
                .iter()
                .map(|label| {
                    let mut start_line = label.span.start.line;
                    let mut end_line = label.span.end.line;
                    let mut start_offset = label.span.start.offset;
                    let mut end_offset = label.span.end.offset;
                    let mut start_column = label.span.start.column;
                    let mut end_column = label.span.end.column;
                    let original_start_offset = label.span.start.offset;
                    let original_end_offset = label.span.end.offset;
                    if let Some(mapping) = mapping {
                        if mapping.inserted_length > 0 && start_line > mapping.insertion_line {
                            start_line = start_line.saturating_sub(1);
                        }
                        if mapping.inserted_length > 0 && end_line > mapping.insertion_line {
                            end_line = end_line.saturating_sub(1);
                        }
                        let inserted_end = mapping.insertion_offset + mapping.inserted_length;
                        if start_offset >= inserted_end {
                            start_offset = start_offset.saturating_sub(mapping.inserted_length);
                        }
                        if end_offset >= inserted_end {
                            end_offset = end_offset.saturating_sub(mapping.inserted_length);
                        }
                        if let Some((prefix_offset, prefix_length)) = mapping.expression_prefix {
                            if original_start_offset >= prefix_offset + prefix_length {
                                start_offset = start_offset.saturating_sub(prefix_length);
                            }
                            if original_end_offset >= prefix_offset + prefix_length {
                                end_offset = end_offset.saturating_sub(prefix_length);
                            }
                            if original_start_offset >= prefix_offset + prefix_length {
                                start_column = start_column.saturating_sub(prefix_length);
                            }
                            if original_end_offset >= prefix_offset + prefix_length {
                                end_column = end_column.saturating_sub(prefix_length);
                            }
                        }
                    }
                    serde_json::json!({
                        "source_id": identity_source.as_ref().map_or(label.span.start.source_id.0, |value| value.source_id.0),
                        "revision": identity_source.as_ref().map_or(label.span.start.revision.0, |value| value.revision.0),
                        "source_name": mapping.map_or_else(|| source.name.clone(), |mapping| mapping.source_name.clone()),
                        "style": format!("{:?}", label.style).to_lowercase(),
                        "text": label.text.clone(),
                        "start": {"offset": start_offset, "line": start_line, "column": start_column},
                        "end": {"offset": end_offset, "line": end_line, "column": end_column},
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let severity = DiagId::from_code(diagnostic.code)
        .filter(|id| id.warnings_allowed())
        .map(|id| match options.warning_policy.level(id) {
            Level::Error => "error",
            Level::Allow | Level::Warn => "warning",
        })
        .map(str::to_string)
        .or_else(|| {
            rendered
                .as_ref()
                .map(|value| format!("{:?}", value.severity).to_lowercase())
        })
        .unwrap_or_else(|| "error".into());
    serde_json::json!({
        "code": diagnostic.code,
        "severity": severity,
        "phase": phase,
        "title": rendered.as_ref().map_or_else(|| diagnostic.code.to_string(), |value| value.title.clone()),
        "message": rendered.as_ref().map_or_else(|| diagnostic.message.to_string(), |value| value.message.clone()),
        "labels": labels,
        "causes": rendered.as_ref().map_or_else(Vec::new, |value| value.causes.clone()),
        "help": rendered.as_ref().and_then(|value| value.help.clone()),
    })
}

/// Prints one frontend error in the selected format and classifies it as a
/// language error.
#[must_use]
pub fn emit_frontend_error(
    diagnostic: &Diagnostic,
    source: &SourceFile,
    options: &Options,
    phase: &str,
) -> ExitCode {
    if options.output_format == OutputFormat::Json {
        let envelope = serde_json::json!({
            "schema_version": 1,
            "ok": false,
            "exit_code": 1,
            "stdout": "",
            "stderr": "",
            "diagnostics": [diagnostic_json(diagnostic, source, options, phase)],
        });
        println!("{envelope}");
    } else {
        eprintln!("{}", render_diagnostic(diagnostic, source, options));
    }
    language_error()
}

/// Applies the warning policy to frontend warnings, printing them in text
/// mode. Returns `true` when a warning is promoted to an error.
#[must_use]
pub fn emit_frontend_warnings(frontend: &Prepared, source: &SourceFile, options: &Options) -> bool {
    let mut fatal = false;
    for warning in &frontend.warnings {
        if options.output_format == OutputFormat::Json {
            // JSON aggregation is emitted by the eval boundary; never leak
            // human-readable warning text onto process stderr.
            let Some(id) = DiagId::from_code(warning.diagnostic.code) else {
                fatal = true;
                continue;
            };
            fatal |= options.warning_policy.level(id) == Level::Error;
            continue;
        }
        let Some(id) = DiagId::from_code(warning.diagnostic.code) else {
            eprintln!(
                "{}",
                render_diagnostic(&warning.diagnostic, source, options)
            );
            fatal = true;
            continue;
        };
        let level = options.warning_policy.level(id);
        let Some(module) = frontend.graph.modules.get(module_index(warning.module.0)) else {
            eprintln!("error: warning refers to a missing module");
            fatal = true;
            continue;
        };
        let rendered = render_diagnostic(&warning.diagnostic, &module.source, options);
        if !rendered.is_empty() {
            eprintln!("{rendered}");
        }
        fatal |= level == Level::Error;
    }
    fatal
}
