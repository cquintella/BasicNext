// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An integer constant that reaches no destination is `INTEGER` (`INT32`)
//! (0.6.md, "Integer constants without a destination"; BDFL decision,
//! 2026-10-28, bucket 0.6.11 S5.2).
//!
//! Analysis folds a constant such as `10 ** 30` to its exact value so that a
//! destination can check it (`LET x AS INT64 = 2 ** 40`). Where no
//! destination follows — `PRINT`, `TYPEOF`, a comparison of two constants —
//! each literal must fit `INT32`, and every operation of the constant becomes
//! `INT32` arithmetic. An overflow then stops the program with
//! `NUMERIC_OVERFLOW` on both backends, as `n ** 30` does for `n AS INT32`.

#![allow(clippy::wildcard_imports)]
use super::*;

/// The type an integer constant takes without a destination.
const UNDESTINED_INTEGER: IntegerType = IntegerType::Int32;

impl Analyzer {
    /// Settles `expression` when its type is still an integer constant and
    /// returns the type it then has.
    pub(crate) fn settle_integer_constant(
        &mut self,
        expression: &Expression,
        context: &str,
    ) -> Result<Type, Diagnostic> {
        let Some(ty) = self.recorded_type(expression.span) else {
            return Ok(Type::Unknown);
        };
        if !matches!(ty, Type::IntegerLiteral(_)) {
            return Ok(ty);
        }
        self.settle_node(expression, context)?;
        Ok(Type::Integer(UNDESTINED_INTEGER))
    }

    /// A literal, or a minus sign applied to one (0.6.md, C1), is a leaf: it
    /// must fit and keeps its literal type, which every backend reads as a
    /// constant of the operation's type. An operation becomes `INT32`.
    fn settle_node(&mut self, expression: &Expression, context: &str) -> Result<(), Diagnostic> {
        if !matches!(
            self.recorded_type(expression.span),
            Some(Type::IntegerLiteral(_))
        ) {
            return Ok(());
        }
        if let Some(value) = literal_leaf(expression) {
            if !integer_literal_fits(&value.to_string(), UNDESTINED_INTEGER) {
                return Err(integer_out_of_range(
                    UNDESTINED_INTEGER,
                    &value.to_string(),
                    context,
                    expression.span,
                ));
            }
            return Ok(());
        }
        match &expression.kind {
            ExpressionKind::Unary { operand, .. } => self.settle_node(operand, context)?,
            ExpressionKind::Binary { left, right, .. } => {
                self.settle_node(left, context)?;
                self.settle_node(right, context)?;
            }
            _ => return Ok(()),
        }
        self.record_expression(expression.span, &Type::Integer(UNDESTINED_INTEGER), None);
        Ok(())
    }

    /// The operands that reach no destination when both are integer
    /// constants: the sides of a comparison (its result is `BOOLEAN`), the
    /// operands of `/` and of `**` with a negative constant exponent (their
    /// result is `FLOAT64`; BDFL decision, 2026-10-28), and the operands of an
    /// integer operation analysis cannot fold (`1 DIV 0`, `10 ** 40`), which
    /// then runs as `INT32` arithmetic. The shifted value of `SHL`/`SHR` never
    /// reaches one: the count is checked against its width.
    pub(crate) fn settle_undestined_operands(
        &mut self,
        expression: &Expression,
        left_type: Type,
        right_type: Type,
    ) -> Result<(Type, Type), Diagnostic> {
        let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &expression.kind
        else {
            return Ok((left_type, right_type));
        };
        let context = bn_catalog::fill(
            "frontend",
            "msg.operand_of",
            &[("operator", &bn_types::operator_spelling(operator))],
        );
        let both_constants = matches!(
            (&left_type, &right_type),
            (Type::IntegerLiteral(_), Type::IntegerLiteral(_))
        );
        let undestined = is_comparison(operator)
            || has_float_result(operator, right)
            || constant_integer(expression).is_none();
        if both_constants && undestined {
            let left_type = self.settle_integer_constant(left, &context)?;
            let right_type = self.settle_integer_constant(right, &context)?;
            return Ok((left_type, right_type));
        }
        if matches!(operator.as_str(), "SHL" | "SHR") {
            return Ok((self.settle_integer_constant(left, &context)?, right_type));
        }
        Ok((left_type, right_type))
    }

    fn recorded_type(&self, span: Span) -> Option<Type> {
        self.expressions
            .iter()
            .find(|expression| expression.span == span)
            .map(|expression| expression.ty.clone())
    }
}

/// The value of a literal or of a minus sign applied to one.
fn literal_leaf(expression: &Expression) -> Option<i128> {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Integer(value)) => parse_integer(value),
        ExpressionKind::Unary { operator, operand }
            if operator == "Minus"
                && matches!(operand.kind, ExpressionKind::Literal(Literal::Integer(_))) =>
        {
            constant_integer(expression)
        }
        _ => None,
    }
}

/// `/`, and `**` with a negative constant exponent, are `FLOAT64` on
/// integer operands (0.6.md, "Operators").
fn has_float_result(operator: &str, right: &Expression) -> bool {
    operator == "Slash"
        || operator == "Power" && constant_integer(right).is_some_and(|exponent| exponent < 0)
}

fn is_comparison(operator: &str) -> bool {
    matches!(
        operator,
        "Assign" | "NotEqual" | "Less" | "LessEqual" | "Greater" | "GreaterEqual"
    )
}
