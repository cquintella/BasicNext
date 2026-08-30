// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use crate::{
    ast::{
        Block as AstBlock, DeclarationKind, Expression, ExpressionKind, ForHeader,
        FunctionSignature, Item, Literal, Program, Statement, TypeAtom, TypeReference,
    },
    diagnostic::Diagnostic,
    module_graph::{ModuleGraph, ModuleId, StandardModule},
    semantic::{IntegerType, SemanticModel, SymbolId, Type, static_len},
    source::Span,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(pub u32);

#[derive(Debug)]
pub struct Module {
    pub source_name: Option<String>,
    pub functions: Vec<Function>,
    pub bndata_providers: HashSet<ModuleId>,
    pub bnmath_providers: HashSet<ModuleId>,
    pub filesystem_import: Option<Span>,
}

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<SymbolId>,
    pub return_type: Type,
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub span: Span,
}

#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

struct OpenBlock {
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
}

struct LoopTargets {
    kind: &'static str,
    exit: BlockId,
    continue_at: BlockId,
}

enum AssignPlace {
    Binding {
        symbol: SymbolId,
        indices: Vec<ValueId>,
    },
    Member {
        object: ValueId,
        name: String,
        owner: String,
    },
    Field {
        symbol: SymbolId,
        path: Vec<String>,
    },
    Static {
        class: String,
        field: String,
    },
}

struct Builder<'a> {
    model: &'a SemanticModel,
    methods: HashSet<String>,
    prefix: String,
    blocks: Vec<OpenBlock>,
    current: BlockId,
    next_value: u32,
    loops: Vec<LoopTargets>,
    receiver: Option<(SymbolId, Type)>,
    derived_fields: Option<String>,
}

/// Lowers a semantically valid program to typed BN control-flow IR.
///
/// # Errors
///
/// Returns a source-spanned diagnostic if the AST contains a construct that
/// is not part of the core IR yet or if semantic resolution data is missing.
pub fn lower(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    let functions = lower_program(program, model, "", &collect_methods(program, ""))?;
    let module = Module {
        source_name: program.source_name.clone(),
        functions,
        bndata_providers: HashSet::new(),
        bnmath_providers: HashSet::new(),
        filesystem_import: filesystem_import_span(program),
    };
    validate(&module)?;
    Ok(module)
}

/// Lowers every module in an acyclic graph into one IR module.
///
/// Imported functions are named `#id.name` so two modules may export `Soma`.
///
/// # Errors
///
/// Returns a source-spanned diagnostic if any module cannot be lowered.
pub fn lower_graph(graph: &ModuleGraph, models: &[SemanticModel]) -> Result<Module, Diagnostic> {
    let mut method_names = HashSet::new();
    for loaded in &graph.modules {
        let prefix = module_prefix(graph.root, loaded.id);
        method_names.extend(collect_methods(&loaded.program, &prefix));
    }
    let mut functions = Vec::new();
    for loaded in &graph.modules {
        if loaded.standard_module.is_some() {
            continue;
        }
        let index = usize::try_from(loaded.id.0)
            .map_err(|_| ir_error("module index does not fit", default_span()))?;
        let model = models
            .get(index)
            .ok_or_else(|| ir_error("missing semantic model for module", default_span()))?;
        let prefix = module_prefix(graph.root, loaded.id);
        functions.extend(lower_program(
            &loaded.program,
            model,
            &prefix,
            &method_names,
        )?);
    }
    let root = graph.modules.iter().find(|module| module.id == graph.root);
    let source_name = root.and_then(|module| module.program.source_name.clone());
    let filesystem_import = root.and_then(|module| filesystem_import_span(&module.program));
    let module = Module {
        source_name,
        functions,
        bndata_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNData)).then_some(loaded.id)
            })
            .collect(),
        bnmath_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNMath)).then_some(loaded.id)
            })
            .collect(),
        filesystem_import,
    };
    validate(&module)?;
    Ok(module)
}

fn module_prefix(root: ModuleId, id: ModuleId) -> String {
    if id == root {
        String::new()
    } else {
        format!("#{}.", id.0)
    }
}

fn class_method_name(prefix: &str, class: &str, method: &str) -> String {
    if class.starts_with('#') {
        format!("{class}.{method}")
    } else {
        format!("{prefix}{class}.{method}")
    }
}

fn collect_methods(program: &Program, prefix: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        let Item::Declaration {
            kind,
            name,
            base_class,
            statements,
            ..
        } = item
        else {
            continue;
        };
        match kind {
            DeclarationKind::Class => {
                for statement in statements {
                    if let Statement::MemberFunction { name: method, .. } = statement {
                        names.insert(format!("{prefix}{name}.{method}"));
                    }
                }
                names.insert(format!("{prefix}{name}.$init"));
                names.insert(format!("{prefix}{name}.$fields"));
                if base_class.is_some() {
                    names.insert(format!("{prefix}{name}.CONSTRUCTOR"));
                    names.insert(format!("{prefix}{name}.DESTRUCTOR"));
                }
            }
            DeclarationKind::Struct => {
                names.insert(format!("{prefix}{name}.$default"));
            }
            DeclarationKind::Function => {
                names.insert(format!("{prefix}{name}"));
            }
            DeclarationKind::Interface => {}
        }
    }
    names
}

