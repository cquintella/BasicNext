// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-level debug information for `bnc -g`: the request type, the span
//! markers `emit_function` records in the module, and the metadata table
//! whose nodes the module attaches as `!dbg` when it renders
//! (`LlvmModule::render_debug`, bucket 0.6.5 T.5.6c).

use std::{collections::BTreeMap, path::PathBuf};

use bn_source::{SourceId, Span};

use crate::ir::{DebugFormat, DebugMetadata, DebugScopes, LlvmModule, MdRef};

/// The toolchain that wrote a module: the `.ll` header comment and the
/// debug compile unit name it.
pub(crate) const PRODUCER: &str = concat!("Basic Next ", env!("CARGO_PKG_VERSION"));

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

/// Marks the function that opens next as `name`, declared at `span`.
pub(crate) fn mark_function(text: &mut LlvmModule, span: Span, name: &str) {
    let line = u32::try_from(span.start.line).unwrap_or(0);
    text.mark_function(span.start.source_id.0, line, name);
}

/// Marks the instructions lowered from `span`. Spans from another file or
/// without a line keep the previous location.
pub(crate) fn mark_location(text: &mut LlvmModule, span: Span, function: Span) {
    if span.start.line == 0 || span.start.source_id != function.start.source_id {
        return;
    }
    let line = u32::try_from(span.start.line).unwrap_or(0);
    let column = u32::try_from(span.start.column).unwrap_or(0);
    text.mark_location(line, column);
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

/// The metadata table of one module, with the sources its spans name.
struct Scopes<'a> {
    table: DebugMetadata,
    info: &'a DebugInfo,
    entry: PathBuf,
}

impl DebugScopes for Scopes<'_> {
    fn subprogram(&mut self, source: u64, line: u32, name: &str) -> MdRef {
        let path = self
            .info
            .sources
            .get(&SourceId(source))
            .unwrap_or(&self.entry);
        let (filename, directory) = split_path(path);
        let file = self.table.file(&filename, &directory);
        self.table.subprogram(name, file, line)
    }

    fn location(&mut self, line: u32, column: u32, scope: MdRef) -> MdRef {
        self.table.location(line, column, scope)
    }
}

/// The module's text with its `!dbg` attachments and the metadata after it.
pub(crate) fn attach_debug_info(text: &LlvmModule, info: &DebugInfo) -> String {
    let entry = info.sources.get(&info.entry).cloned().unwrap_or_default();
    let (filename, directory) = split_path(&entry);
    let table = DebugMetadata::new(&filename, &directory, PRODUCER, info.optimized, info.format);
    let mut scopes = Scopes { table, info, entry };
    let mut out = text.render_debug(&mut scopes);
    out.push_str(&scopes.table.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{InstSink as _, LlvmFunction, LlvmGlobal, LlvmInst, LlvmOperand, LlvmType};
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

    fn twice() -> LlvmFunction {
        let params = vec![(LlvmType::I32, "p0".to_string())];
        LlvmFunction::new_definition("bn_Twice", LlvmType::I32, params)
    }

    #[test]
    fn markers_become_subprogram_and_location_attachments() {
        let function = span(7, 1, 1);
        let mut text = LlvmModule::new();
        text.global(LlvmGlobal::variable(
            "g",
            LlvmType::I32,
            LlvmOperand::int(0),
        ));
        mark_function(&mut text, function, "Twice");
        text.begin_function(twice());
        text.label("b0");
        text.assign("s0", LlvmInst::alloca(LlvmType::I64));
        mark_location(&mut text, span(7, 2, 5), function);
        text.comment("note");
        let doubled = LlvmInst::binary(
            crate::ir::BinaryOp::Mul,
            LlvmType::I64,
            LlvmOperand::reg("v0"),
            LlvmOperand::int(2),
        );
        text.assign("v1", doubled);
        text.emit(LlvmInst::Ret {
            val: Some((LlvmType::I32, LlvmOperand::int(0))),
        });
        text.end_function();
        let out = attach_debug_info(&text, &info());
        assert!(
            out.contains("define i32 @bn_Twice(i32 %p0) !dbg !3 {\nb0:\n"),
            "{out}"
        );
        assert!(out.contains("  %s0 = alloca i64, !dbg !4\n"), "{out}");
        assert!(
            out.contains("; note\n  %v1 = mul i64 %v0, 2, !dbg !5\n  ret i32 0, !dbg !5\n}\n"),
            "{out}"
        );
        assert!(out.contains("@g = global i32 0\n"));
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
        let mut text = LlvmModule::new();
        mark_location(&mut text, span(8, 9, 9), function);
        mark_location(&mut text, span(7, 0, 0), function);
        assert!(text.render().is_empty());
    }

    #[test]
    fn functions_without_a_marker_get_no_debug_attachments() {
        let mut text = LlvmModule::new();
        text.begin_function(twice());
        text.label("b0");
        text.emit(LlvmInst::Ret { val: None });
        text.end_function();
        let out = attach_debug_info(&text, &info());
        assert!(out.starts_with(&text.render()), "{out}");
        assert!(!out[..text.render().len()].contains("!dbg"), "{out}");
    }
}
