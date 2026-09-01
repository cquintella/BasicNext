// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

use std::collections::HashSet;

use crate::{
    module_graph::ModuleId,
    semantic::{SymbolId, Type},
    source::Span,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(pub u32);

impl ValueId {
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub source_name: Option<String>,
    pub functions: Vec<Function>,
    pub bndata_providers: HashSet<ModuleId>,
    pub bnmath_providers: HashSet<ModuleId>,
    pub bnlog_providers: HashSet<ModuleId>,
    pub bnjson_providers: HashSet<ModuleId>,
    pub bnweb_providers: HashSet<ModuleId>,
    pub bndispatch_providers: HashSet<ModuleId>,
    pub filesystem_import: Option<Span>,
    pub console_import: Option<Span>,
    pub network_import: Option<Span>,
    pub bnlog_import: Option<Span>,
    pub bnweb_import: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<SymbolId>,
    pub return_type: Type,
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Clone, Debug)]
pub enum Instruction {
    Constant {
        destination: ValueId,
        value: Constant,
        ty: Type,
        span: Span,
    },
    Default {
        destination: ValueId,
        ty: Type,
        dimensions: Vec<usize>,
        dynamic_dimensions: Vec<ValueId>,
        span: Span,
    },
    Load {
        destination: ValueId,
        symbol: SymbolId,
        ty: Type,
        span: Span,
    },
    Store {
        symbol: SymbolId,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Copy {
        destination: ValueId,
        source: ValueId,
        ty: Type,
        span: Span,
    },
    Unary {
        destination: ValueId,
        operator: String,
        operand: ValueId,
        ty: Type,
        span: Span,
    },
    Binary {
        destination: ValueId,
        operator: String,
        left: ValueId,
        right: ValueId,
        ty: Type,
        span: Span,
    },
    Cast {
        destination: ValueId,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Call {
        destination: ValueId,
        callee: ValueId,
        arguments: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    Input {
        destination: ValueId,
        ty: Type,
        span: Span,
    },
    Vector {
        destination: ValueId,
        values: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    Index {
        destination: ValueId,
        object: ValueId,
        index: ValueId,
        ty: Type,
        span: Span,
    },
    Member {
        destination: ValueId,
        object: ValueId,
        name: String,
        owner: String,
        ty: Type,
        span: Span,
    },
    SetIndex {
        symbol: SymbolId,
        indices: Vec<ValueId>,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Length {
        destination: ValueId,
        vector: ValueId,
        span: Span,
    },
    SizeOf {
        destination: ValueId,
        value: ValueId,
        span: Span,
    },
    Print {
        values: Vec<ValueId>,
        span: Span,
    },
    ClearScreen {
        console: ValueId,
        span: Span,
    },
    Beep {
        console: ValueId,
        span: Span,
    },
    Allocate {
        destination: ValueId,
        type_name: String,
        arguments: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    Delete {
        value: ValueId,
        destructor: Option<String>,
        span: Span,
    },
    SetMember {
        object: ValueId,
        name: String,
        owner: String,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    SetField {
        symbol: SymbolId,
        path: Vec<String>,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    EnsureClass {
        class: String,
        span: Span,
    },
    LoadStatic {
        destination: ValueId,
        class: String,
        field: String,
        ty: Type,
        span: Span,
    },
    StoreStatic {
        class: String,
        field: String,
        value: ValueId,
        ty: Type,
        span: Span,
    },
}

impl Instruction {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Constant { span, .. }
            | Self::Default { span, .. }
            | Self::Load { span, .. }
            | Self::Store { span, .. }
            | Self::Copy { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Cast { span, .. }
            | Self::Call { span, .. }
            | Self::Input { span, .. }
            | Self::Vector { span, .. }
            | Self::Index { span, .. }
            | Self::Member { span, .. }
            | Self::SetIndex { span, .. }
            | Self::Length { span, .. }
            | Self::SizeOf { span, .. }
            | Self::Print { span, .. }
            | Self::ClearScreen { span, .. }
            | Self::Beep { span, .. }
            | Self::Allocate { span, .. }
            | Self::Delete { span, .. }
            | Self::SetMember { span, .. }
            | Self::SetField { span, .. }
            | Self::EnsureClass { span, .. }
            | Self::LoadStatic { span, .. }
            | Self::StoreStatic { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Terminator {
    Jump {
        target: BlockId,
    },
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return {
        value: Option<ValueId>,
    },
    Stop {
        code: ValueId,
    },
}

#[derive(Clone, Debug)]
pub enum Constant {
    Integer(String),
    Float(String),
    String(String),
    Boolean(bool),
    Null,
    NotAvailable,
    EndOfFile,
    Function(String),
    Type(String),
    HostConsole,
    HostArgs,
}