#[allow(clippy::too_many_lines)] // Class lowering keeps construction order explicit.
fn lower_program(
    program: &Program,
    model: &SemanticModel,
    prefix: &str,
    method_names: &HashSet<String>,
) -> Result<Vec<Function>, Diagnostic> {
    let mut functions = Vec::new();
    for item in &program.items {
        let Item::Declaration {
            kind,
            name: type_name,
            base_class,
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        match kind {
            DeclarationKind::Class => {
                let resolved_base = model.base_classes.get(type_name).cloned();
                if let Some(init) =
                    lower_static_init(model, prefix, type_name, statements, *span, method_names)?
                {
                    functions.push(init);
                }
                functions.push(lower_instance_fields(
                    model,
                    prefix,
                    type_name,
                    statements,
                    *span,
                    method_names,
                )?);
                for statement in statements {
                    let Statement::MemberFunction {
                        name,
                        is_static,
                        parameters,
                        signature,
                        body: Some(body),
                        span,
                        ..
                    } = statement
                    else {
                        continue;
                    };
                    functions.push(lower_callable(
                        model,
                        &format!("{prefix}{type_name}.{name}"),
                        signature.as_ref(),
                        parameters,
                        &body.statements,
                        *span,
                        (!*is_static).then_some(body.span),
                        (name == "CONSTRUCTOR")
                            .then(|| resolved_base.clone())
                            .flatten()
                            .filter(|_| {
                                !body.statements.first().is_some_and(|statement| {
                                    matches!(statement, Statement::Call { expression, .. } if matches!(expression.kind, ExpressionKind::Call { ref callee, .. } if matches!(callee.kind, ExpressionKind::Super)))
                                })
                            }),
                        (name == "DESTRUCTOR")
                            .then(|| resolved_base.clone())
                            .flatten()
                            .filter(|base| method_names.contains(&class_method_name(prefix, base, "DESTRUCTOR"))),
                        method_names.clone(),
                        prefix,
                    )?);
                }
                if base_class.is_some()
                    && !statements.iter().any(|statement| {
                        matches!(statement, Statement::MemberFunction { name, .. } if name == "CONSTRUCTOR")
                    })
                {
                    functions.push(lower_inherited_constructor(
                        prefix,
                        type_name,
                        resolved_base.as_deref().expect("base class exists"),
                        *span,
                        method_names,
                    )?);
                }
                if base_class.is_some()
                    && !statements.iter().any(|statement| {
                        matches!(statement, Statement::MemberFunction { name, .. } if name == "DESTRUCTOR")
                    })
                {
                    functions.push(lower_inherited_destructor(
                        prefix,
                        type_name,
                        resolved_base.as_deref().expect("base class exists"),
                        *span,
                        method_names,
                    )?);
                }
            }
            DeclarationKind::Struct => {
                functions.push(lower_struct_default(
                    model,
                    prefix,
                    type_name,
                    statements,
                    *span,
                    method_names,
                )?);
            }
            _ => {}
        }
    }
    for item in &program.items {
        let Item::Declaration {
            kind: DeclarationKind::Function,
            name,
            signature: Some(signature),
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        functions.push(lower_callable(
            model,
            &format!("{prefix}{name}"),
            Some(signature),
            &signature.parameters,
            statements,
            *span,
            None,
            None,
            None,
            method_names.clone(),
            prefix,
        )?);
    }
    Ok(functions)
}

fn lower_static_init(
    model: &SemanticModel,
    prefix: &str,
    class_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Option<Function>, Diagnostic> {
    let statics: Vec<_> = statements
        .iter()
        .filter(|statement| {
            matches!(
                statement,
                Statement::Binding {
                    is_static: true,
                    ..
                }
            )
        })
        .collect();
    if statics.is_empty() {
        return Ok(None);
    }
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    let class = format!("{prefix}{class_name}");
    for statement in statics {
        let Statement::Binding {
            name,
            initializer,
            type_ref,
            span,
            ..
        } = statement
        else {
            continue;
        };
        let ty = type_at(model, *span).unwrap_or_else(|_| named_or_void(type_ref));
        let value = if let Some(initializer) = initializer {
            builder.expression(initializer)?
        } else {
            builder.default_value(ty.clone(), type_ref, *span)?
        };
        builder.emit(Instruction::StoreStatic {
            class: class.clone(),
            field: name.clone(),
            value,
            ty,
            span: *span,
        });
    }
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    Ok(Some(Function {
        name: format!("{class}.$init"),
        parameters: Vec::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    }))
}

const SYNTHETIC_SELF: SymbolId = {
    // $fields has no AST SELF binding; runtime maps this parameter to the instance.
    SymbolId::from_raw(u32::MAX)
};

fn lower_instance_fields(
    model: &SemanticModel,
    prefix: &str,
    class_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    builder.receiver = Some((SYNTHETIC_SELF, Type::Named(class_name.into())));
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    emit_field_inits(&mut builder, model, statements, receiver, class_name)?;
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    Ok(Function {
        name: format!("{prefix}{class_name}.$fields"),
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

fn lower_inherited_constructor(
    prefix: &str,
    class_name: &str,
    base_class: &str,
    span: Span,
    methods: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let empty_model = SemanticModel {
        symbols: Vec::new(),
        expressions: Vec::new(),
        layouts: std::collections::HashMap::new(),
        base_classes: std::collections::HashMap::new(),
        bnmath_modules: HashSet::new(),
    };
    let mut builder = Builder::new(&empty_model, methods.clone(), prefix);
    builder.derived_fields = Some(format!("{prefix}{class_name}.$fields"));
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    builder.emit_super_construction(base_class, receiver, Vec::new(), span);
    builder.emit_derived_fields(receiver, span);
    builder.terminate(Terminator::Return { value: None });
    Ok(Function {
        name: format!("{prefix}{class_name}.CONSTRUCTOR"),
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

fn lower_inherited_destructor(
    prefix: &str,
    class_name: &str,
    base_class: &str,
    span: Span,
    methods: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let empty_model = SemanticModel {
        symbols: Vec::new(),
        expressions: Vec::new(),
        layouts: std::collections::HashMap::new(),
        base_classes: std::collections::HashMap::new(),
        bnmath_modules: HashSet::new(),
    };
    let mut builder = Builder::new(&empty_model, methods.clone(), prefix);
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    let callee = builder.function_constant(
        &format!(
            "@super:{}",
            class_method_name(prefix, base_class, "DESTRUCTOR")
        ),
        span,
    );
    let destination = builder.value();
    builder.emit(Instruction::Call {
        destination,
        callee,
        arguments: vec![receiver],
        ty: Type::Named("VOID".into()),
        span,
    });
    builder.terminate(Terminator::Return { value: None });
    Ok(Function {
        name: format!("{prefix}{class_name}.DESTRUCTOR"),
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

fn lower_struct_default(
    model: &SemanticModel,
    prefix: &str,
    struct_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    let ty = Type::Named(struct_name.into());
    let record = builder.value();
    builder.emit(Instruction::Default {
        destination: record,
        ty: ty.clone(),
        dimensions: Vec::new(),
        span,
    });
    emit_field_inits(&mut builder, model, statements, record, struct_name)?;
    if !builder.terminated() {
        builder.terminate(Terminator::Return {
            value: Some(record),
        });
    }
    Ok(Function {
        name: format!("{prefix}{struct_name}.$default"),
        parameters: Vec::new(),
        return_type: ty,
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

fn emit_field_inits(
    builder: &mut Builder<'_>,
    model: &SemanticModel,
    statements: &[Statement],
    object: ValueId,
    owner: &str,
) -> Result<(), Diagnostic> {
    for statement in statements {
        let Statement::Binding {
            name,
            initializer,
            type_ref,
            is_static: false,
            span,
            ..
        } = statement
        else {
            continue;
        };
        let ty = type_at(model, *span).unwrap_or_else(|_| named_or_void(type_ref));
        let value = if let Some(initializer) = initializer {
            builder.expression(initializer)?
        } else {
            builder.default_value(ty.clone(), type_ref, *span)?
        };
        builder.emit(Instruction::SetMember {
            object,
            name: name.clone(),
            owner: owner.into(),
            value,
            ty,
            span: *span,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Callable lowering keeps signature, body, and SELF explicit.
fn lower_callable(
    model: &SemanticModel,
    name: &str,
    signature: Option<&FunctionSignature>,
    parameters: &[crate::ast::Parameter],
    statements: &[Statement],
    span: Span,
    self_span: Option<Span>,
    implicit_super: Option<String>,
    parent_destructor: Option<String>,
    methods: HashSet<String>,
    prefix: &str,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, methods, prefix);
    if name.ends_with(".CONSTRUCTOR") {
        builder.derived_fields = Some(format!(
            "{}{}",
            name.trim_end_matches("CONSTRUCTOR"),
            "$fields"
        ));
    }
    if let Some(self_span) = self_span {
        builder.receiver = Some((builder.symbol(self_span)?, type_at(model, self_span)?));
    }
    let explicit_super = statements.first().is_some_and(|statement| {
        matches!(statement, Statement::Call { expression, .. } if matches!(expression.kind, ExpressionKind::Call { ref callee, .. } if matches!(callee.kind, ExpressionKind::Super)))
    });
    if let Some(base) = implicit_super {
        let (receiver, receiver_type) = builder
            .receiver
            .clone()
            .ok_or_else(|| ir_error("implicit SUPER call has no receiver", span))?;
        let receiver = builder.load(receiver, receiver_type, span);
        builder.emit_super_construction(&base, receiver, Vec::new(), span);
        builder.emit_derived_fields(receiver, span);
    } else if name.ends_with(".CONSTRUCTOR")
        && !explicit_super
        && let Some((receiver, receiver_type)) = builder.receiver.clone()
    {
        let receiver = builder.load(receiver, receiver_type, span);
        builder.emit_derived_fields(receiver, span);
    }
    builder.statements(statements)?;
    if let Some(base) = parent_destructor
        && !builder.terminated()
    {
        let (receiver, receiver_type) = builder
            .receiver
            .clone()
            .ok_or_else(|| ir_error("destructor has no receiver", span))?;
        let receiver = builder.load(receiver, receiver_type, span);
        let callee = builder.function_constant(
            &format!("@super:{}", class_method_name(prefix, &base, "DESTRUCTOR")),
            span,
        );
        let unused = builder.value();
        builder.emit(Instruction::Call {
            destination: unused,
            callee,
            arguments: vec![receiver],
            ty: Type::Named("VOID".into()),
            span,
        });
    }
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    let mut parameter_ids = Vec::new();
    if let Some((receiver, _)) = &builder.receiver {
        parameter_ids.push(*receiver);
    }
    for parameter in parameters {
        parameter_ids.push(builder.symbol(parameter.span)?);
    }
    let return_type = if let Some(signature) = signature {
        type_at(model, signature.return_type.span)
            .unwrap_or_else(|_| named_or_void(&signature.return_type))
    } else {
        Type::Named("VOID".into())
    };
    Ok(Function {
        name: name.into(),
        parameters: parameter_ids,
        return_type,
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

/// Checks structural invariants required by every BN IR consumer.
///
/// # Errors
///
/// Returns `INVALID_IR` if an entry point or terminator references a block that
/// does not exist.
pub fn validate(module: &Module) -> Result<(), Diagnostic> {
    for function in &module.functions {
        let block_count = u32::try_from(function.blocks.len())
            .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?;
        if function.entry.0 >= block_count {
            return Err(invalid_ir(
                "function entry block does not exist",
                function.span,
            ));
        }
        let mut defined = HashSet::new();
        for (index, block) in function.blocks.iter().enumerate() {
            if block.id.0
                != u32::try_from(index)
                    .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?
            {
                return Err(invalid_ir(
                    "basic block IDs must be dense and ordered",
                    function.span,
                ));
            }
            for instruction in &block.instructions {
                for used in instruction_uses(instruction) {
                    if !defined.contains(&used)
                        && !instruction_defines(instruction)
                            .is_some_and(|defined_id| defined_id == used)
                    {
                        return Err(invalid_ir(
                            "instruction uses a value that is not defined",
                            function.span,
                        ));
                    }
                }
                if let Some(destination) = instruction_defines(instruction) {
                    defined.insert(destination);
                }
            }
            match &block.terminator {
                Terminator::Jump { target } if target.0 >= block_count => {
                    return Err(invalid_ir(
                        "terminator references a basic block that does not exist",
                        function.span,
                    ));
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                    condition,
                } => {
                    if then_block.0 >= block_count || else_block.0 >= block_count {
                        return Err(invalid_ir(
                            "terminator references a basic block that does not exist",
                            function.span,
                        ));
                    }
                    if !defined.contains(condition) {
                        return Err(invalid_ir("branch condition is not defined", function.span));
                    }
                }
                Terminator::Return { value: Some(value) } | Terminator::Stop { code: value }
                    if !defined.contains(value) =>
                {
                    return Err(invalid_ir("terminator value is not defined", function.span));
                }
                Terminator::Jump { .. } | Terminator::Return { .. } | Terminator::Stop { .. } => {}
            }
        }
    }
    Ok(())
}

impl<'a> Builder<'a> {
    fn new(model: &'a SemanticModel, methods: HashSet<String>, prefix: &str) -> Self {
        Self {
            model,
            methods,
            prefix: prefix.into(),
            blocks: vec![OpenBlock {
                instructions: Vec::new(),
                terminator: None,
            }],
            current: BlockId(0),
            next_value: 0,
            loops: Vec::new(),
            receiver: None,
            derived_fields: None,
        }
    }

    fn emit_void_call(&mut self, name: &str, arguments: Vec<ValueId>, span: Span) {
        if !self.methods.contains(name) {
            return;
        }
        let unused = self.value();
        let callee = self.function_constant(name, span);
        self.emit(Instruction::Call {
            destination: unused,
            callee,
            arguments,
            ty: Type::Named("VOID".into()),
            span,
        });
    }

    fn emit_super_construction(
        &mut self,
        base: &str,
        receiver: ValueId,
        constructor_arguments: Vec<ValueId>,
        span: Span,
    ) {
        self.emit_void_call(
            &class_method_name(&self.prefix, base, "$fields"),
            vec![receiver],
            span,
        );
        let constructor = class_method_name(&self.prefix, base, "CONSTRUCTOR");
        if self.methods.contains(&constructor) {
            let mut arguments = vec![receiver];
            arguments.extend(constructor_arguments);
            self.emit_void_call(&constructor, arguments, span);
        }
    }

    fn emit_derived_fields(&mut self, receiver: ValueId, span: Span) {
        if let Some(fields) = self.derived_fields.take() {
            self.emit_void_call(&fields, vec![receiver], span);
        }
    }

    fn statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.terminated() {
                break;
            }
            self.statement(statement)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // IR cases mirror the statement AST.
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Binding {
                initializer,
                type_ref,
                span,
                ..
            } => {
                let ty = type_at(self.model, *span)?;
                let value = if let Some(initializer) = initializer {
                    self.expression(initializer)?
                } else {
                    self.default_value(ty.clone(), type_ref, *span)?
                };
                self.emit(Instruction::Store {
                    symbol: self.symbol(*span)?,
                    value,
                    ty,
                    span: *span,
                });
            }
            Statement::Assignment {
                target,
                operator,
                value,
                span,
            } => {
                let mut result = self.expression(value)?;
                if operator != "Assign" {
                    let left = self.expression(target)?;
                    let destination = self.value();
                    self.emit(Instruction::Binary {
                        destination,
                        operator: assignment_operator(operator)?.into(),
                        left,
                        right: result,
                        ty: type_at(self.model, target.span)?,
                        span: *span,
                    });
                    result = destination;
                }
                match self.assignment_place(target)? {
                    AssignPlace::Binding { symbol, indices } if indices.is_empty() => {
                        self.emit(Instruction::Store {
                            symbol,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Binding { symbol, indices } => {
                        self.emit(Instruction::SetIndex {
                            symbol,
                            indices,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Member {
                        object,
                        name,
                        owner,
                    } => {
                        self.emit(Instruction::SetMember {
                            object,
                            name,
                            owner,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Field { symbol, path } => {
                        self.emit(Instruction::SetField {
                            symbol,
                            path,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Static { class, field } => {
                        self.ensure_class(&class, *span);
                        self.emit(Instruction::StoreStatic {
                            class,
                            field,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                }
            }
            Statement::Print { values, span } => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit(Instruction::Print {
                    values,
                    span: *span,
                });
            }
            Statement::ClearScreen { console, span } => {
                let console = self.expression(console)?;
                self.emit(Instruction::ClearScreen {
                    console,
                    span: *span,
                });
            }
            Statement::Beep { console, span } => {
                let console = self.expression(console)?;
                self.emit(Instruction::Beep {
                    console,
                    span: *span,
                });
            }
            Statement::Call { expression, .. } => {
                self.expression(expression)?;
            }
            Statement::Return { value, .. } => {
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                self.terminate(Terminator::Return { value });
            }
            Statement::Stop { code, .. } => {
                let code = self.expression(code)?;
                self.terminate(Terminator::Stop { code });
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.if_statement(branches, otherwise.as_ref())?,
            Statement::While {
                condition, body, ..
            } => self.while_statement(condition, body)?,
            Statement::Repeat {
                body, condition, ..
            } => self.repeat_statement(body, condition)?,
            Statement::For { header, body, span } => self.for_statement(header, body, *span)?,
            Statement::Control { kind, target, span } => {
                let targets = self
                    .loops
                    .iter()
                    .rev()
                    .find(|targets| targets.kind == target)
                    .ok_or_else(|| ir_error("loop target is missing", *span))?;
                let destination = if kind == "EXIT" {
                    targets.exit
                } else {
                    targets.continue_at
                };
                self.terminate(Terminator::Jump {
                    target: destination,
                });
            }
            Statement::Delete { value, span } => {
                let deleted = self.expression(value)?;
                let destructor =
                    destructor_name(self.model, value.span, &self.methods, &self.prefix);
                self.emit(Instruction::Delete {
                    value: deleted,
                    destructor,
                    span: *span,
                });
            }
            Statement::MemberFunction { .. } => {}
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Expression alternatives mirror the syntax AST.
    fn expression(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let ty = type_at(self.model, expression.span)?;
        if let ExpressionKind::New {
            type_name,
            arguments,
        } = &expression.kind
        {
            return self.allocate(type_name, arguments, ty, expression.span);
        }
        let destination = self.value();
        let instruction = match &expression.kind {
            ExpressionKind::Super => unreachable!("semantic analysis rejects bare SUPER"),
            ExpressionKind::Literal(literal) => Instruction::Constant {
                destination,
                value: constant(literal),
                ty,
                span: expression.span,
            },
            ExpressionKind::Name { name } => {
                if name == "SELF" {
                    if let Some((symbol, receiver_type)) = self.receiver.clone() {
                        Instruction::Load {
                            destination,
                            symbol,
                            ty: receiver_type,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Load {
                            destination,
                            symbol: self.expression_symbol(expression)?,
                            ty,
                            span: expression.span,
                        }
                    }
                } else if matches!(ty, Type::Function { .. }) {
                    let qualified = format!("{}{name}", self.prefix);
                    if self.methods.contains(&qualified) {
                        Instruction::Constant {
                            destination,
                            value: Constant::Function(qualified),
                            ty,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Load {
                            destination,
                            symbol: self.expression_symbol(expression)?,
                            ty,
                            span: expression.span,
                        }
                    }
                } else if ty == Type::HostConsole {
                    Instruction::Constant {
                        destination,
                        value: Constant::HostConsole,
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostArgs {
                    Instruction::Constant {
                        destination,
                        value: Constant::HostArgs,
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostClock {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.Clock".into()),
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostRandom {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.Random".into()),
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostFileSystem {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.FileSystem".into()),
                        ty,
                        span: expression.span,
                    }
                } else if matches!(ty, Type::TypeName(_) | Type::Module(_)) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type(name.clone()),
                        ty,
                        span: expression.span,
                    }
                } else {
                    Instruction::Load {
                        destination,
                        symbol: self.expression_symbol(expression)?,
                        ty,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::Input => Instruction::Input {
                destination,
                ty,
                span: expression.span,
            },
            ExpressionKind::HostCapability { name } => Instruction::Constant {
                destination,
                value: host_capability_constant(name, expression.span)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Length { operand } => {
                let operand_type = if matches!(operand.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    Type::HostArgs
                } else {
                    type_at(self.model, operand.span)?
                };
                let value = if operand_type == Type::HostArgs {
                    let value = self.value();
                    self.emit(Instruction::Constant {
                        destination: value,
                        value: Constant::HostArgs,
                        ty: Type::HostArgs,
                        span: operand.span,
                    });
                    value
                } else {
                    self.expression(operand)?
                };
                if let Some(length) = static_len(&operand_type) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Integer(length.to_string()),
                        ty: Type::Integer(crate::semantic::IntegerType::Int32),
                        span: expression.span,
                    }
                } else {
                    Instruction::Length {
                        destination,
                        vector: value,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::SizeOf { operand } => {
                let operand_type = type_at(self.model, operand.span)?;
                let value = self.expression(operand)?;
                if let Some(size) = self.model.size_of(&operand_type) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Integer(size.to_string()),
                        ty: Type::Integer(crate::semantic::IntegerType::Int32),
                        span: expression.span,
                    }
                } else {
                    Instruction::SizeOf {
                        destination,
                        value,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::TypeTest { .. } => {
                return Err(ir_error(
                    "type tests are lowered only as the right operand of IS",
                    expression.span,
                ));
            }
            ExpressionKind::Unary { operator, operand } => Instruction::Unary {
                destination,
                operator: operator.clone(),
                operand: self.expression(operand)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                if matches!(operator.as_str(), "AND" | "OR") && ty == Type::Boolean {
                    return self.short_circuit(destination, operator, left, right, expression.span);
                }
                let left = self.expression(left)?;
                let right = if operator == "IS" {
                    self.type_test(right)?
                } else {
                    self.expression(right)?
                };
                Instruction::Binary {
                    destination,
                    operator: operator.clone(),
                    left,
                    right,
                    ty,
                    span: expression.span,
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                if matches!(callee.kind, ExpressionKind::Super) {
                    let owner = self
                        .model
                        .expression(expression.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .ok_or_else(|| {
                            ir_error("SUPER call has no resolved base class", expression.span)
                        })?;
                    let (receiver, receiver_type) = self
                        .receiver
                        .clone()
                        .ok_or_else(|| ir_error("SUPER call has no receiver", expression.span))?;
                    let receiver = self.load(receiver, receiver_type, expression.span);
                    let mut values = Vec::new();
                    for argument in arguments {
                        values.push(self.expression(argument)?);
                    }
                    self.emit_super_construction(&owner, receiver, values, expression.span);
                    self.emit_derived_fields(receiver, expression.span);
                    Instruction::Constant {
                        destination,
                        value: Constant::Null,
                        ty,
                        span: expression.span,
                    }
                } else {
                    let (callee, arguments) = self.call_operands(callee, arguments)?;
                    Instruction::Call {
                        destination,
                        callee,
                        arguments,
                        ty,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::Cast { value, .. } => Instruction::Cast {
                destination,
                value: self.expression(value)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Vector { values } => Instruction::Vector {
                destination,
                values: values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Index { object, index } => {
                let object = if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    let value = self.value();
                    self.emit(Instruction::Constant {
                        destination: value,
                        value: Constant::HostArgs,
                        ty: Type::HostArgs,
                        span: object.span,
                    });
                    value
                } else {
                    self.expression(object)?
                };
                Instruction::Index {
                    destination,
                    object,
                    index: self.expression(index)?,
                    ty,
                    span: expression.span,
                }
            }
            ExpressionKind::Member { object, name } => {
                let object_type = type_at(self.model, object.span)?;
                if matches!(object_type, Type::Module(module) if self.model.bnmath_modules.contains(&module))
                    && let Some(value) = math_constant(name)
                {
                    let _ = self.expression(object)?;
                    Instruction::Constant {
                        destination,
                        value,
                        ty,
                        span: expression.span,
                    }
                } else if matches!(object_type, Type::HostFileSystem)
                    && let Some(value) = filesystem_constant(name)
                {
                    let _ = self.expression(object)?;
                    Instruction::Constant {
                        destination,
                        value,
                        ty,
                        span: expression.span,
                    }
                } else {
                    let owner = self
                        .model
                        .expression(expression.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .unwrap_or_default();
                    if matches!(ty, Type::Function { .. })
                        && let Some(function) = namespace_function(&object_type, name, &self.prefix)
                    {
                        let function =
                            if matches!(object_type, Type::TypeName(_)) && !owner.is_empty() {
                                format!("{}{}.{}", self.prefix, owner, name)
                            } else {
                                function
                            };
                        let _ = self.expression(object)?;
                        if user_class_name(&object_type).is_some() {
                            let class =
                                if owner.is_empty() || !matches!(object_type, Type::TypeName(_)) {
                                    static_class_name(&object_type, &self.prefix)
                                } else {
                                    format!("{}{}", self.prefix, owner)
                                };
                            self.ensure_class(&class, expression.span);
                        }
                        Instruction::Constant {
                            destination,
                            value: Constant::Function(function),
                            ty,
                            span: expression.span,
                        }
                    } else if matches!(ty, Type::TypeName(_) | Type::ImportedTypeName { .. }) {
                        let _ = self.expression(object)?;
                        Instruction::Constant {
                            destination,
                            value: Constant::Type(name.clone()),
                            ty,
                            span: expression.span,
                        }
                    } else if is_namespace_type(&object_type) {
                        let class = if owner.is_empty() || !matches!(object_type, Type::TypeName(_))
                        {
                            static_class_name(&object_type, &self.prefix)
                        } else {
                            format!("{}{}", self.prefix, owner)
                        };
                        self.ensure_class(&class, expression.span);
                        let _ = self.expression(object)?;
                        Instruction::LoadStatic {
                            destination,
                            class,
                            field: name.clone(),
                            ty,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Member {
                            destination,
                            object: self.expression(object)?,
                            name: name.clone(),
                            owner,
                            ty,
                            span: expression.span,
                        }
                    }
                }
            }
            ExpressionKind::New { .. } => {
                return Err(ir_error(
                    "NEW is lowered as an allocation sequence",
                    expression.span,
                ));
            }
        };
        self.emit(instruction);
        Ok(destination)
    }

    fn short_circuit(
        &mut self,
        destination: ValueId,
        operator: &str,
        left: &Expression,
        right: &Expression,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let left = self.expression(left)?;
        let right_block = self.block();
        let short_block = self.block();
        let join = self.block();
        let (then_block, else_block, short_value) = if operator == "AND" {
            (right_block, short_block, false)
        } else {
            (short_block, right_block, true)
        };
        self.terminate(Terminator::Branch {
            condition: left,
            then_block,
            else_block,
        });
        self.current = short_block;
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Boolean(short_value),
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = right_block;
        let right = self.expression(right)?;
        self.emit(Instruction::Copy {
            destination,
            source: right,
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = join;
        Ok(destination)
    }

    fn type_test(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let value = match &expression.kind {
            ExpressionKind::TypeTest { type_ref } => type_ref
                .alternatives
                .first()
                .map(type_test_name)
                .ok_or_else(|| ir_error("invalid IS type test", expression.span))?,
            ExpressionKind::Name { name } | ExpressionKind::Literal(Literal::TypeName(name)) => {
                name.clone()
            }
            ExpressionKind::Literal(Literal::Null) => "NULL".into(),
            ExpressionKind::Literal(Literal::NotAvailable) => "NA".into(),
            ExpressionKind::Literal(Literal::EndOfFile) => "EOF".into(),
            ExpressionKind::Literal(Literal::Special(value)) => value.clone(),
            ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
                let ExpressionKind::Literal(Literal::Special(value)) = &operand.kind else {
                    return Err(ir_error("invalid IS test", expression.span));
                };
                format!("-{value}")
            }
            _ => return Err(ir_error("invalid IS test", expression.span)),
        };
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Type(value.clone()),
            ty: Type::TypeName(value.clone()),
            span: expression.span,
        });
        Ok(destination)
    }

    fn if_statement(
        &mut self,
        branches: &[crate::ast::IfBranch],
        otherwise: Option<&AstBlock>,
    ) -> Result<(), Diagnostic> {
        let join = self.block();
        for branch in branches {
            let body = self.block();
            let next = self.block();
            let condition = self.expression(&branch.condition)?;
            self.terminate(Terminator::Branch {
                condition,
                then_block: body,
                else_block: next,
            });
            self.current = body;
            self.statements(&branch.body.statements)?;
            self.jump_if_open(join);
            self.current = next;
        }
        if let Some(otherwise) = otherwise {
            self.statements(&otherwise.statements)?;
        }
        self.jump_if_open(join);
        self.current = join;
        Ok(())
    }

    fn while_statement(
        &mut self,
        condition: &Expression,
        body: &AstBlock,
    ) -> Result<(), Diagnostic> {
        let condition_block = self.block();
        let body_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump {
            target: condition_block,
        });
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: body_block,
            else_block: exit,
        });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "WHILE",
            exit,
            continue_at: condition_block,
        });
        self.statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = exit;
        Ok(())
    }

    fn repeat_statement(
        &mut self,
        body: &AstBlock,
        condition: &Expression,
    ) -> Result<(), Diagnostic> {
        let body_block = self.block();
        let condition_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump { target: body_block });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "REPEAT",
            exit,
            continue_at: condition_block,
        });
        self.statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: exit,
            else_block: body_block,
        });
        self.current = exit;
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Counted and FOR EACH lowering share loop target handling.
    fn for_statement(
        &mut self,
        header: &ForHeader,
        body: &AstBlock,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match header {
            ForHeader::Counted {
                type_ref,
                start,
                end,
                step,
                ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let start = self.expression(start)?;
                let end = self.expression(end)?;
                let step = if let Some(step) = step {
                    self.expression(step)?
                } else {
                    self.integer_one(type_at(self.model, type_ref.span)?, span)
                };
                self.emit(Instruction::Store {
                    symbol,
                    value: start,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                let condition_block = self.block();
                let body_block = self.block();
                let next_block = self.block();
                let exit = self.block();
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = condition_block;
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
                let condition = self.value();
                let for_condition = self.function_constant("$for_condition", span);
                self.emit(Instruction::Call {
                    destination: condition,
                    callee: for_condition,
                    arguments: vec![current, end, step],
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
                let next = self.value();
                self.emit(Instruction::Binary {
                    destination: next,
                    operator: "Plus".into(),
                    left: current,
                    right: step,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: next,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
            ForHeader::Each {
                type_ref, iterable, ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let vector = self.expression(iterable)?;
                let index = self.value();
                self.emit(Instruction::Constant {
                    destination: index,
                    value: Constant::Integer("0".into()),
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                let length = self.value();
                self.emit(Instruction::Length {
                    destination: length,
                    vector,
                    span,
                });
                let condition_block = self.block();
                let body_block = self.block();
                let next_block = self.block();
                let exit = self.block();
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = condition_block;
                let condition = self.value();
                self.emit(Instruction::Binary {
                    destination: condition,
                    operator: "Less".into(),
                    left: index,
                    right: length,
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                let element = self.value();
                self.emit(Instruction::Index {
                    destination: element,
                    object: vector,
                    index,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: element,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let one =
                    self.integer_one(Type::Integer(crate::semantic::IntegerType::Int32), span);
                self.emit(Instruction::Binary {
                    destination: index,
                    operator: "Plus".into(),
                    left: index,
                    right: one,
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
        }
        Ok(())
    }

    fn integer_one(&mut self, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Integer("1".into()),
            ty,
            span,
        });
        destination
    }

    fn default_value(
        &mut self,
        ty: Type,
        type_ref: &TypeReference,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        if matches!(ty, Type::Pointer { .. } | Type::Function { .. }) {
            return Err(ir_error(
                "pointer and function bindings require an initializer",
                span,
            ));
        }
        if let Some(name) = self.struct_default_name(&ty) {
            let destination = self.value();
            let callee = self.function_constant(&name, span);
            self.emit(Instruction::Call {
                destination,
                callee,
                arguments: Vec::new(),
                ty,
                span,
            });
            return Ok(destination);
        }
        let destination = self.value();
        let dimensions = type_ref
            .alternatives
            .first()
            .map(|atom| {
                atom.parts
                    .windows(3)
                    .filter(|parts| parts[0] == "LeftBracket" && parts[2] == "RightBracket")
                    .map(|parts| {
                        parts[1]
                            .parse::<usize>()
                            .map_err(|_| ir_error("invalid vector dimension", span))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        self.emit(Instruction::Default {
            destination,
            ty,
            dimensions,
            span,
        });
        Ok(destination)
    }

    fn ensure_class(&mut self, class: &str, span: Span) {
        self.emit(Instruction::EnsureClass {
            class: class.into(),
            span,
        });
    }

    fn function_constant(&mut self, name: &str, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Function(name.into()),
            ty: Type::Unknown,
            span,
        });
        destination
    }

    fn load(&mut self, symbol: SymbolId, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Load {
            destination,
            symbol,
            ty,
            span,
        });
        destination
    }

    fn expression_symbol(&self, expression: &Expression) -> Result<SymbolId, Diagnostic> {
        self.model
            .expression(expression.span)
            .and_then(|expression| expression.symbol_id)
            .ok_or_else(|| ir_error("expression has no resolved SymbolId", expression.span))
    }

    fn assignment_place(&mut self, expression: &Expression) -> Result<AssignPlace, Diagnostic> {
        let mut indices = Vec::new();
        let mut current = expression;
        while let ExpressionKind::Index { object, index } = &current.kind {
            indices.push(self.expression(index)?);
            current = object;
        }
        indices.reverse();
        match &current.kind {
            ExpressionKind::Name { .. } => Ok(AssignPlace::Binding {
                symbol: self.expression_symbol(current)?,
                indices,
            }),
            ExpressionKind::Member { object, name } if indices.is_empty() => {
                let mut path = vec![name.clone()];
                let mut base = object.as_ref();
                while let ExpressionKind::Member {
                    object: nested,
                    name: nested_name,
                } = &base.kind
                {
                    path.push(nested_name.clone());
                    base = nested;
                }
                path.reverse();
                if let ExpressionKind::Name { .. } = &base.kind {
                    let object_type = type_at(self.model, base.span)?;
                    if is_namespace_type(&object_type) {
                        let class = self
                            .model
                            .expression(current.span)
                            .and_then(|resolved| resolved.member_target.as_ref())
                            .and_then(|target| target.owner.clone())
                            .map_or_else(
                                || static_class_name(&object_type, &self.prefix),
                                |owner| format!("{}{}", self.prefix, owner),
                            );
                        return Ok(AssignPlace::Static {
                            class,
                            field: path.last().cloned().unwrap_or_default(),
                        });
                    }
                    return Ok(AssignPlace::Field {
                        symbol: self.expression_symbol(base)?,
                        path,
                    });
                }
                let object_type = type_at(self.model, object.span)?;
                if is_namespace_type(&object_type) {
                    let class = self
                        .model
                        .expression(current.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .map_or_else(
                            || static_class_name(&object_type, &self.prefix),
                            |owner| format!("{}{}", self.prefix, owner),
                        );
                    Ok(AssignPlace::Static {
                        class,
                        field: name.clone(),
                    })
                } else {
                    let owner = self
                        .model
                        .expression(current.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .unwrap_or_default();
                    Ok(AssignPlace::Member {
                        object: self.expression(object)?,
                        name: name.clone(),
                        owner,
                    })
                }
            }
            _ => Err(ir_error(
                "assignment target is not a binding, index, or member",
                expression.span,
            )),
        }
    }

    fn struct_default_name(&self, ty: &Type) -> Option<String> {
        let name = match ty {
            Type::Named(name) => format!("{}{name}.$default", self.prefix),
            Type::ImportedNamed { module, name } => format!("#{}.{name}.$default", module.0),
            _ => return None,
        };
        self.methods.contains(&name).then_some(name)
    }

    fn call_operands(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) -> Result<(ValueId, Vec<ValueId>), Diagnostic> {
        if let ExpressionKind::Name { name } = &callee.kind
            && matches!(name.as_str(), "ASC" | "CHAR")
        {
            let values = arguments
                .iter()
                .map(|argument| self.expression(argument))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok((self.function_constant(name, callee.span), values));
        }
        if let ExpressionKind::Member { object, name } = &callee.kind {
            if matches!(object.kind, ExpressionKind::Super) {
                let owner = self
                    .model
                    .expression(callee.span)
                    .and_then(|resolved| resolved.member_target.as_ref())
                    .and_then(|target| target.owner.clone())
                    .ok_or_else(|| {
                        ir_error("SUPER member has no resolved base class", callee.span)
                    })?;
                let (receiver, receiver_type) = self
                    .receiver
                    .clone()
                    .ok_or_else(|| ir_error("SUPER member has no receiver", callee.span))?;
                let receiver = self.load(receiver, receiver_type, callee.span);
                let mut values = vec![receiver];
                for argument in arguments {
                    values.push(self.expression(argument)?);
                }
                return Ok((
                    self.function_constant(
                        &format!("@super:{}{}.{}", self.prefix, owner, name),
                        callee.span,
                    ),
                    values,
                ));
            }
            let object_type = type_at(self.model, object.span)?;
            let member_type = type_at(self.model, callee.span)?;
            if matches!(member_type, Type::Function { .. }) && !is_namespace_type(&object_type) {
                let receiver = self.expression(object)?;
                let owner = self
                    .model
                    .expression(callee.span)
                    .and_then(|resolved| resolved.member_target.as_ref())
                    .and_then(|target| target.owner.clone())
                    .unwrap_or_else(|| display_type(&object_type));
                let qualified = match &object_type {
                    Type::ImportedNamed {
                        module,
                        name: class,
                    } => format!("#{}.{class}.{name}", module.0),
                    _ => format!("{}{owner}.{name}", self.prefix),
                };
                let callee = self.function_constant(&qualified, callee.span);
                let mut values = vec![receiver];
                for argument in arguments {
                    values.push(self.expression(argument)?);
                }
                return Ok((callee, values));
            }
        }
        let callee = self.expression(callee)?;
        let mut values = Vec::new();
        for argument in arguments {
            values.push(self.expression(argument)?);
        }
        Ok((callee, values))
    }

    fn allocate(
        &mut self,
        type_name: &str,
        arguments: &[Expression],
        ty: Type,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let arguments = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let destination = self.value();
        let class = class_ir_name(&ty, type_name, &self.prefix);
        if !is_numeric_type_name(type_name) {
            self.ensure_class(&class, span);
        }
        self.emit(Instruction::Allocate {
            destination,
            type_name: class.clone(),
            arguments: arguments.clone(),
            ty,
            span,
        });
        let constructor = format!("{class}.CONSTRUCTOR");
        if self.methods.contains(&constructor) {
            let callee = self.function_constant(&constructor, span);
            let mut constructor_arguments = vec![destination];
            constructor_arguments.extend(arguments);
            let unused = self.value();
            self.emit(Instruction::Call {
                destination: unused,
                callee,
                arguments: constructor_arguments,
                ty: Type::Named("VOID".into()),
                span,
            });
        } else {
            let fields = format!("{class}.$fields");
            if self.methods.contains(&fields) {
                let callee = self.function_constant(&fields, span);
                let unused = self.value();
                self.emit(Instruction::Call {
                    destination: unused,
                    callee,
                    arguments: vec![destination],
                    ty: Type::Named("VOID".into()),
                    span,
                });
            }
        }
        Ok(destination)
    }

    fn symbol(&self, span: Span) -> Result<SymbolId, Diagnostic> {
        self.model
            .symbol_at(span)
            .map(|symbol| symbol.id)
            .ok_or_else(|| ir_error("declaration has no resolved SymbolId", span))
    }

    fn block(&mut self) -> BlockId {
        let id = BlockId(u32::try_from(self.blocks.len()).expect("IR block count fits u32"));
        self.blocks.push(OpenBlock {
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn emit(&mut self, instruction: Instruction) {
        self.blocks[self.current.0 as usize]
            .instructions
            .push(instruction);
    }

    fn terminate(&mut self, terminator: Terminator) {
        self.blocks[self.current.0 as usize].terminator = Some(terminator);
    }

    fn terminated(&self) -> bool {
        self.blocks[self.current.0 as usize].terminator.is_some()
    }

    fn jump_if_open(&mut self, target: BlockId) {
        if !self.terminated() {
            self.terminate(Terminator::Jump { target });
        }
    }

    fn finish(self) -> Result<Vec<BasicBlock>, Diagnostic> {
        self.blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                Ok(BasicBlock {
                    id: BlockId(u32::try_from(index).expect("IR block count fits u32")),
                    instructions: block.instructions,
                    terminator: block.terminator.ok_or_else(|| {
                        ir_error("generated basic block has no terminator", default_span())
                    })?,
                })
            })
            .collect()
    }
}

fn type_at(model: &SemanticModel, span: Span) -> Result<Type, Diagnostic> {
    model
        .expression(span)
        .map(|expression| expression.ty.clone())
        .or_else(|| model.symbol_at(span).map(|symbol| symbol.ty.clone()))
        .ok_or_else(|| ir_error("type information is missing for IR lowering", span))
}

fn host_capability_constant(name: &str, span: Span) -> Result<Constant, Diagnostic> {
    match name {
        "Args" => Ok(Constant::HostArgs),
        "Console" => Ok(Constant::HostConsole),
        "Clock" => Ok(Constant::Type("HOST.Clock".into())),
        _ => Err(ir_error(format!("HOST.{name} cannot be lowered"), span)),
    }
}

fn constant(literal: &Literal) -> Constant {
    match literal {
        Literal::Integer(value) => Constant::Integer(value.clone()),
        Literal::Float(value) | Literal::Special(value) => Constant::Float(value.clone()),
        Literal::String(value) => Constant::String(value.clone()),
        Literal::TypeName(value) => Constant::Type(value.clone()),
        Literal::Boolean(value) => Constant::Boolean(*value),
        Literal::Null => Constant::Null,
        Literal::NotAvailable => Constant::NotAvailable,
        Literal::EndOfFile => Constant::EndOfFile,
    }
}

fn assignment_operator(operator: &str) -> Result<&'static str, Diagnostic> {
    match operator {
        "PlusAssign" => Ok("Plus"),
        "MinusAssign" => Ok("Minus"),
        "StarAssign" => Ok("Star"),
        "SlashAssign" => Ok("Slash"),
        "PercentAssign" => Ok("Percent"),
        "PowerAssign" => Ok("Power"),
        _ => Err(ir_error("unknown assignment operator", default_span())),
    }
}

fn invalid_ir(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code: "INVALID_IR",
        message: message.into(),
        span,
    }
}

fn ir_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code: "IR_LOWERING",
        message: message.into(),
        span,
    }
}

fn type_test_name(atom: &TypeAtom) -> String {
    let dotted = atom.parts.iter().any(|part| part == "." || part == "Dot");
    let names: Vec<&str> = std::iter::once(atom.name.as_str())
        .chain(
            atom.parts
                .iter()
                .filter(|part| *part != "." && *part != "Dot")
                .map(String::as_str),
        )
        .collect();
    if dotted {
        names.join(".")
    } else {
        names.join(" ")
    }
}

fn named_or_void(reference: &TypeReference) -> Type {
    let name = reference
        .alternatives
        .first()
        .map_or("VOID", |atom| atom.name.as_str());
    match name {
        "INTEGER" | "INT32" => Type::Integer(IntegerType::Int32),
        "INT8" => Type::Integer(IntegerType::Int8),
        "INT16" => Type::Integer(IntegerType::Int16),
        "INT64" | "TIMESTAMP" => Type::Integer(IntegerType::Int64),
        "BYTE" => Type::Integer(IntegerType::Byte),
        "UINT16" => Type::Integer(IntegerType::UInt16),
        "UINT32" => Type::Integer(IntegerType::UInt32),
        "UINT64" => Type::Integer(IntegerType::UInt64),
        "FLOAT" | "FLOAT64" => Type::Float(crate::semantic::FloatType::Float64),
        "FLOAT32" => Type::Float(crate::semantic::FloatType::Float32),
        "BOOLEAN" => Type::Boolean,
        "STRING" => Type::String,
        "VOID" => Type::Named("VOID".into()),
        other => Type::Named(other.into()),
    }
}

fn is_namespace_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::TypeName(_)
            | Type::ImportedTypeName { .. }
            | Type::HostClock
            | Type::HostRandom
            | Type::HostConsole
            | Type::HostFileSystem
            | Type::Module(_)
    )
}

fn math_constant(name: &str) -> Option<Constant> {
    let value = match name {
        "MAX_INT8" => Constant::Integer("127".into()),
        "MIN_INT8" => Constant::Integer("-128".into()),
        "MAX_INT16" => Constant::Integer("32767".into()),
        "MIN_INT16" => Constant::Integer("-32768".into()),
        "MAX_INTEGER" | "MAX_INT32" => Constant::Integer("2147483647".into()),
        "MIN_INTEGER" | "MIN_INT32" => Constant::Integer("-2147483648".into()),
        "MAX_INT64" | "MAX_TIMESTAMP" => Constant::Integer("9223372036854775807".into()),
        "MIN_INT64" | "MIN_TIMESTAMP" => Constant::Integer("-9223372036854775808".into()),
        "MAX_BYTE" | "MAX_UINT16" => {
            Constant::Integer(if name == "MAX_BYTE" { "255" } else { "65535" }.into())
        }
        "MIN_BYTE" | "MIN_UINT16" | "MIN_UINT32" | "MIN_UINT64" => Constant::Integer("0".into()),
        "MAX_UINT32" => Constant::Integer("4294967295".into()),
        "MAX_UINT64" => Constant::Integer("18446744073709551615".into()),
        "MAX_FLOAT32" => Constant::Float("3.4028234663852886e38".into()),
        "MIN_FLOAT32" => Constant::Float("-3.4028234663852886e38".into()),
        "MIN_POSITIVE_FLOAT32" => Constant::Float("1.1754943508222875e-38".into()),
        "MAX_FLOAT" | "MAX_FLOAT64" => Constant::Float("1.7976931348623157e308".into()),
        "MIN_FLOAT" | "MIN_FLOAT64" => Constant::Float("-1.7976931348623157e308".into()),
        "MIN_POSITIVE_FLOAT" | "MIN_POSITIVE_FLOAT64" => {
            Constant::Float("2.2250738585072014e-308".into())
        }
        _ => return None,
    };
    Some(value)
}

fn filesystem_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "FileSystem" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

fn filesystem_constant(name: &str) -> Option<Constant> {
    Some(Constant::Integer(
        match name {
            "READ" => "0",
            "WRITE" => "1",
            "APPEND" => "2",
            _ => return None,
        }
        .into(),
    ))
}

fn namespace_function(object_type: &Type, name: &str, prefix: &str) -> Option<String> {
    match object_type {
        Type::TypeName(owner) if user_class_name(object_type).is_some() => {
            Some(format!("{prefix}{owner}.{name}"))
        }
        Type::TypeName(owner) => Some(format!("{owner}.{name}")),
        Type::ImportedTypeName {
            module,
            name: owner,
        } => Some(format!("#{}.{owner}.{name}", module.0)),
        Type::Module(module) => Some(format!("#{}.{name}", module.0)),
        Type::HostClock => Some(format!("HOST.Clock.{name}")),
        Type::HostRandom => Some(format!("HOST.Random.{name}")),
        Type::HostConsole => Some(format!("HOST.Console.{name}")),
        Type::HostFileSystem => Some(format!("HOST.FileSystem.{name}")),
        _ => None,
    }
}

fn user_class_name(ty: &Type) -> Option<String> {
    match ty {
        Type::TypeName(name)
            if !matches!(
                name.as_str(),
                "Float" | "Date" | "Time" | "TimeZone" | "Timestamp" | "Error" | "SYSTEM"
            ) =>
        {
            Some(name.clone())
        }
        Type::ImportedTypeName { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn static_class_name(ty: &Type, prefix: &str) -> String {
    match ty {
        Type::TypeName(name) => format!("{prefix}{name}"),
        Type::ImportedTypeName { module, name } => format!("#{}.{name}", module.0),
        other => display_type(other),
    }
}

fn class_ir_name(ty: &Type, type_name: &str, prefix: &str) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) => format!("{prefix}{name}"),
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            format!("#{}.{name}", module.0)
        }
        _ => {
            if is_numeric_type_name(type_name) {
                type_name.into()
            } else {
                format!("{prefix}{type_name}")
            }
        }
    }
}

fn is_numeric_type_name(name: &str) -> bool {
    matches!(
        name,
        "BYTE"
            | "INT8"
            | "INT16"
            | "INT32"
            | "INT64"
            | "INTEGER"
            | "UINT16"
            | "UINT32"
            | "UINT64"
            | "FLOAT32"
            | "FLOAT64"
            | "FLOAT"
            | "TIMESTAMP"
    )
}

fn destructor_name(
    model: &SemanticModel,
    span: Span,
    methods: &HashSet<String>,
    prefix: &str,
) -> Option<String> {
    let ty = type_at(model, span).ok()?;
    let name = match ty {
        Type::Named(name) => format!("{prefix}{name}.DESTRUCTOR"),
        Type::ImportedNamed { module, name } => format!("#{}.{name}.DESTRUCTOR", module.0),
        Type::Alternative(types) => types.iter().find_map(|ty| match ty {
            Type::Named(name) => Some(format!("{prefix}{name}.DESTRUCTOR")),
            Type::ImportedNamed { module, name } => {
                Some(format!("#{}.{name}.DESTRUCTOR", module.0))
            }
            _ => None,
        })?,
        _ => return None,
    };
    methods.contains(&name).then_some(name)
}

fn display_type(ty: &Type) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) | Type::ImportedNamed { name, .. } => name.clone(),
        _ => format!("{ty:?}"),
    }
}

fn instruction_defines(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Constant { destination, .. }
        | Instruction::Default { destination, .. }
        | Instruction::Load { destination, .. }
        | Instruction::Copy { destination, .. }
        | Instruction::Unary { destination, .. }
        | Instruction::Binary { destination, .. }
        | Instruction::Cast { destination, .. }
        | Instruction::Call { destination, .. }
        | Instruction::Input { destination, .. }
        | Instruction::Vector { destination, .. }
        | Instruction::Index { destination, .. }
        | Instruction::Member { destination, .. }
        | Instruction::Length { destination, .. }
        | Instruction::SizeOf { destination, .. }
        | Instruction::Allocate { destination, .. }
        | Instruction::LoadStatic { destination, .. } => Some(*destination),
        Instruction::Store { .. }
        | Instruction::SetIndex { .. }
        | Instruction::SetMember { .. }
        | Instruction::SetField { .. }
        | Instruction::Print { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::Delete { .. }
        | Instruction::EnsureClass { .. }
        | Instruction::StoreStatic { .. } => None,
    }
}

fn instruction_uses(instruction: &Instruction) -> Vec<ValueId> {
    match instruction {
        Instruction::Copy { source, .. }
        | Instruction::Unary {
            operand: source, ..
        }
        | Instruction::Cast { value: source, .. }
        | Instruction::Length { vector: source, .. }
        | Instruction::SizeOf { value: source, .. }
        | Instruction::Store { value: source, .. }
        | Instruction::Delete { value: source, .. } => vec![*source],
        Instruction::Binary { left, right, .. } => vec![*left, *right],
        Instruction::Call {
            callee, arguments, ..
        } => {
            let mut used = vec![*callee];
            used.extend(arguments.iter().copied());
            used
        }
        Instruction::Vector { values, .. } | Instruction::Print { values, .. } => values.clone(),
        Instruction::Index { object, index, .. } => vec![*object, *index],
        Instruction::Member { object, .. }
        | Instruction::ClearScreen {
            console: object, ..
        }
        | Instruction::Beep {
            console: object, ..
        } => vec![*object],
        Instruction::SetIndex { indices, value, .. } => {
            let mut used = indices.clone();
            used.push(*value);
            used
        }
        Instruction::SetMember { object, value, .. } => vec![*object, *value],
        Instruction::SetField { value, .. } | Instruction::StoreStatic { value, .. } => {
            vec![*value]
        }
        Instruction::Allocate { arguments, .. } => arguments.clone(),
        Instruction::EnsureClass { .. }
        | Instruction::LoadStatic { .. }
        | Instruction::Constant { .. }
        | Instruction::Default { .. }
        | Instruction::Load { .. }
        | Instruction::Input { .. } => Vec::new(),
    }
}

fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
