// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Infallible constructors for diagnostics whose facts the caller writes in
//! its own code, so they always match the registry schema.

use bn_source::Span;

use crate::Diagnostic;

impl Diagnostic {
    /// [`Self::lexical_facts`] for facts the caller writes in its own code: they
    /// always satisfy the `LEXICAL` schema (two text facts), so no error can
    /// come back.
    ///
    /// # Panics
    ///
    /// Never: the two facts are the schema's.
    #[must_use]
    pub fn lexical_error(
        found: impl Into<String>,
        expected: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::lexical_facts(found, expected, span)
            .expect("LEXICAL takes the two text facts it is given")
    }

    /// [`Self::parse_facts`] for facts the caller writes in its own code: they
    /// always satisfy the `PARSE` schema (two text facts), so no error can come
    /// back.
    ///
    /// # Panics
    ///
    /// Never: the two facts are the schema's.
    #[must_use]
    pub fn parse_error(
        expected: impl Into<String>,
        context: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::parse_facts(expected, context, span)
            .expect("PARSE takes the two text facts it is given")
    }
}
