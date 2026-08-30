// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::source::{SourceFile, Span};

#[derive(Debug)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    #[must_use]
    pub fn lexical(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "E0001",
            message: message.into(),
            span,
        }
    }

    #[must_use]
    pub fn render(&self, source: &SourceFile) -> String {
        let position = self.span.start;
        format!(
            "error[{code}]: {message}\n --> {name}:{line}:{column}\n  |\n{line:>3} | {text}\n  | {padding}^",
            code = self.code,
            message = self.message,
            name = source.name,
            line = position.line,
            column = position.column,
            text = source.line(position.line),
            padding = " ".repeat(position.column.saturating_sub(1))
        )
    }
}
