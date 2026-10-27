// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::source::Span;

#[derive(Debug)]
pub struct Program {
    pub source_name: Option<String>,
    pub items: Vec<Item>,
}

#[allow(clippy::large_enum_variant)] // Syntax nodes stay direct while the AST is internal.
#[derive(Debug)]
pub enum Item {
    Import {
        /// `EXPORT IMPORT` (E3): the alias is an export of this module.
        exported: bool,
        path: Vec<String>,
        alias: String,
        span: Span,
    },
    Constant {
        exported: bool,
        name: String,
        type_ref: TypeReference,
        initializer: Expression,
        span: Span,
    },
    Declaration {
        exported: bool,
        asynchronous: bool,
        kind: DeclarationKind,
        name: String,
        base_class: Option<TypeReference>,
        interfaces: Vec<String>,
        signature: Option<FunctionSignature>,
        statements: Vec<Statement>,
        span: Span,
    },
}

#[derive(Debug)]
pub struct FunctionSignature {
    pub parameters: Vec<Parameter>,
    pub return_type: TypeReference,
    pub span: Span,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: String,
    pub type_ref: TypeReference,
    pub span: Span,
}

#[allow(clippy::large_enum_variant)] // Syntax nodes stay direct while the AST is internal.
#[derive(Debug)]
pub enum Statement {
    If {
        branches: Vec<IfBranch>,
        otherwise: Option<Block>,
        span: Span,
    },
    While {
        condition: Expression,
        body: Block,
        span: Span,
    },
    Repeat {
        body: Block,
        condition: Expression,
        span: Span,
    },
    For {
        header: ForHeader,
        body: Block,
        span: Span,
    },
    Binding {
        constant: bool,
        visibility: Option<Visibility>,
        is_static: bool,
        name: String,
        additional_names: Vec<String>,
        additional_name_spans: Vec<Span>,
        type_ref: TypeReference,
        initialized: bool,
        initializer: Option<Expression>,
        additional_initializers: Vec<Expression>,
        span: Span,
    },
    /// `LET [name AS type, ...] = source` (0.6.5). Lowering turns it into
    /// ordinary bindings; the backends never see this node.
    Destructuring {
        bindings: Vec<DestructuringBinding>,
        source: Expression,
        span: Span,
    },
    Assignment {
        target: Expression,
        operator: String,
        value: Expression,
        span: Span,
    },
    Return {
        value: Option<Expression>,
        span: Span,
    },
    Print {
        values: Vec<Expression>,
        span: Span,
    },
    Input {
        prompt: Option<Box<Expression>>,
        target: Expression,
        span: Span,
    },
    ClearScreen {
        console: Expression,
        span: Span,
    },
    Beep {
        console: Expression,
        span: Span,
    },
    Release {
        value: Expression,
        span: Span,
    },
    Stop {
        code: Expression,
        span: Span,
    },
    Control {
        kind: String,
        target: String,
        span: Span,
    },
    Call {
        expression: Expression,
        span: Span,
    },
    MemberFunction {
        name: String,
        visibility: Option<Visibility>,
        is_static: bool,
        /// `OVERRIDE` marker (0.5.2 O2): the method replaces an inherited one.
        is_override: bool,
        parameters: Vec<Parameter>,
        signature: Option<FunctionSignature>,
        body: Option<Block>,
        span: Span,
    },
}

/// One name in a destructuring `LET`. `type_ref` is absent when the binding
/// takes the type of the value it receives.
#[derive(Debug)]
pub struct DestructuringBinding {
    pub name: String,
    pub type_ref: Option<TypeReference>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

#[allow(clippy::large_enum_variant)] // Counted and foreach headers have distinct grammar data.
#[derive(Debug)]
pub enum ForHeader {
    Counted {
        variable: String,
        /// Absent `AS` means `INTEGER` (C10).
        type_ref: Option<TypeReference>,
        variable_span: Span,
        start: Expression,
        end: Expression,
        step: Option<Expression>,
    },
    Each {
        variable: String,
        /// Absent `AS` means the element type (C11).
        type_ref: Option<TypeReference>,
        variable_span: Span,
        iterable: Expression,
    },
}

#[derive(Debug)]
pub struct IfBranch {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypeReference {
    pub alternatives: Vec<TypeAtom>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypeAtom {
    pub name: String,
    pub parts: Vec<String>,
    pub dimensions: Vec<VectorDimension>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum VectorDimension {
    Literal { value: String, span: Span },
    Expression(Expression),
}

#[derive(Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeclarationKind {
    Function,
    Class,
    Struct,
    Interface,
}

#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Literal(Literal),
    Name {
        name: String,
    },
    Super,
    Input {
        prompt: Option<Box<Expression>>,
    },
    HostCapability {
        name: String,
    },
    Length {
        operand: Box<Expression>,
    },
    SizeOf {
        operand: Box<Expression>,
    },
    TypeTest {
        type_ref: TypeReference,
    },
    Vector {
        values: Vec<Expression>,
    },
    New {
        type_name: String,
        arguments: Vec<Expression>,
    },
    Unary {
        operator: String,
        operand: Box<Expression>,
    },
    Binary {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Member {
        object: Box<Expression>,
        name: String,
    },
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    Cast {
        value: Box<Expression>,
        type_ref: TypeReference,
    },
    /// `++t` / `--t` (prefix, yields the new value) and `t++` / `t--`
    /// (postfix, yields the old value); `delta` is `+1` or `-1` (0.6.1 S1').
    Increment {
        target: Box<Expression>,
        delta: i8,
        prefix: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Literal {
    Integer(String),
    Float(String),
    String(String),
    TypeName(String),
    Special(String),
    Boolean(bool),
    Null,
    NotAvailable,
    EndOfFile,
}

/// The binary operator of a compound assignment operator (`PlusAssign` is
/// `Plus`), or `None` for a plain `=` or an unknown spelling.
#[must_use]
pub fn compound_binary_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "PlusAssign" => Some("Plus"),
        "MinusAssign" => Some("Minus"),
        "StarAssign" => Some("Star"),
        "SlashAssign" => Some("Slash"),
        "PercentAssign" => Some("Percent"),
        "PowerAssign" => Some("Power"),
        _ => None,
    }
}

impl Statement {
    /// `target op= value` is `target = target op value` (0.6.md, "Compound
    /// assignment"), so analysis applies the one rule of `=`. The statement
    /// keeps the compound `operator` because lowering still evaluates the
    /// place once and first. The left operand of the binary reads the place;
    /// its span is the empty span just after the target, so the type the
    /// analyzer gives that read (possibly narrowed) never overwrites the
    /// target's.
    #[must_use]
    pub fn assignment(target: Expression, operator: String, value: Expression, span: Span) -> Self {
        let Some(binary_operator) = compound_binary_operator(&operator) else {
            return Self::Assignment {
                target,
                operator,
                value,
                span,
            };
        };
        let read = Expression {
            span: Span {
                start: target.span.end,
                end: target.span.end,
            },
            kind: target.kind.clone(),
        };
        let binary = Expression {
            span: Span {
                start: target.span.start,
                end: value.span.end,
            },
            kind: ExpressionKind::Binary {
                operator: binary_operator.into(),
                left: Box::new(read),
                right: Box::new(value),
            },
        };
        Self::Assignment {
            target,
            operator,
            value: binary,
            span,
        }
    }
}
