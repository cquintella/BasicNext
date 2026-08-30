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
        path: Vec<String>,
        alias: String,
        span: Span,
    },
    Declaration {
        exported: bool,
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
        type_ref: TypeReference,
        initialized: bool,
        initializer: Option<Expression>,
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
    ClearScreen {
        console: Expression,
        span: Span,
    },
    Beep {
        console: Expression,
        span: Span,
    },
    Delete {
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
        parameters: Vec<Parameter>,
        signature: Option<FunctionSignature>,
        body: Option<Block>,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Public,
    Private,
}

#[allow(clippy::large_enum_variant)] // Counted and foreach headers have distinct grammar data.
#[derive(Debug)]
pub enum ForHeader {
    Counted {
        variable: String,
        type_ref: TypeReference,
        start: Expression,
        end: Expression,
        step: Option<Expression>,
    },
    Each {
        variable: String,
        type_ref: TypeReference,
        iterable: Expression,
    },
}

#[derive(Debug)]
pub struct IfBranch {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug)]
pub struct TypeReference {
    pub alternatives: Vec<TypeAtom>,
    pub span: Span,
}

#[derive(Debug)]
pub struct TypeAtom {
    pub name: String,
    pub parts: Vec<String>,
    pub span: Span,
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

#[derive(Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum ExpressionKind {
    Literal(Literal),
    Name {
        name: String,
    },
    Super,
    Input,
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
}

#[derive(Debug)]
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
