// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Unit tests of the language IR validator: function kinds, field layouts,
// member and field paths, and alternative widening.

use std::collections::{BTreeMap, HashMap, HashSet};

use bn_source::{Position, Revision, SourceId, Span};
use bn_types::Type;

use super::super::{
    BasicBlock, BlockId, Constant, FieldId, FieldLayout, FieldLayoutEntry, FieldRef, FieldSlot,
    Function, FunctionKind, Instruction, Module, SymbolId, Terminator, ValueId,
};

fn span() -> Span {
    let position = Position {
        source_id: SourceId(1),
        revision: Revision(1),
        offset: 0,
        line: 1,
        column: 1,
    };
    Span {
        start: position,
        end: position,
    }
}

fn function(
    name: &str,
    kind: FunctionKind,
    owner: Option<&str>,
    params: usize,
    ret: &str,
) -> Function {
    Function {
        name: name.into(),
        kind,
        owner: owner.map(str::to_string),
        asynchronous: false,
        parameters: (0..params)
            .map(|i| SymbolId(u32::try_from(i).expect("small")))
            .collect(),
        weak_symbols: HashSet::new(),
        return_type: Type::Named(ret.into()),
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        }],
        span: span(),
    }
}

fn module(functions: Vec<Function>) -> Module {
    Module {
        functions,
        ..Module::default()
    }
}

fn detail(result: Result<(), bn_diag::Diagnostic>) -> String {
    result.expect_err("must be invalid IR").message.to_string()
}

#[test]
fn alternatives_widen_but_never_narrow_without_a_test() {
    use bn_types::IntegerType::Int32;
    let error = Type::Named("Error".into());
    let found = Type::Alternative(vec![Type::Null, Type::Integer(Int32)]);
    let result = Type::Alternative(vec![Type::Integer(Int32), Type::Null, error]);
    // Widening, in any member order (0.6.md rule 3).
    assert!(super::assignment_types_compatible(&found, &result));
    assert!(super::call_types_compatible(&found, &result));
    // Narrowing needs an IS test, so the IR never stores it (rule 4).
    assert!(!super::assignment_types_compatible(&result, &found));
    assert!(!super::call_types_compatible(&result, &found));
    assert!(!super::assignment_types_compatible(
        &found,
        &Type::Integer(Int32)
    ));
    // A member still enters its alternative.
    assert!(super::assignment_types_compatible(
        &Type::Integer(Int32),
        &result
    ));
}

