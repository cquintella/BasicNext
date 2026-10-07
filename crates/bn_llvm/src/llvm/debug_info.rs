// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-level debug information for `bnc -g`: the request type, the span
//! markers `emit_function` writes, and the pass that turns markers into
//! `!dbg` attachments plus the typed metadata table.
//!
//! Markers are comment lines, so later insertions by byte offset (deferred
//! Phi nodes) stay valid; the pass runs once on the finished module text.
// ponytail: line-based attachment over the textual backend; attach `!dbg`
// through `InstructionEntry::with_dbg` once the backend is fully typed.

use std::{collections::BTreeMap, fmt::Write as _, path::PathBuf};

use bn_source::{SourceId, Span};

use crate::ir::{DebugFormat, DebugMetadata, MdRef};

const FUNCTION_MARKER: &str = ";bn.dbg.fn ";
const LOCATION_MARKER: &str = ";bn.dbg ";

/// What `bnc -g` asks the backend to describe.
#[derive(Clone, Debug)]
pub struct DebugInfo {
    /// Absolute path of every loaded source, by the id its spans carry.
    pub sources: BTreeMap<SourceId, PathBuf>,
    /// The entry file; it names the compile unit.
    pub entry: SourceId,
    pub optimized: bool,
    pub format: DebugFormat,
}

impl DebugInfo {
    /// `CodeView` for Windows executables (the MSVC linker writes a PDB),
    /// DWARF 4 everywhere else, including Wasm.
    #[must_use]
    pub const fn format_for(wasm32: bool) -> DebugFormat {
        if cfg!(windows) && !wasm32 {
            DebugFormat::CodeView
        } else {
            DebugFormat::Dwarf(4)
        }
    }
}

/// Writes the marker that precedes a function's `define` line.
pub(crate) fn mark_function(text: &mut String, span: Span, name: &str) {
    let _ = writeln!(
        text,
        "{FUNCTION_MARKER}{} {} {name}",
        span.start.source_id.0, span.start.line
    );
}

/// Writes the marker for the instructions lowered from `span`. Spans from
/// another file or without a line keep the previous location.
pub(crate) fn mark_location(text: &mut String, span: Span, function: Span) {
    if span.start.line == 0 || span.start.source_id != function.start.source_id {
        return;
    }
    let _ = writeln!(
        text,
        "{LOCATION_MARKER}{} {}",
        span.start.line, span.start.column
    );
}

fn split_path(path: &std::path::Path) -> (String, String) {
    let filename = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let directory = path
        .parent()
        .map_or_else(String::new, |dir| dir.to_string_lossy().into_owned());
    (filename, directory)
}

fn to_u32(value: &str) -> u32 {
    value.parse().unwrap_or(0)
}

/// Every non-empty, non-comment, non-label line inside a function body is
/// an instruction; the backend emits no multi-line instructions.
fn is_instruction(line: &str) -> bool {
    line.starts_with("  ") && !line.trim_start().starts_with(';') && !line.contains("!dbg")
}

