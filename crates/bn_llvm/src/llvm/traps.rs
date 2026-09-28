// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native runtime diagnostics (bucket 0.6.2b R6). A trap site branches to
//! its own block, which prints the diagnostic `bni` would print and then
//! joins the function's trap exit. The text is rendered at compile time by
//! the caller's renderer (the shared catalog and the source), so the program
//! embeds no catalog; facts known only at run time (an index, an exact
//! result) are marked slots that `bn_rt_trap_report` fills.
//!
//! The diagnostic travels in the constant's symbol (`@.bn_trap_<hex>`), the
//! pattern `@.bn_typename_<hex>` uses: sites need no module-wide registry,
//! and [`define_trap_globals`] renders every symbol the text references.

#![allow(clippy::wildcard_imports)]
use super::*;

use bn_diag::{DiagId, Diagnostic, DiagnosticValue, Label, LabelStyle};
use bn_source::{Position, Revision, SourceId, Span};

/// One argument of a trap diagnostic.
pub(crate) enum Fact {
    /// Known when compiling.
    Text(String),
    /// `template` with its `{}` replaced by an `i128` LLVM operand read when
    /// the trap fires (at most two runtime facts per site).
    Runtime(&'static str, String),
}

/// A slot for runtime fact `index` in rendered text; `bn_rt_trap_report`
/// replaces it with the value. Control characters never occur in rendered
/// catalog text or BN source excerpts.
const SLOT: char = '\u{1}';
const FIELD: char = '\u{1f}';
const PAIR: char = '\u{1e}';
const PREFIX: &str = "@.bn_trap_";

/// Branches to a trap block when `condition` holds, else to `ok`: the block
/// reports `id` with `facts` at the current instruction's span, then joins
/// the function's trap exit (`trap_numeric_overflow`).
pub(crate) fn emit_trap(
    text: &mut String,
    block_id: BlockId,
    state: &mut EmissionState,
    condition: &str,
    ok: String,
    id: DiagId,
    facts: Vec<(&'static str, Fact)>,
) {
    let mut runtime = Vec::new();
    let mut payload = id.code().to_owned();
    for position in [state.span.start, state.span.end] {
        for value in [
            position.source_id.0,
            position.revision.0,
            position.offset as u64,
            position.line as u64,
            position.column as u64,
        ] {
            let _ = write!(payload, "{FIELD}{value}");
        }
    }
    for (name, fact) in facts {
        let value = match fact {
            Fact::Text(text) => text,
            Fact::Runtime(template, operand) => {
                let slot = format!("{SLOT}{}", runtime.len());
                runtime.push(operand);
                template.replacen("{}", &slot, 1)
            }
        };
        let _ = write!(payload, "{FIELD}{name}{PAIR}{value}");
    }
    let symbol = format!("{PREFIX}{}", hex(&payload));
    let mut operands = runtime.into_iter();
    let first = operands.next().unwrap_or_else(|| "0".into());
    let second = operands.next().unwrap_or_else(|| "0".into());
    let site = take_continuation(block_id, state);
    let _ = writeln!(text, "  br i1 {condition}, label %{site}, label %{ok}");
    state.control_flow.label(text, site);
    let _ = writeln!(
        text,
        "  call void @bn_rt_trap_report(ptr {symbol}, i128 {first}, i128 {second})\n  br label %trap_numeric_overflow"
    );
    state.control_flow.label(text, ok);
    state.needs_numeric_overflow_trap = true;
}

/// `INDEX_OUT_OF_BOUNDS` when `condition` holds, with the `i32` index and
/// length read at run time (the interpreter's facts; `context` is `vector`
/// or `region`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_index_trap(
    text: &mut String,
    block_id: BlockId,
    state: &mut EmissionState,
    condition: &str,
    ok: String,
    index: &str,
    length: &str,
    context: &'static str,
) {
    let tag = state.continuation_count;
    let _ = writeln!(
        text,
        "  %idxfact{tag} = sext i32 {index} to i128\n  %lenfact{tag} = sext i32 {length} to i128"
    );
    emit_trap(
        text,
        block_id,
        state,
        condition,
        ok,
        DiagId::INDEX_OUT_OF_BOUNDS,
        vec![
            ("index", Fact::Runtime("{}", format!("%idxfact{tag}"))),
            ("bound", Fact::Runtime("{}", format!("%lenfact{tag}"))),
            ("context", Fact::Text(context.into())),
        ],
    );
}

/// `NUMERIC_OVERFLOW` when `condition` holds, reporting the exact `i128`
/// value that did not fit `ty` (the interpreter's "converting V to T").
pub(crate) fn emit_overflow_trap(
    text: &mut String,
    block_id: BlockId,
    state: &mut EmissionState,
    condition: &str,
    ok: String,
    exact: &str,
    ty: &Type,
) {
    let Type::Integer(kind) = ty else {
        unreachable!("an overflow check on an integer type")
    };
    let template = match kind {
        IntegerType::Byte => "converting {} to BYTE",
        IntegerType::Int8 => "converting {} to INT8",
        IntegerType::Int16 => "converting {} to INT16",
        IntegerType::Int32 => "converting {} to INT32",
        IntegerType::Int64 => "converting {} to INT64",
        IntegerType::UInt16 => "converting {} to UINT16",
        IntegerType::UInt32 => "converting {} to UINT32",
        IntegerType::UInt64 => "converting {} to UINT64",
    };
    emit_trap(
        text,
        block_id,
        state,
        condition,
        ok,
        DiagId::NUMERIC_OVERFLOW,
        vec![("operation", Fact::Runtime(template, exact.into()))],
    );
}

/// The span before the first instruction of a function is lowered.
pub(crate) const fn unknown_span() -> Span {
    let position = Position {
        source_id: Position::UNKNOWN_SOURCE,
        revision: Position::UNKNOWN_REVISION,
        offset: 0,
        line: 1,
        column: 1,
    };
    Span {
        start: position,
        end: position,
    }
}

fn hex(text: &str) -> String {
    text.bytes().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

fn unhex(hex: &str) -> Option<String> {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

/// The diagnostic a trap symbol carries.
fn decode(payload: &str) -> Option<Diagnostic> {
    let mut fields = payload.split(FIELD);
    let id = DiagId::from_code(fields.next()?)?;
    let mut number = || fields.next()?.parse::<u64>().ok();
    let mut position = || {
        Some(Position {
            source_id: SourceId(number()?),
            revision: Revision(number()?),
            offset: usize::try_from(number()?).ok()?,
            line: usize::try_from(number()?).ok()?,
            column: usize::try_from(number()?).ok()?,
        })
    };
    let span = Span {
        start: position()?,
        end: position()?,
    };
    let arguments = fields
        .map(|field| {
            let (name, value) = field.split_once(PAIR)?;
            Some((name.to_owned(), DiagnosticValue::Text(value.to_owned())))
        })
        .collect::<Option<Vec<_>>>()?;
    Diagnostic::structured(
        id,
        arguments,
        vec![Label {
            span,
            style: LabelStyle::Primary,
            text: None,
        }],
    )
    .ok()
}

/// Defines every `@.bn_trap_<hex>` the module references with the text
/// `render` gives its diagnostic, and declares `bn_rt_trap_report`. The
/// Wasm target links no `bn_rt`, so there the report is a no-op.
pub(crate) fn define_trap_globals(
    text: &mut String,
    render: &dyn Fn(&Diagnostic) -> String,
    wasm32: bool,
) {
    let mut symbols = std::collections::BTreeSet::new();
    for (index, _) in text.match_indices(PREFIX) {
        let hex: String = text[index + PREFIX.len()..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        symbols.insert(hex);
    }
    if symbols.is_empty() {
        return;
    }
    let mut definitions = String::new();
    for hex in symbols {
        let rendered = unhex(&hex)
            .and_then(|payload| decode(&payload))
            .map_or_else(
                || "error: runtime failure".into(),
                |diagnostic| render(&diagnostic),
            );
        let (literal, length) = c_literal(&rendered);
        let _ = writeln!(
            definitions,
            "{PREFIX}{hex} = private unnamed_addr constant [{length} x i8] c\"{literal}\""
        );
    }
    if wasm32 {
        definitions.push_str(
            "define void @bn_rt_trap_report(ptr %text, i128 %first, i128 %second) {\n  ret void\n}\n",
        );
    } else {
        definitions.push_str("declare void @bn_rt_trap_report(ptr, i128, i128)\n");
    }
    text.push_str(&definitions);
}

/// `text` as an LLVM `c"..."` body with its NUL, and the byte length.
fn c_literal(text: &str) -> (String, usize) {
    let mut literal = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_graphic() && byte != b'"' && byte != b'\\' || byte == b' ' {
            literal.push(char::from(byte));
        } else {
            let _ = write!(literal, "\\{byte:02X}");
        }
    }
    literal.push_str("\\00");
    (literal, text.len() + 1)
}

#[cfg(test)]
mod tests {
    use super::{c_literal, decode, hex, unhex};

    #[test]
    fn trap_payload_round_trips_to_the_diagnostic() {
        let payload = "INDEX_OUT_OF_BOUNDS\u{1f}0\u{1f}0\u{1f}10\u{1f}2\u{1f}3\u{1f}0\u{1f}0\u{1f}14\u{1f}2\u{1f}7\u{1f}index\u{1e}\u{1}0\u{1f}bound\u{1e}\u{1}1\u{1f}context\u{1e}vector";
        assert_eq!(unhex(&hex(payload)).as_deref(), Some(payload));
        let diagnostic = decode(payload).expect("decodes");
        assert_eq!(diagnostic.code, "INDEX_OUT_OF_BOUNDS");
        assert_eq!(diagnostic.span.start.line, 2);
        assert_eq!(c_literal("a\"b\n"), ("a\\22b\\0A\\00".into(), 5));
    }
}