#[test]
fn field_layout_slots_must_be_dense_and_ordered() {
    let layout = FieldLayout {
        owner: "Point".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(1),
            ty: Type::Named("INTEGER".into()),
            declaring_owner: "Point".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let mut module = module(Vec::new());
    module.field_names = vec!["x".into()];
    module.field_layouts = BTreeMap::from([("Point".into(), layout)]);

    assert!(detail(super::validate(&module)).contains("slots must be dense"));
}

#[test]
fn field_layout_ids_must_exist_in_the_interned_name_table() {
    let layout = FieldLayout {
        owner: "Point".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(1),
            slot: FieldSlot::from_raw(0),
            ty: Type::Named("INTEGER".into()),
            declaring_owner: "Point".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let mut module = module(Vec::new());
    module.field_names = vec!["x".into()];
    module.field_layouts = BTreeMap::from([("Point".into(), layout)]);

    assert!(detail(super::validate(&module)).contains("absent from the name table"));
}

#[test]
fn member_receiver_and_result_must_match_the_field_layout() {
    let layout = FieldLayout {
        owner: "Point".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(0),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            declaring_owner: "Point".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let mut module = module(vec![function(
        "Start",
        FunctionKind::Entry,
        None,
        0,
        "VOID",
    )]);
    module.field_names = vec!["x".into()];
    module.field_layouts = BTreeMap::from([("Point".into(), layout)]);
    module.functions[0].blocks[0].instructions = vec![
        Instruction::Default {
            destination: ValueId(0),
            ty: Type::Named("Point".into()),
            dimensions: Vec::new(),
            dynamic_dimensions: Vec::new(),
            span: span(),
        },
        Instruction::Member {
            destination: ValueId(1),
            object: ValueId(0),
            field: Some(FieldRef {
                owner: "Point".into(),
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
            }),
            name: "x".into(),
            owner: "Point".into(),
            ty: Type::String,
            span: span(),
        },
    ];

    assert!(detail(super::validate(&module)).contains("result type"));
    let Instruction::Default { ty, .. } = &mut module.functions[0].blocks[0].instructions[0] else {
        unreachable!("default");
    };
    *ty = Type::Boolean;
    let Instruction::Member { ty, .. } = &mut module.functions[0].blocks[0].instructions[1] else {
        unreachable!("member");
    };
    *ty = Type::Integer(bn_types::IntegerType::Int32);
    assert!(detail(super::validate(&module)).contains("receiver"));
}

#[test]
fn derived_receiver_may_access_a_base_layout_field() {
    let base_layout = FieldLayout {
        owner: "Animal".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(0),
            ty: Type::String,
            declaring_owner: "Animal".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let derived_layout = FieldLayout {
        owner: "Dog".into(),
        fields: base_layout.fields.clone(),
        span: span(),
    };
    let mut module = module(vec![
        function(
            "Animal.$fields",
            FunctionKind::FieldInit,
            Some("Animal"),
            1,
            "VOID",
        ),
        function(
            "Dog.$fields",
            FunctionKind::FieldInit,
            Some("Dog"),
            1,
            "VOID",
        ),
        function("Start", FunctionKind::Entry, None, 0, "VOID"),
    ]);
    module.field_names = vec!["name".into()];
    module.class_bases = HashMap::from([("Dog".into(), "Animal".into())]);
    module.field_layouts = BTreeMap::from([
        ("Animal".into(), base_layout),
        ("Dog".into(), derived_layout),
    ]);
    module.functions[2].blocks[0].instructions = vec![
        Instruction::Default {
            destination: ValueId(0),
            ty: Type::Named("Dog".into()),
            dimensions: Vec::new(),
            dynamic_dimensions: Vec::new(),
            span: span(),
        },
        Instruction::Member {
            destination: ValueId(1),
            object: ValueId(0),
            field: Some(FieldRef {
                owner: "Animal".into(),
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
            }),
            name: "name".into(),
            owner: "Animal".into(),
            ty: Type::String,
            span: span(),
        },
        // The `Default` object is owned: it is released, as the ownership
        // balance rule requires.
        Instruction::Release {
            value: ValueId(0),
            destructor: None,
            span: span(),
        },
    ];

    super::validate(&module).expect("derived receiver may access inherited base field");
}

#[test]
fn field_name_table_must_not_duplicate_spellings() {
    let mut module = module(Vec::new());
    module.field_names = vec!["x".into(), "x".into()];

    assert!(detail(super::validate(&module)).contains("field names must be unique"));
}

#[test]
fn derived_layout_must_preserve_the_base_prefix() {
    let base = FieldLayout {
        owner: "Parent".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(0),
            ty: Type::Named("INTEGER".into()),
            declaring_owner: "Parent".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let child = FieldLayout {
        owner: "Child".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(1),
            slot: FieldSlot::from_raw(0),
            ty: Type::Named("INTEGER".into()),
            declaring_owner: "Child".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let mut module = module(vec![function(
        "Parent.$fields",
        FunctionKind::FieldInit,
        Some("Parent"),
        1,
        "VOID",
    )]);
    module.functions.push(function(
        "Child.$fields",
        FunctionKind::FieldInit,
        Some("Child"),
        1,
        "VOID",
    ));
    module.field_names = vec!["first".into(), "second".into()];
    module.class_bases = HashMap::from([("Child".into(), "Parent".into())]);
    module.field_layouts = BTreeMap::from([("Parent".into(), base), ("Child".into(), child)]);

    assert!(detail(super::validate(&module)).contains("base layout prefix"));
}

#[test]
fn field_path_stores_require_matching_resolved_fields() {
    let layout = FieldLayout {
        owner: "Point".into(),
        fields: vec![FieldLayoutEntry {
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(0),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            declaring_owner: "Point".into(),
            weak: false,
            span: span(),
        }],
        span: span(),
    };
    let mut module = module(vec![function(
        "Start",
        FunctionKind::Entry,
        None,
        0,
        "VOID",
    )]);
    module.field_names = vec!["x".into()];
    module.field_layouts = BTreeMap::from([("Point".into(), layout)]);
    module.functions[0].blocks[0].instructions = vec![
        Instruction::Constant {
            destination: ValueId(0),
            value: Constant::Integer("1".into()),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            span: span(),
        },
        Instruction::SetField {
            previous: None,
            symbol: SymbolId(0),
            root_owner: "Point".into(),
            path: vec!["x".into()],
            fields: None,
            value: ValueId(0),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            span: span(),
        },
    ];

    assert!(detail(super::validate(&module)).contains("must carry resolved fields"));

    let Instruction::SetField { fields, .. } = &mut module.functions[0].blocks[0].instructions[1]
    else {
        unreachable!("field store");
    };
    *fields = Some(vec![FieldRef {
        owner: "Point".into(),
        id: FieldId::from_raw(0),
        slot: FieldSlot::from_raw(1),
    }]);
    assert!(detail(super::validate(&module)).contains("does not match"));
}

#[test]
fn well_formed_kinds_validate() {
    let m = module(vec![
        function("Start", FunctionKind::Entry, None, 0, "VOID"),
        function(
            "C.CONSTRUCTOR",
            FunctionKind::Constructor,
            Some("C"),
            1,
            "VOID",
        ),
        function(
            "C.DESTRUCTOR",
            FunctionKind::Destructor,
            Some("C"),
            1,
            "VOID",
        ),
        function("C.$fields", FunctionKind::FieldInit, Some("C"), 1, "VOID"),
        function("C.$init", FunctionKind::Init, Some("C"), 0, "VOID"),
        function("P.$default", FunctionKind::Default, Some("P"), 0, "VOID"),
        function("C.Method", FunctionKind::User, Some("C"), 1, "VOID"),
        function("Free", FunctionKind::User, None, 0, "VOID"),
    ]);
    super::validate(&m).expect("well-formed kinds");
}

#[test]
fn two_entries_are_invalid() {
    let m = module(vec![
        function("Start", FunctionKind::Entry, None, 0, "VOID"),
        function("Start2", FunctionKind::Entry, None, 0, "VOID"),
    ]);
    assert!(detail(super::validate(&m)).contains("more than one entry"));
}

#[test]
fn entry_with_parameters_is_invalid() {
    let m = module(vec![function(
        "Start",
        FunctionKind::Entry,
        None,
        1,
        "VOID",
    )]);
    assert!(detail(super::validate(&m)).contains("entry function must not take parameters"));
}

#[test]
fn synthesised_kinds_need_an_owner() {
    for kind in [
        FunctionKind::Constructor,
        FunctionKind::Destructor,
        FunctionKind::FieldInit,
        FunctionKind::Init,
    ] {
        let m = module(vec![function("X.f", kind, None, 1, "VOID")]);
        assert!(
            detail(super::validate(&m)).contains("need an owner class"),
            "{kind:?}"
        );
    }
    let m = module(vec![function(
        "P.$default",
        FunctionKind::Default,
        None,
        0,
        "VOID",
    )]);
    assert!(detail(super::validate(&m)).contains("needs an owner struct"));
}

#[test]
fn self_taking_kinds_need_a_parameter() {
    for kind in [
        FunctionKind::Constructor,
        FunctionKind::Destructor,
        FunctionKind::FieldInit,
    ] {
        let m = module(vec![function("C.f", kind, Some("C"), 0, "VOID")]);
        assert!(
            detail(super::validate(&m)).contains("take SELF first"),
            "{kind:?}"
        );
    }
}

#[test]
fn destructor_must_return_void() {
    let m = module(vec![function(
        "C.DESTRUCTOR",
        FunctionKind::Destructor,
        Some("C"),
        1,
        "INTEGER",
    )]);
    assert!(detail(super::validate(&m)).contains("destructor must return VOID"));
}

#[test]
fn default_constructor_takes_no_parameters() {
    let m = module(vec![function(
        "P.$default",
        FunctionKind::Default,
        Some("P"),
        1,
        "VOID",
    )]);
    assert!(detail(super::validate(&m)).contains("must not take parameters"));
}
