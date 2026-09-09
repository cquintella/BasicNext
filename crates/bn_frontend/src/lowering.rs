// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet};

use crate::{
    ast::{
        Block as AstBlock, DeclarationKind, Expression, ExpressionKind, ForHeader,
        FunctionSignature, Item, Literal, Program, Statement, TypeAtom, TypeReference,
    },
    diagnostic::Diagnostic,
    module_graph::{ModuleGraph, ModuleId as FrontendModuleId, StandardModule},
    semantic::{SemanticModel, SymbolId as FrontendSymbolId, static_len},
    source::Span,
};

pub use crate::types::{FloatType, IntegerType, PointerLength, Type};

#[path = "lowering/builder.rs"]
mod builder;
pub use bn_ir::{
    BasicBlock, BlockId, Constant, Function, Instruction, Module, ModuleId, SymbolId, Terminator,
    ValidatedModule, ValueId, validate, validate_module,
};

pub(crate) fn ir_symbol_id(value: FrontendSymbolId) -> SymbolId {
    SymbolId::from_raw(value.value())
}

fn ir_module_id(value: FrontendModuleId) -> ModuleId {
    ModuleId(value.0)
}

fn qualified_class_name(prefix: &str, name: &str) -> String {
    if name.starts_with('#') {
        name.to_string()
    } else {
        format!("{prefix}{name}")
    }
}

fn lowered_class_bases(
    program: &Program,
    model: &SemanticModel,
    prefix: &str,
) -> HashMap<String, String> {
    program
        .items
        .iter()
        .filter_map(|item| {
            let Item::Declaration {
                kind: DeclarationKind::Class,
                name,
                ..
            } = item
            else {
                return None;
            };
            let base = model.base_classes.get(name)?;
            Some((
                qualified_class_name(prefix, name),
                qualified_class_name(prefix, base),
            ))
        })
        .collect()
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
    MemberIndex {
        object: ValueId,
        name: String,
        owner: String,
        indices: Vec<ValueId>,
    },
    FieldIndex {
        symbol: SymbolId,
        path: Vec<String>,
        indices: Vec<ValueId>,
    },
    Field {
        symbol: SymbolId,
        path: Vec<String>,
    },
    Static {
        class: String,
        field: String,
    },
    StaticIndex {
        class: String,
        field: String,
        indices: Vec<ValueId>,
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
fn lower_unvalidated(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    let functions = lower_program(program, model, "", &collect_methods(program, ""))?;
    let module = Module {
        source_name: program.source_name.clone(),
        functions,
        class_bases: lowered_class_bases(program, model, ""),
        bndata_providers: HashSet::new(),
        bnmath_providers: HashSet::new(),
        bnlog_providers: HashSet::new(),
        bnjson_providers: HashSet::new(),
        bnweb_providers: HashSet::new(),
        bndispatch_providers: HashSet::new(),
        filesystem_import: filesystem_import_span(program),
        clock_import: clock_import_span(program),
        random_import: random_import_span(program),
        console_import: console_import_span(program),
        network_import: network_import_span(program),
        bnlog_import: standard_import_span(program, "BNLog"),
        bnweb_import: standard_import_span(program, "BNWeb"),
    };
    Ok(module)
}

/// Lowers and validates one program for a backend handoff.
///
/// This is the language-level gate only. Target capability checks (for example
/// whether LLVM can emit a valid BN operation) belong to a separate
/// `validate_for(target)` stage and must report support diagnostics rather than
/// turning a valid BN program into invalid IR.
///
/// # Errors
///
/// Returns a source-spanned diagnostic when lowering or language validation
/// fails.
pub fn lower_validated(
    program: &Program,
    model: &SemanticModel,
) -> Result<ValidatedModule, Diagnostic> {
    validate_module(lower_unvalidated(program, model)?)
}

/// Lowers one program, retaining the historical unchecked-module API.
///
/// New backend handoffs must use [`lower_validated`].
///
/// # Errors
///
/// Returns a source-spanned diagnostic when lowering or language validation
/// fails.
pub fn lower(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    lower_validated(program, model).map(ValidatedModule::into_module)
}

/// Lowers every module in an acyclic graph into one IR module.
///
/// Imported functions are named `#id.name` so two modules may export `Soma`.
///
/// # Errors
///
/// Returns a source-spanned diagnostic if any module cannot be lowered.
fn lower_graph_unvalidated(
    graph: &ModuleGraph,
    models: &[SemanticModel],
) -> Result<Module, Diagnostic> {
    let mut method_names = HashSet::new();
    for loaded in &graph.modules {
        let prefix = module_prefix(ir_module_id(graph.root), ir_module_id(loaded.id));
        method_names.extend(collect_methods(&loaded.program, &prefix));
    }
    let mut functions = Vec::new();
    let mut class_bases = HashMap::new();
    for loaded in &graph.modules {
        if loaded.standard_module.is_some() {
            continue;
        }
        let index = usize::try_from(loaded.id.0)
            .map_err(|_| ir_error("module index does not fit", default_span()))?;
        let model = models
            .get(index)
            .ok_or_else(|| ir_error("missing semantic model for module", default_span()))?;
        let prefix = module_prefix(ir_module_id(graph.root), ir_module_id(loaded.id));
        class_bases.extend(lowered_class_bases(&loaded.program, model, &prefix));
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
    let clock_import = root.and_then(|module| clock_import_span(&module.program));
    let random_import = root.and_then(|module| random_import_span(&module.program));
    let console_import = root.and_then(|module| console_import_span(&module.program));
    let network_import = root.and_then(|module| network_import_span(&module.program));
    let bnlog_import = root.and_then(|module| standard_import_span(&module.program, "BNLog"));
    let bnweb_import = root.and_then(|module| standard_import_span(&module.program, "BNWeb"));
    let module = Module {
        source_name,
        functions,
        class_bases,
        bndata_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNData))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bnmath_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNMath))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bnlog_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNLog))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bnjson_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNJson))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bnweb_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNWeb))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bndispatch_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNDispatch))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        filesystem_import,
        clock_import,
        random_import,
        console_import,
        network_import,
        bnlog_import,
        bnweb_import,
    };
    Ok(module)
}

