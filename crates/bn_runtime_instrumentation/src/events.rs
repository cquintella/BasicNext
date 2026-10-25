// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_source::Span;
use serde_json::{Value as JsonValue, json};

/// Observational execution events emitted by the Basic Next interpreter.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    FunctionEntry {
        function: String,
        depth: usize,
        span: Span,
        args: Vec<(String, String)>,
    },
    FunctionReturn {
        function: String,
        depth: usize,
        span: Span,
        return_value: Option<String>,
    },
    Statement {
        stmt_kind: String,
        depth: usize,
        span: Span,
    },
    LineChange {
        new_line: usize,
        depth: usize,
        span: Span,
    },
    VariableWrite {
        name: String,
        new_value: String,
        depth: usize,
        span: Span,
    },
    VariableRead {
        name: String,
        value: String,
        depth: usize,
        span: Span,
    },
    ErrorThrown {
        code: String,
        message: String,
        depth: usize,
        span: Span,
    },
    ErrorCaught {
        code: String,
        message: String,
        depth: usize,
        span: Span,
    },
}

impl Event {
    /// Source span associated with this event.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::FunctionEntry { span, .. }
            | Self::FunctionReturn { span, .. }
            | Self::Statement { span, .. }
            | Self::LineChange { span, .. }
            | Self::VariableWrite { span, .. }
            | Self::VariableRead { span, .. }
            | Self::ErrorThrown { span, .. }
            | Self::ErrorCaught { span, .. } => *span,
        }
    }

    /// Execution depth (stack height) associated with this event.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::FunctionEntry { depth, .. }
            | Self::FunctionReturn { depth, .. }
            | Self::Statement { depth, .. }
            | Self::LineChange { depth, .. }
            | Self::VariableWrite { depth, .. }
            | Self::VariableRead { depth, .. }
            | Self::ErrorThrown { depth, .. }
            | Self::ErrorCaught { depth, .. } => *depth,
        }
    }

    /// Converts this event to a JSON representation suitable for NDJSON streams.
    #[must_use]
    pub fn to_json(&self) -> JsonValue {
        match self {
            Self::FunctionEntry {
                function,
                depth,
                span,
                args,
            } => json!({
                "type": "FunctionEntry",
                "function": function,
                "depth": depth,
                "span": format_span(span),
                "args": args.iter().map(|(n, v)| json!({ "name": n, "value": v })).collect::<Vec<_>>(),
            }),
            Self::FunctionReturn {
                function,
                depth,
                span,
                return_value,
            } => json!({
                "type": "FunctionReturn",
                "function": function,
                "depth": depth,
                "span": format_span(span),
                "return_value": return_value,
            }),
            Self::Statement {
                stmt_kind,
                depth,
                span,
            } => json!({
                "type": "Statement",
                "stmt_kind": stmt_kind,
                "depth": depth,
                "span": format_span(span),
            }),
            Self::LineChange {
                new_line,
                depth,
                span,
            } => json!({
                "type": "LineChange",
                "new_line": new_line,
                "depth": depth,
                "span": format_span(span),
            }),
            Self::VariableWrite {
                name,
                new_value,
                depth,
                span,
            } => json!({
                "type": "VariableWrite",
                "name": name,
                "new_value": new_value,
                "depth": depth,
                "span": format_span(span),
            }),
            Self::VariableRead {
                name,
                value,
                depth,
                span,
            } => json!({
                "type": "VariableRead",
                "name": name,
                "value": value,
                "depth": depth,
                "span": format_span(span),
            }),
            Self::ErrorThrown {
                code,
                message,
                depth,
                span,
            } => json!({
                "type": "ErrorThrown",
                "code": code,
                "message": message,
                "depth": depth,
                "span": format_span(span),
            }),
            Self::ErrorCaught {
                code,
                message,
                depth,
                span,
            } => json!({
                "type": "ErrorCaught",
                "code": code,
                "message": message,
                "depth": depth,
                "span": format_span(span),
            }),
        }
    }
}

fn format_span(span: &Span) -> JsonValue {
    json!({
        "source_id": span.source_id().0,
        "start": {
            "line": span.start.line,
            "column": span.start.column,
            "offset": span.start.offset,
        },
        "end": {
            "line": span.end.line,
            "column": span.end.column,
            "offset": span.end.offset,
        }
    })
}
