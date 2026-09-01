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

mod builder;
mod model;
mod validate;
pub use model::{
    BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator, ValueId,
};

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

/// Returns a source-spanned diagnostic if the AST contains a construct that
/// is not part of the core IR yet or if semantic resolution data is missing.
///
/// # Errors
///
/// Returns a diagnostic when semantic information is missing or lowering encounters an unsupported construct.
pub fn lower(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    let functions = lower_program(program, model, "", &collect_methods(program, ""))?;
    let module = Module {
        source_name: program.source_name.clone(),
        functions,
        bndata_providers: HashSet::new(),
        bnmath_providers: HashSet::new(),
        bnlog_providers: HashSet::new(),
        bnjson_providers: HashSet::new(),
        bnweb_providers: HashSet::new(),
        bndispatch_providers: HashSet::new(),
        filesystem_import: filesystem_import_span(program),
        console_import: console_import_span(program),
        network_import: network_import_span(program),
        bnlog_import: standard_import_span(program, "BNLog"),
        bnweb_import: standard_import_span(program, "BNWeb"),
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
    let console_import = root.and_then(|module| console_import_span(&module.program));
    let network_import = root.and_then(|module| network_import_span(&module.program));
    let bnlog_import = root.and_then(|module| standard_import_span(&module.program, "BNLog"));
    let bnweb_import = root.and_then(|module| standard_import_span(&module.program, "BNWeb"));
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
        bnlog_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNLog)).then_some(loaded.id)
            })
            .collect(),
        bnjson_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNJson)).then_some(loaded.id)
            })
            .collect(),
        bnweb_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNWeb)).then_some(loaded.id)
            })
            .collect(),
        bndispatch_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNDispatch)).then_some(loaded.id)
            })
            .collect(),
        filesystem_import,
        console_import,
        network_import,
        bnlog_import,
        bnweb_import,
    };
    validate(&module)?;
    Ok(module)
}

mod lowering;
use lowering::{class_method_name, collect_methods, lower_program, module_prefix};

/// Validates structural invariants required by IR consumers.
///
/// # Errors
///
/// Returns `INVALID_IR` when a block, value, or terminator reference is invalid.
pub fn validate(module: &Module) -> Result<(), Diagnostic> {
    validate::validate(module)
}

mod helpers;
use helpers::{
    assignment_operator, class_ir_name, console_import_span, constant, destructor_name,
    display_type, filesystem_constant, filesystem_import_span, host_capability_constant,
    invalid_ir, ir_error, is_namespace_type, is_numeric_type_name, math_constant, named_or_void,
    namespace_function, network_import_span, standard_import_span, static_class_name, type_at,
    type_test_name, user_class_name,
};
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
