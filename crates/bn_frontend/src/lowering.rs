// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
#[path = "lowering/field_layouts.rs"]
mod field_layouts;
pub use bn_ir::{
    BasicBlock, BlockId, ClassModel, Constant, FieldId, FieldLayout, FieldLayoutEntry, FieldRef,
    FieldSlot, Function, FunctionKind, Instruction, Module, ModuleId, SymbolId, Terminator,
    ValidatedModule, ValueId, validate, validate_module,
};
use field_layouts::{lower_field_layout, record_owner, resolve_member_fields};

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

/// The fully qualified names of the interfaces `program` declares.
fn lowered_interfaces(program: &Program, prefix: &str) -> BTreeSet<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration {
                kind: DeclarationKind::Interface,
                name,
                ..
            } => Some(qualified_class_name(prefix, name)),
            _ => None,
        })
        .collect()
}

/// The module's class relation from semantic analysis, with the names
/// `bn_ir` compares: local classes and interfaces get `prefix`; imported
/// ones are already `#<module>.Name`.
fn lowered_class_model(model: &SemanticModel, prefix: &str) -> ClassModel {
    let mut class_model = ClassModel::new();
    for (class, base) in &model.class_model.bases {
        class_model.add_base(
            qualified_class_name(prefix, class),
            qualified_class_name(prefix, base),
        );
    }
    for (class, interfaces) in &model.class_model.interfaces {
        for interface in interfaces {
            class_model.add_interface(
                qualified_class_name(prefix, class),
                qualified_class_name(prefix, interface),
            );
        }
    }
    class_model
}

#[derive(Clone)]
struct PendingField {
    name: String,
    ty: Type,
    weak: bool,
    span: Span,
}

#[derive(Clone)]
struct PendingLayout {
    owner: String,
    span: Span,
    fields: Vec<PendingField>,
}

