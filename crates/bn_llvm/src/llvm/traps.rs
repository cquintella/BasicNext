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

use crate::ir::{BinaryOp, CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use bn_diag::{DiagId, Diagnostic, DiagnosticValue, Label, LabelStyle};
use bn_source::{Position, Revision, SourceId, Span};

/// One argument of a trap diagnostic.
pub(crate) enum Fact {
    /// Known when compiling.
    Text(String),
    /// `template` with its `{}` replaced by an `i128` LLVM operand read when
    /// the trap fires (at most two runtime facts per site).
    Runtime(&'static str, String),
    /// A runtime fact with a dynamically constructed template.
    RuntimeDynamic(String, String),
}

/// A slot for runtime fact `index` in rendered text; `bn_rt_trap_report`
/// replaces it with the value. Control characters never occur in rendered
/// catalog text or BN source excerpts.
const SLOT: char = '\u{1}';
const FIELD: char = '\u{1f}';
const PAIR: char = '\u{1e}';
const PREFIX: &str = "@.bn_trap_";
/// A site whose runtime function records its failure (`RuntimeFailure`):
/// one rendered text per identity the call can raise, every fact a named
/// slot `\u{1}name\u{2}` the runtime fills from the record.
const SET_PREFIX: &str = "@.bn_trapset_";
const NAMED_END: char = '\u{2}';
const ENTRY: char = '\u{1d}';
const CODE_END: char = '\u{1e}';

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
    let (symbol, runtime) = trap_symbol(state, id, facts);
    let mut operands = runtime.into_iter();
    let first = operands.next().unwrap_or_else(|| "0".into());
    let second = operands.next().unwrap_or_else(|| "0".into());
    let site = take_continuation(block_id, state);
    emit_branch_to_site(text, condition, &site, &ok);
    state.control_flow.label(text, site.clone());
    // Each fact crosses the ABI as two i64 halves (no portable i128 C ABI).
    let mut args = vec![(T::Ptr, O::raw(symbol))];
    for (index, operand) in [first, second].into_iter().enumerate() {
        let name = format!("{site}.fact{index}");
        let r = |part: &str| O::reg(format!("{name}.{part}"));
        let operand = O::raw(operand);
        let low = I::cast(CastOp::Trunc, T::I128, operand.clone(), T::I64);
        text.assign(format!("{name}.lo"), low);
        let shifted = I::binary(BinaryOp::LShr, T::I128, operand, O::int(64));
        text.assign(format!("{name}.shr"), shifted);
        text.assign(
            format!("{name}.hi"),
            I::cast(CastOp::Trunc, T::I128, r("shr"), T::I64),
        );
        args.extend([(T::I64, r("lo")), (T::I64, r("hi"))]);
    }
    text.emit(I::call(T::Void, "bn_rt_trap_report", args));
    text.emit(I::Br {
        dest: "trap_numeric_overflow".into(),
    });
    state.control_flow.label(text, ok);
    state.needs_numeric_overflow_trap = true;
}

/// The constant symbol carrying `id` with `facts` at the current span, and
/// the runtime operands in slot order. A `bn_rt` function that fails inside
/// takes the symbol as an argument and supplies the runtime facts itself.
pub(crate) fn trap_symbol(
    state: &EmissionState,
    id: DiagId,
    facts: Vec<(&'static str, Fact)>,
) -> (String, Vec<String>) {
    let mut runtime = Vec::new();
    let mut payload = id.code().to_owned();
    payload.push_str(&span_payload(state));
    for (name, fact) in facts {
        let value = match fact {
            Fact::Text(text) => text,
            Fact::Runtime(template, operand) => {
                let slot = format!("{SLOT}{}", runtime.len());
                runtime.push(operand);
                template.replacen("{}", &slot, 1)
            }
            Fact::RuntimeDynamic(template, operand) => {
                let slot = format!("{SLOT}{}", runtime.len());
                runtime.push(operand);
                template.replacen("{}", &slot, 1)
            }
        };
        let _ = write!(payload, "{FIELD}{name}{PAIR}{value}");
    }
    (format!("{PREFIX}{}", hex(&payload)), runtime)
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
    for (name, value) in [("idxfact", index), ("lenfact", length)] {
        let wide = I::cast(CastOp::SExt, T::I32, O::raw(value), T::I128);
        text.assign(format!("{name}{tag}"), wide);
    }
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

/// The symbol of a failure set at the current span: the diagnostic of each
/// identity in `ids`, rendered with named slots for all its arguments.
pub(crate) fn trap_set_symbol(state: &EmissionState, ids: &[DiagId]) -> String {
    let mut payload = span_payload(state);
    for id in ids {
        let _ = write!(payload, "{FIELD}{}", id.code());
    }
    format!("{SET_PREFIX}{}", hex(&payload))
}

/// The ten span numbers, each preceded by `FIELD`.
fn span_payload(state: &EmissionState) -> String {
    let mut payload = String::new();
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
    payload
}

/// Branches to `trap_bn_rt` when `failed` holds, printing the failure the
/// runtime recorded with this site's texts for `ids` first.
pub(crate) fn emit_failure_trap(
    text: &mut String,
    block_id: BlockId,
    state: &mut EmissionState,
    failed: &str,
    ok: String,
    ids: &[DiagId],
) {
    let set = trap_set_symbol(state, ids);
    let site = take_continuation(block_id, state);
    emit_branch_to_site(text, failed, &site, &ok);
    state.control_flow.label(text, site);
    let args = vec![(T::Ptr, O::raw(set))];
    text.emit(I::call(T::Void, "bn_rt_trap_report_failure", args));
    text.emit(I::Br {
        dest: "trap_bn_rt".into(),
    });
    state.control_flow.label(text, ok);
    state.needs_bn_rt_trap = true;
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

/// The span encoded by `span_payload`, read from `fields`.
fn decode_span<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Option<Span> {
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
    Some(Span {
        start: position()?,
        end: position()?,
    })
}

fn primary(span: Span) -> Vec<Label> {
    vec![Label {
        span,
        style: LabelStyle::Primary,
        text: None,
    }]
}

/// The diagnostic a trap symbol carries.
fn decode(payload: &str) -> Option<Diagnostic> {
    let mut fields = payload.split(FIELD);
    let id = DiagId::from_code(fields.next()?)?;
    let span = decode_span(&mut fields)?;
    let arguments = fields
        .map(|field| {
            let (name, value) = field.split_once(PAIR)?;
            Some((name.to_owned(), DiagnosticValue::Text(value.to_owned())))
        })
        .collect::<Option<Vec<_>>>()?;
    Diagnostic::structured(id, arguments, primary(span)).ok()
}

/// The entries of a failure set: `CODE\u{1e}text` joined by `\u{1d}`.
fn render_set(payload: &str, render: &dyn Fn(&Diagnostic) -> String) -> Option<String> {
    let mut fields = payload.split(FIELD).skip(1);
    let span = decode_span(&mut fields)?;
    let entries = fields
        .map(|code| {
            let id = DiagId::from_code(code)?;
            let arguments = id
                .argument_schema()
                .iter()
                .map(|argument| {
                    (
                        argument.name.to_owned(),
                        DiagnosticValue::Text(format!("{SLOT}{}{NAMED_END}", argument.name)),
                    )
                })
                .collect();
            let diagnostic = Diagnostic::structured(id, arguments, primary(span)).ok()?;
            Some(format!("{code}{CODE_END}{}", render(&diagnostic)))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(entries.join(&ENTRY.to_string()))
}

/// Defines every `@.bn_trap_<hex>` the module references with the text
/// `render` gives its diagnostic, and declares `bn_rt_trap_report`. The
/// Wasm target links no `bn_rt`, so there the report is a no-op.
pub(crate) fn define_trap_globals(
    text: &mut String,
    render: &dyn Fn(&Diagnostic) -> String,
    wasm32: bool,
) {
    let referenced = |prefix: &str| {
        text.match_indices(prefix)
            .map(|(index, _)| {
                text[index + prefix.len()..]
                    .chars()
                    .take_while(char::is_ascii_hexdigit)
                    .collect::<String>()
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let symbols = referenced(PREFIX);
    let sets = referenced(SET_PREFIX);
    let mut definitions = String::new();
    for hex in &sets {
        let rendered = unhex(hex)
            .and_then(|payload| render_set(&payload, render))
            .unwrap_or_default();
        let (literal, length) = c_literal(&rendered);
        let _ = writeln!(
            definitions,
            "{SET_PREFIX}{hex} = private unnamed_addr constant [{length} x i8] c\"{literal}\""
        );
    }
    // The ARC glue reports a broken core invariant with the null set.
    if !sets.is_empty() || text.contains("@bn_rt_trap_report_failure(ptr null)") {
        definitions.push_str(if wasm32 {
            "define void @bn_rt_trap_report_failure(ptr %set) {\n  ret void\n}\n"
        } else {
            "declare void @bn_rt_trap_report_failure(ptr)\n"
        });
    }
    if symbols.is_empty() {
        text.push_str(&definitions);
        return;
    }
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
            "define void @bn_rt_trap_report(ptr %text, i64 %a, i64 %b, i64 %c, i64 %d) {\n  ret void\n}\n",
        );
    } else {
        definitions.push_str("declare void @bn_rt_trap_report(ptr, i64, i64, i64, i64)\n");
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

/// `br i1 condition, label %site, label %ok`.
fn emit_branch_to_site(text: &mut String, condition: &str, site: &str, ok: &str) {
    text.emit(I::CondBr {
        cond: O::raw(condition),
        true_dest: site.into(),
        false_dest: ok.into(),
    });
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