/// Replaces the markers with `!dbg` attachments and appends the metadata.
pub(crate) fn attach_debug_info(text: &str, info: &DebugInfo) -> String {
    let entry = info.sources.get(&info.entry).cloned().unwrap_or_default();
    let (filename, directory) = split_path(&entry);
    let mut table = DebugMetadata::new(
        &filename,
        &directory,
        concat!("Basic Next ", env!("CARGO_PKG_VERSION")),
        info.optimized,
        info.format,
    );
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut pending: Option<(SourceId, u32, String)> = None;
    // The current function's subprogram and the location its next
    // instructions carry.
    let mut scope: Option<(MdRef, MdRef)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(FUNCTION_MARKER) {
            let mut parts = rest.splitn(3, ' ');
            let source = SourceId(parts.next().map_or(0, |id| id.parse().unwrap_or(0)));
            let start = parts.next().map_or(0, to_u32);
            let name = parts.next().unwrap_or_default().to_owned();
            pending = Some((source, start, name));
            continue;
        }
        if let Some(rest) = line.strip_prefix(LOCATION_MARKER) {
            if let Some((subprogram, _)) = scope {
                let mut parts = rest.split(' ');
                let row = parts.next().map_or(0, to_u32);
                let column = parts.next().map_or(0, to_u32);
                scope = Some((subprogram, table.location(row, column, subprogram)));
            }
            continue;
        }
        if line.starts_with("define ")
            && let Some((source, start, name)) = pending.take()
            && let Some(head) = line.strip_suffix(" {")
        {
            let path = info.sources.get(&source).unwrap_or(&entry);
            let (filename, directory) = split_path(path);
            let file = table.file(&filename, &directory);
            let subprogram = table.subprogram(&name, file, start);
            // Prologue code (allocas, policy setup) carries the header line.
            scope = Some((subprogram, table.location(start, 1, subprogram)));
            let _ = writeln!(out, "{head} !dbg {subprogram} {{");
            continue;
        }
        if line == "}" {
            scope = None;
        } else if let Some((_, location)) = scope
            && is_instruction(line)
        {
            let _ = writeln!(out, "{line}, !dbg {location}");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    let _ = write!(out, "{table}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bn_source::{Position, Revision};

    fn span(source: u64, line: usize, column: usize) -> Span {
        let position = Position {
            source_id: SourceId(source),
            revision: Revision(0),
            offset: 0,
            line,
            column,
        };
        Span {
            start: position,
            end: position,
        }
    }

    fn info() -> DebugInfo {
        DebugInfo {
            sources: BTreeMap::from([(SourceId(7), PathBuf::from("/src/h.bn"))]),
            entry: SourceId(7),
            optimized: false,
            format: DebugFormat::Dwarf(4),
        }
    }

    #[test]
    fn markers_become_subprogram_and_location_attachments() {
        let function = span(7, 1, 1);
        let mut text = String::from("@g = global i32 0\n");
        mark_function(&mut text, function, "Twice");
        text.push_str("define i32 @bn_Twice(i32 %p0) {\nb0:\n  %s0 = alloca i64\n");
        mark_location(&mut text, span(7, 2, 5), function);
        text.push_str("  ; note\n  %v1 = mul i64 %v0, 2\n  ret i32 0\n}\ndeclare void @f()\n");
        let out = attach_debug_info(&text, &info());
        assert!(!out.contains(";bn.dbg"), "markers are removed: {out}");
        assert!(out.contains("define i32 @bn_Twice(i32 %p0) !dbg !3 {\nb0:\n"));
        assert!(out.contains("  %s0 = alloca i64, !dbg !4\n"), "{out}");
        assert!(
            out.contains("  ; note\n  %v1 = mul i64 %v0, 2, !dbg !5\n  ret i32 0, !dbg !5\n}\n")
        );
        assert!(out.contains("@g = global i32 0\n"));
        assert!(out.contains("declare void @f()\n"));
        assert!(
            out.contains(
                "!3 = distinct !DISubprogram(name: \"Twice\", scope: !0, file: !0, line: 1"
            )
        );
        assert!(out.contains("!5 = !DILocation(line: 2, column: 5, scope: !3)"));
    }

    #[test]
    fn foreign_or_lineless_spans_keep_the_previous_location() {
        let function = span(7, 3, 1);
        let mut text = String::new();
        mark_location(&mut text, span(8, 9, 9), function);
        mark_location(&mut text, span(7, 0, 0), function);
        assert!(text.is_empty());
    }

    #[test]
    fn functions_without_a_marker_get_no_debug_attachments() {
        let text = "define internal void @helper() {\nb0:\n  ret void\n}\n";
        let out = attach_debug_info(text, &info());
        assert!(out.starts_with(text), "{out}");
    }
}
