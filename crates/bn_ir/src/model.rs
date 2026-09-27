// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

use std::collections::{BTreeMap, HashMap, HashSet};

use bn_source::Span;
use bn_types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SymbolId(pub u32);

impl SymbolId {
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Module-local identity of one interned record or class field name.
///
/// This is deliberately distinct from [`SymbolId`], which identifies a
/// binding rather than a field declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FieldId(pub u32);

impl FieldId {
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Checked positional address of a field in its owner's complete layout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FieldSlot(pub u32);

impl FieldSlot {
    #[must_use]
    pub const fn from_raw(slot: u32) -> Self {
        Self(slot)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Resolved field access carried by lowered IR instructions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldRef {
    pub owner: String,
    pub id: FieldId,
    pub slot: FieldSlot,
}

/// One field in an ordered record/class layout.
#[derive(Clone, Debug)]
pub struct FieldLayoutEntry {
    pub id: FieldId,
    pub slot: FieldSlot,
    pub ty: Type,
    pub declaring_owner: String,
    pub weak: bool,
    pub span: Span,
}

/// Complete, base-first positional layout for one record/class owner.
#[derive(Clone, Debug)]
pub struct FieldLayout {
    pub owner: String,
    pub fields: Vec<FieldLayoutEntry>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModuleId(pub u32);

impl From<bn_types::ModuleId> for ModuleId {
    fn from(value: bn_types::ModuleId) -> Self {
        Self(value.0)
    }
}

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
    /// Interned field spellings, indexed by [`FieldId`]. Runtime values never
    /// carry these names; they only consume validated [`FieldSlot`] values.
    pub field_names: Vec<String>,
    /// Complete record/class layouts keyed by fully-qualified owner. The
    /// ordered map makes metadata construction deterministic.
    pub field_layouts: BTreeMap<String, FieldLayout>,
    /// Fully qualified class identity to its fully qualified direct base.
    /// The relation validates inherited field-layout prefixes and dispatch
    /// without depending on frontend semantic types.
    pub class_bases: HashMap<String, String>,
    pub bndata_providers: HashSet<ModuleId>,
    pub bnmath_providers: HashSet<ModuleId>,
    pub bnlog_providers: HashSet<ModuleId>,
    pub bnjson_providers: HashSet<ModuleId>,
    pub bnweb_providers: HashSet<ModuleId>,
    pub bndispatch_providers: HashSet<ModuleId>,
    pub bncrypto_providers: HashSet<ModuleId>,
    pub filesystem_import: Option<Span>,
    pub clock_import: Option<Span>,
    pub random_import: Option<Span>,
    pub console_import: Option<Span>,
    pub network_import: Option<Span>,
    pub exec_import: Option<Span>,
    pub bnlog_import: Option<Span>,
    pub bnweb_import: Option<Span>,
}

/// Role of a function in the module (bucket 0.5.1c §3.2). The name of a
/// synthesised function still follows `crate::names`, but that spelling is
/// informative; `validate` and both backends rely on this field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FunctionKind {
    /// A user-declared function or method.
    User,
    /// The program entry point (`Start`).
    Entry,
    /// Class constructor body.
    Constructor,
    /// Class destructor body, run by ARC when the strong count reaches zero.
    Destructor,
    /// Field-initialiser prologue run by `NEW` before the constructor.
    FieldInit,
    /// Construction helper: allocate, run `FieldInit`, then `Constructor`.
    Init,
    /// Value-type (struct) default constructor.
    Default,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    /// What this function is. Backends read this, never the name.
    pub kind: FunctionKind,
    /// Class / struct / module that owns the function, spelled as in the IR
    /// (`Counter`, `#3.Box`); `None` for free functions and the entry point.
    pub owner: Option<String>,
    pub asynchronous: bool,
    pub parameters: Vec<SymbolId>,
    /// Local bindings declared with `AS WEAK`.
    pub weak_symbols: HashSet<SymbolId>,
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
    Phi {
        destination: ValueId,
        incoming: Vec<(BlockId, ValueId)>,
        ty: Type,
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
    DispatchSubmit {
        destination: ValueId,
        callee: ValueId,
        queue: ValueId,
        task: ValueId,
        arguments: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    DispatchAwait {
        destination: ValueId,
        callee: ValueId,
        ticket: ValueId,
        timeout: ValueId,
        ty: Type,
        span: Span,
    },
    Input {
        destination: ValueId,
        prompt: Option<ValueId>,
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
        /// Resolved positional reference for record data. Function members do
        /// not use record storage and therefore retain `None` here.
        field: Option<FieldRef>,
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
    /// Stores through an indexed object member without materializing a copy
    /// of the member value. This preserves object-field aliasing for values
    /// such as vectors and pointer regions.
    SetMemberIndex {
        object: ValueId,
        field: Option<FieldRef>,
        name: String,
        owner: String,
        indices: Vec<ValueId>,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    /// Stores through an indexed field path rooted in a mutable binding. This
    /// preserves value semantics for nested structs as well as object handles.
    SetFieldIndex {
        symbol: SymbolId,
        root_owner: String,
        path: Vec<String>,
        fields: Option<Vec<FieldRef>>,
        indices: Vec<ValueId>,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    /// Stores through an indexed static field.
    SetStaticIndex {
        class: String,
        field: String,
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
    Release {
        value: ValueId,
        destructor: Option<String>,
        span: Span,
    },
    SetMember {
        object: ValueId,
        field: Option<FieldRef>,
        name: String,
        owner: String,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    SetField {
        symbol: SymbolId,
        root_owner: String,
        path: Vec<String>,
        fields: Option<Vec<FieldRef>>,
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
            | Self::Phi { span, .. }
            | Self::Load { span, .. }
            | Self::Store { span, .. }
            | Self::Copy { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Cast { span, .. }
            | Self::Call { span, .. }
            | Self::DispatchSubmit { span, .. }
            | Self::DispatchAwait { span, .. }
            | Self::Input { span, .. }
            | Self::Vector { span, .. }
            | Self::Index { span, .. }
            | Self::Member { span, .. }
            | Self::SetIndex { span, .. }
            | Self::SetMemberIndex { span, .. }
            | Self::SetFieldIndex { span, .. }
            | Self::SetStaticIndex { span, .. }
            | Self::Length { span, .. }
            | Self::SizeOf { span, .. }
            | Self::Print { span, .. }
            | Self::ClearScreen { span, .. }
            | Self::Beep { span, .. }
            | Self::Allocate { span, .. }
            | Self::Release { span, .. }
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

impl Module {
    /// Resolves an interned field spelling to its checked positional address.
    #[must_use]
    pub fn field_ref(&self, owner: &str, name: &str) -> Option<FieldRef> {
        let layout = self.field_layouts.get(owner)?;
        layout.fields.iter().find_map(|entry| {
            let index = usize::try_from(entry.id.0).ok()?;
            (self.field_names.get(index)?.as_str() == name).then(|| FieldRef {
                owner: owner.into(),
                id: entry.id,
                slot: entry.slot,
            })
        })
    }

    /// Returns whether a resolved field slot has weak ownership.
    #[must_use]
    pub fn field_is_weak(&self, field: &FieldRef) -> bool {
        self.field_layouts
            .get(&field.owner)
            .and_then(|layout| layout.fields.get(usize::try_from(field.slot.value()).ok()?))
            .is_some_and(|entry| entry.weak && entry.id == field.id)
    }

    /// The program entry point, if the module has one.
    #[must_use]
    pub fn entry(&self) -> Option<&Function> {
        self.functions
            .iter()
            .find(|function| function.kind == FunctionKind::Entry)
    }

    /// The function of a synthesised `kind` owned by `owner` (a class or
    /// struct name as spelled in the IR). Backends select constructors,
    /// destructors, field initialisers and defaults through this, never by
    /// decoding the name.
    #[must_use]
    pub fn function_of_kind(&self, kind: FunctionKind, owner: &str) -> Option<&Function> {
        self.functions
            .iter()
            .find(|function| function.kind == kind && function.owner.as_deref() == Some(owner))
    }

    /// The standard library (`"BNMath"`, `"BNData"`, …) that module `id`
    /// provides, if any. The interpreter routes `#<id>.<member>` callees to the
    /// library provider registered under this name.
    #[must_use]
    pub fn standard_library_of(&self, id: ModuleId) -> Option<&'static str> {
        [
            (&self.bnmath_providers, "BNMath"),
            (&self.bndata_providers, "BNData"),
            (&self.bnlog_providers, "BNLog"),
            (&self.bnjson_providers, "BNJson"),
            (&self.bnweb_providers, "BNWeb"),
            (&self.bndispatch_providers, "BNDispatch"),
            (&self.bncrypto_providers, "BNCrypto"),
        ]
        .into_iter()
        .find(|(providers, _)| providers.contains(&id))
        .map(|(_, name)| name)
    }

    /// Kind of the function named `name`, if it exists in this module.
    #[must_use]
    pub fn kind_of(&self, name: &str) -> Option<FunctionKind> {
        self.functions
            .iter()
            .find(|function| function.name == name)
            .map(|function| function.kind)
    }
}
