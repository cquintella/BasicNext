// Basic Next interactive session unit tests (Cluster B)

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::Ordering;

use super::*;
use bn_ir::{
    BasicBlock, BlockId, Constant, Function, FunctionKind, Instruction, Module, SymbolId,
    Terminator,
};
use bn_source::Span;
use bn_types::{IntegerType, Type};

fn dummy_span() -> Span {
    default_span()
}

fn build_validated_module(functions: Vec<Function>) -> ValidatedModule {
    let module = Module {
        functions,
        ..Module::default()
    };
    bn_ir::validate_module(module).expect("valid IR module")
}

#[test]
fn session_initial_state_and_reset() {
    let host = HostEnv::sandbox(Vec::new());
    let mut session = InteractiveSession::new(host);

    assert!(session.variables().is_empty());
    let inspection = session.arc_inspection();
    assert_eq!(inspection.total_live_objects, 0);
    assert_eq!(inspection.live_memory_allocations, 0);
    assert_eq!(inspection.live_instances, 0);

    // Cancel flag initially false
    assert!(!session.cancel_flag().load(Ordering::Relaxed));
    session.cancel();
    assert!(session.cancel_flag().load(Ordering::Relaxed));

    // Reset restores all
    session.reset();
    assert!(!session.cancel_flag().load(Ordering::Relaxed));
    assert!(session.variables().is_empty());
}

#[test]
fn session_executes_simple_turn_and_updates_variables() {
    let host = HostEnv::sandbox(Vec::new());
    let mut session = InteractiveSession::new(host);

    // Create Start function: LET x = 42; RETURN 0
    let sym_x = SymbolId(0);
    let start_fn = Function {
        name: "Start".to_string(),
        kind: FunctionKind::Entry,
        owner: None,
        asynchronous: false,
        parameters: Vec::new(),
        weak_symbols: HashSet::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Constant {
                    destination: bn_ir::ValueId(0),
                    value: Constant::Integer("42".to_string()),
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
                Instruction::Store {
                    symbol: sym_x,
                    value: bn_ir::ValueId(0),
                    previous: None,
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
            ],
            terminator: Terminator::Return { value: None },
        }],
        span: dummy_span(),
    };

    let validated = build_validated_module(vec![start_fn]);
    let mut var_map = HashMap::new();
    var_map.insert("x".to_string(), sym_x);

    let mut input = Cursor::new(Vec::new());
    let mut output = Vec::new();

    let result = session
        .execute_submission(&validated, &var_map, &mut input, &mut output)
        .expect("turn execution succeeds");

    assert_eq!(result.exit_code, 0);
    assert!(matches!(
        session.variables().get("x"),
        Some(Value::Integer(42, _))
    ));
    assert!(matches!(
        result.updated_variables.get("x"),
        Some(Value::Integer(42, _))
    ));
}

#[test]
fn session_transactional_rollback_on_runtime_error() {
    let host = HostEnv::sandbox(Vec::new());
    let mut session = InteractiveSession::new(host);

    // Turn 1: x = 100
    let sym_x = SymbolId(0);
    let start1 = Function {
        name: "Start".to_string(),
        kind: FunctionKind::Entry,
        owner: None,
        asynchronous: false,
        parameters: Vec::new(),
        weak_symbols: HashSet::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Constant {
                    destination: bn_ir::ValueId(0),
                    value: Constant::Integer("100".to_string()),
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
                Instruction::Store {
                    symbol: sym_x,
                    value: bn_ir::ValueId(0),
                    previous: None,
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
            ],
            terminator: Terminator::Return { value: None },
        }],
        span: dummy_span(),
    };
    let val1 = build_validated_module(vec![start1]);
    let mut var_map1 = HashMap::new();
    var_map1.insert("x".to_string(), sym_x);

    let mut input = Cursor::new(Vec::new());
    let mut output = Vec::new();
    session
        .execute_submission(&val1, &var_map1, &mut input, &mut output)
        .unwrap();
    assert!(matches!(
        session.variables().get("x"),
        Some(Value::Integer(100, _))
    ));

    // Turn 2: attempts x = x / 0 (Division by zero!)
    let start2 = Function {
        name: "Start".to_string(),
        kind: FunctionKind::Entry,
        owner: None,
        asynchronous: false,
        parameters: Vec::new(),
        weak_symbols: HashSet::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Constant {
                    destination: bn_ir::ValueId(0),
                    value: Constant::Integer("0".to_string()),
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
                Instruction::Binary {
                    destination: bn_ir::ValueId(1),
                    operator: "DIV".to_string(),
                    left: bn_ir::ValueId(0),
                    right: bn_ir::ValueId(0),
                    ty: Type::Integer(IntegerType::Int32),
                    span: dummy_span(),
                },
            ],
            terminator: Terminator::Return { value: None },
        }],
        span: dummy_span(),
    };
    let val2 = build_validated_module(vec![start2]);

    let err = session
        .execute_submission(&val2, &var_map1, &mut input, &mut output)
        .expect_err("division by zero must return Err");

    assert_eq!(err.code, "DIVISION_BY_ZERO");
    // Verify x is still 100! State rolled back!
    assert!(matches!(
        session.variables().get("x"),
        Some(Value::Integer(100, _))
    ));
}

#[test]
fn session_cancel_aborts_running_turn() {
    let host = HostEnv::sandbox(Vec::new());
    let mut session = InteractiveSession::new(host);

    // Pre-cancel the turn
    session.cancel();

    let start = Function {
        name: "Start".to_string(),
        kind: FunctionKind::Entry,
        owner: None,
        asynchronous: false,
        parameters: Vec::new(),
        weak_symbols: HashSet::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Constant {
                destination: bn_ir::ValueId(0),
                value: Constant::Integer("1".to_string()),
                ty: Type::Integer(IntegerType::Int32),
                span: dummy_span(),
            }],
            terminator: Terminator::Return { value: None },
        }],
        span: dummy_span(),
    };
    let val = build_validated_module(vec![start]);
    let var_map = HashMap::new();

    let mut input = Cursor::new(Vec::new());
    let mut output = Vec::new();

    let err = session
        .execute_submission(&val, &var_map, &mut input, &mut output)
        .expect_err("cancelled turn must return Err");

    assert_eq!(err.code, "DEBUG_TERMINATED");
    assert!(!session.cancel_flag().load(Ordering::Relaxed));
}