/// Lowers and validates a complete module graph for a backend handoff.
///
/// # Errors
///
/// Returns a source-spanned diagnostic when lowering or language validation
/// fails.
pub fn lower_graph_validated(
    graph: &ModuleGraph,
    models: &[SemanticModel],
) -> Result<ValidatedModule, Diagnostic> {
    validate_module(lower_graph_unvalidated(graph, models)?)
}

/// Lowers a complete graph, retaining the historical module-returning API.
///
/// New backend handoffs must use [`lower_graph_validated`].
///
/// # Errors
///
/// Returns a source-spanned diagnostic when lowering or language validation
/// fails.
pub fn lower_graph(graph: &ModuleGraph, models: &[SemanticModel]) -> Result<Module, Diagnostic> {
    lower_graph_validated(graph, models).map(ValidatedModule::into_module)
}

#[path = "lowering/lowering.rs"]
mod program_lowering;
use program_lowering::{class_method_name, collect_methods, lower_program, module_prefix};

#[path = "lowering/helpers.rs"]
mod helpers;
use helpers::{
    assignment_operator, class_ir_name, clock_import_span, console_import_span, constant,
    destructor_name, display_type, filesystem_constant, filesystem_import_span,
    host_capability_constant, ir_error, is_namespace_type, is_numeric_type_name, math_constant,
    module_constant, named_or_void, namespace_function, network_import_span, random_import_span,
    standard_import_span, static_class_name, type_at, type_test_name, user_class_name,
};
fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            source_id: crate::source::Position::UNKNOWN_SOURCE,
            revision: crate::source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            source_id: crate::source::Position::UNKNOWN_SOURCE,
            revision: crate::source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
