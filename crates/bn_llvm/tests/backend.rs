use bn_ir::{BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator, ValueId};
use bn_source::{Position, Revision, SourceId, Span};
use bn_types::{IntegerType, Type};

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

#[test]
fn llvm_crate_emits_a_validated_start_function() {
    let span = span();
    let module = Module {
        functions: vec![Function {
            name: "Start".into(),
            asynchronous: false,
            parameters: Vec::new(),
            return_type: Type::Integer(IntegerType::Int32),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(0),
                    value: Constant::Integer("0".into()),
                    ty: Type::Integer(IntegerType::Int32),
                    span,
                }],
                terminator: Terminator::Stop { code: ValueId(0) },
            }],
            span,
        }],
        ..Module::default()
    };
    let validated = bn_ir::validate_module(module).expect("validate module");
    let llvm =
        bn_llvm::lower_validated_module_for_target(&validated, false).expect("emit native LLVM");
    assert!(llvm.contains("define i32 @main"));
}