fn collect_layouts(
    program: &Program,
    model: &SemanticModel,
    prefix: &str,
    layouts: &mut HashMap<String, PendingLayout>,
) {
    for item in &program.items {
        let Item::Declaration {
            kind: DeclarationKind::Class | DeclarationKind::Struct,
            name,
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        let owner = qualified_class_name(prefix, name);
        let fields = statements
            .iter()
            .filter_map(|statement| {
                let Statement::Binding {
                    name,
                    type_ref,
                    is_static: false,
                    span,
                    ..
                } = statement
                else {
                    return None;
                };
                Some(PendingField {
                    name: name.clone(),
                    ty: type_at(model, *span).unwrap_or_else(|_| named_or_void(type_ref)),
                    weak: type_ref
                        .alternatives
                        .first()
                        .is_some_and(|atom| atom.name == "WEAK"),
                    span: *span,
                })
            })
            .collect();
        layouts.insert(
            owner.clone(),
            PendingLayout {
                owner,
                span: *span,
                fields,
            },
        );
    }
}

fn collect_semantic_record_layouts(
    model: &SemanticModel,
    prefix: &str,
    layouts: &mut HashMap<String, PendingLayout>,
) {
    for (owner, members) in &model.record_members {
        let owner = qualified_class_name(prefix, owner);
        layouts
            .entry(owner.clone())
            .or_insert_with(|| PendingLayout {
                owner,
                span: default_span(),
                fields: members
                    .iter()
                    .map(|member| PendingField {
                        name: member.name.clone(),
                        ty: member.ty.clone(),
                        weak: false,
                        span: member.span,
                    })
                    .collect(),
            });
    }
}

fn lowered_field_metadata(
    pending: &HashMap<String, PendingLayout>,
    class_bases: &BTreeMap<String, String>,
) -> Result<(Vec<String>, BTreeMap<String, FieldLayout>), Diagnostic> {
    let mut names = Vec::new();
    let mut ids = HashMap::new();
    let mut layouts = BTreeMap::new();
    let mut visiting = HashSet::new();
    let mut owners = pending.keys().cloned().collect::<Vec<_>>();
    owners.sort();
    for owner in owners {
        lower_field_layout(
            &owner,
            pending,
            class_bases,
            &mut names,
            &mut ids,
            &mut layouts,
            &mut visiting,
        )?;
    }
    Ok((names, layouts))
}

struct OpenBlock {
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
}

struct LoopTargets {
    kind: &'static str,
    exit: BlockId,
    continue_at: BlockId,
    /// Open scopes outside the loop body: `EXIT` and `CONTINUE` release the
    /// ones from here inward.
    scope_depth: usize,
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
        root_owner: String,
        path: Vec<String>,
        indices: Vec<ValueId>,
    },
    Field {
        symbol: SymbolId,
        root_owner: String,
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
    // Ownership (proposal `arc-shared-core-0.6.5`), decided here only.
    /// The ARC locals of each open block, in declaration order.
    scopes: Vec<Vec<(SymbolId, Type, Span)>>,
    /// Owned values nothing has consumed yet, with the span that made them.
    owned: Vec<(ValueId, Span)>,
    /// The type of every value emitted so far.
    value_types: HashMap<ValueId, Type>,
    /// `AS WEAK` locals: never retained, never released.
    weak_locals: HashSet<SymbolId>,
    /// The function's parameters: borrowed, so `RELEASE` only ends them.
    parameters: HashSet<SymbolId>,
    /// The function named by each function constant emitted so far.
    function_names: HashMap<ValueId, String>,
}

/// Returns a source-spanned diagnostic if the AST contains a construct that
/// is not part of the core IR yet or if semantic resolution data is missing.
///
/// # Errors
///
/// Returns a diagnostic when semantic information is missing or lowering encounters an unsupported construct.
fn lower_unvalidated(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    let functions = lower_program(program, model, "", &collect_methods(program, ""))?;
    let class_model = lowered_class_model(model, "");
    let mut pending_layouts = HashMap::new();
    collect_layouts(program, model, "", &mut pending_layouts);
    collect_semantic_record_layouts(model, "", &mut pending_layouts);
    let (field_names, field_layouts) =
        lowered_field_metadata(&pending_layouts, &class_model.bases)?;
    let mut module = Module {
        source_name: program.source_name.clone(),
        functions,
        field_names,
        field_layouts,
        class_model,
        interfaces: lowered_interfaces(program, ""),
        bndata_providers: HashSet::new(),
        bnmath_providers: HashSet::new(),
        bnlog_providers: HashSet::new(),
        bnjson_providers: HashSet::new(),
        bnweb_providers: HashSet::new(),
        bndispatch_providers: HashSet::new(),
        bncrypto_providers: HashSet::new(),
        bnsqlite_providers: HashSet::new(),
        filesystem_import: filesystem_import_span(program),
        clock_import: clock_import_span(program),
        random_import: random_import_span(program),
        console_import: console_import_span(program),
        network_import: network_import_span(program),
        exec_import: exec_import_span(program),
        env_import: env_import_span(program),
        bnlog_import: standard_import_span(program, "BNLog"),
        bnweb_import: standard_import_span(program, "BNWeb"),
        bnsqlite_import: standard_import_span(program, "BNSqlite"),
    };
    resolve_member_fields(&mut module)?;
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
#[allow(clippy::too_many_lines)] // Module graph lowering preserves one deterministic construction path.
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
    let mut class_model = ClassModel::new();
    let mut interfaces = BTreeSet::new();
    let mut pending_layouts = HashMap::new();
    for loaded in &graph.modules {
        let index = usize::try_from(loaded.id.0)
            .map_err(|_| ir_error("module index does not fit", default_span()))?;
        let model = models
            .get(index)
            .ok_or_else(|| ir_error("missing semantic model for module", default_span()))?;
        let prefix = module_prefix(ir_module_id(graph.root), ir_module_id(loaded.id));
        // Standard modules contribute their class declarations (names only)
        // so a user class may implement a standard interface; their bodies
        // are bound natively and are not lowered.
        let loaded_model = lowered_class_model(model, &prefix);
        for (sub, base) in loaded_model.bases {
            class_model.add_base(sub, base);
        }
        for (sub, ifaces) in loaded_model.interfaces {
            for iface in ifaces {
                class_model.add_interface(&sub, iface);
            }
        }
        if loaded.standard_module.is_some() {
            continue;
        }
        interfaces.extend(lowered_interfaces(&loaded.program, &prefix));
        collect_layouts(&loaded.program, model, &prefix, &mut pending_layouts);
        collect_semantic_record_layouts(model, &prefix, &mut pending_layouts);
        functions.extend(lower_program(
            &loaded.program,
            model,
            &prefix,
            &method_names,
        )?);
    }
    let root = graph.modules.iter().find(|module| module.id == graph.root);
    let source_name = root.and_then(|module| module.program.source_name.clone());
    let filesystem_import = graph
        .modules
        .iter()
        .find_map(|module| filesystem_import_span(&module.program));
    let clock_import = graph
        .modules
        .iter()
        .find_map(|module| clock_import_span(&module.program));
    let random_import = graph
        .modules
        .iter()
        .find_map(|module| random_import_span(&module.program));
    let console_import = graph
        .modules
        .iter()
        .find_map(|module| console_import_span(&module.program));
    let network_import = graph
        .modules
        .iter()
        .find_map(|module| network_import_span(&module.program));
    let exec_import = graph
        .modules
        .iter()
        .find_map(|module| exec_import_span(&module.program));
    let env_import = graph
        .modules
        .iter()
        .find_map(|module| env_import_span(&module.program));
    let bnlog_import = graph
        .modules
        .iter()
        .find_map(|module| standard_import_span(&module.program, "BNLog"));
    let bnweb_import = graph
        .modules
        .iter()
        .find_map(|module| standard_import_span(&module.program, "BNWeb"));
    let bnsqlite_import = graph
        .modules
        .iter()
        .find_map(|module| standard_import_span(&module.program, "BNSqlite"));
    let (field_names, field_layouts) =
        lowered_field_metadata(&pending_layouts, &class_model.bases)?;
    let mut module = Module {
        source_name,
        functions,
        field_names,
        field_layouts,
        class_model,
        interfaces,
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
        bncrypto_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNCrypto))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        bnsqlite_providers: graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(StandardModule::BNSqlite))
                    .then_some(ir_module_id(loaded.id))
            })
            .collect(),
        filesystem_import,
        clock_import,
        random_import,
        console_import,
        network_import,
        exec_import,
        env_import,
        bnlog_import,
        bnweb_import,
        bnsqlite_import,
    };
    resolve_member_fields(&mut module)?;
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
    assignment_operator, capability_constant, class_ir_name, clock_import_span,
    console_import_span, constant, destructor_name, display_type, env_import_span,
    exec_import_span, filesystem_import_span, host_capability_constant, ir_error,
    is_host_or_builtin_owner, is_namespace_type, is_numeric_type_name, is_user_class_name,
    math_constant, module_constant, module_id_from_prefix, named_or_void, namespace_function,
    network_import_span, random_import_span, standard_import_span, static_class_name, type_at,
    type_test_name, user_class_name,
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
