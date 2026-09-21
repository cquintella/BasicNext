use bn_frontend::{lexer::lex, parser::parse, semantic::analyze, source::SourceFile};
use bn_ir::Instruction;

#[test]
fn frontend_exposes_the_shared_lowering_entrypoint() {
    let source = SourceFile::new("lowering.bn", "FUNCTION Start() AS VOID\nEND FUNCTION\n");
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower and validate source");
    assert!(
        module
            .as_module()
            .functions
            .iter()
            .any(|function| function.name == "Start")
    );
}

#[test]
fn lowering_emits_base_first_interned_field_layouts() {
    let source = SourceFile::new(
        "layouts.bn",
        "CLASS Parent\n    PUBLIC first AS INTEGER\nEND CLASS\n\nCLASS Child EXTENDS Parent\n    PUBLIC second AS INTEGER\nEND CLASS\n\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower and validate source");
    let module = module.as_module();
    let child = module.field_layouts.get("Child").expect("Child layout");

    assert_eq!(child.fields.len(), 2);
    assert_eq!(child.fields[0].slot.value(), 0);
    assert_eq!(
        module.field_names[usize::try_from(child.fields[0].id.value()).expect("small field ID")],
        "first"
    );
    assert_eq!(child.fields[0].declaring_owner, "Parent");
    assert_eq!(child.fields[1].slot.value(), 1);
    assert_eq!(
        module.field_names[usize::try_from(child.fields[1].id.value()).expect("small field ID")],
        "second"
    );
    assert_eq!(child.fields[1].declaring_owner, "Child");
    assert!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                Instruction::SetMember { name, field: Some(field), .. }
                    if name == "first" && field.slot.value() == 0
            ))
    );
}

#[test]
fn lowering_emits_layouts_for_host_provider_records() {
    let source = SourceFile::new(
        "host-layouts.bn",
        "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET result AS Exec.Result OR Error = Exec.Run(\"program\", [\"argument\"])\nIF result IS Exec.Result THEN\nPRINT result.ReturnCode\nEND IF\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower and validate source");
    let layout = module
        .as_module()
        .field_layouts
        .get("HOST.Exec.Result")
        .expect("HOST.Exec.Result layout");

    assert_eq!(layout.fields.len(), 3);
    assert_eq!(layout.fields[0].slot.value(), 0);
    assert_eq!(layout.fields[1].slot.value(), 1);
    assert_eq!(layout.fields[2].slot.value(), 2);
}

#[test]
fn lowering_resolves_nested_field_store_paths_to_slots() {
    let source = SourceFile::new(
        "nested-layouts.bn",
        "STRUCT Inner\n    N AS INTEGER\nEND STRUCT\n\nSTRUCT Outer\n    inner AS Inner\nEND STRUCT\n\nFUNCTION Start() AS VOID\n    LET outer AS Outer\n    outer.inner.N = 7\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower and validate source");
    let store = module
        .as_module()
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::SetField {
                root_owner,
                path,
                fields: Some(fields),
                ..
            } => Some((root_owner, path, fields)),
            _ => None,
        })
        .expect("nested field store");

    assert_eq!(store.0, "Outer");
    assert_eq!(store.1, &["inner", "N"]);
    assert_eq!(store.2.len(), 2);
    assert_eq!(store.2[0].owner, "Outer");
    assert_eq!(store.2[0].slot.value(), 0);
    assert_eq!(store.2[1].owner, "Inner");
    assert_eq!(store.2[1].slot.value(), 0);
}

#[test]
fn lowering_assigns_field_ids_deterministically() {
    let source = SourceFile::new(
        "deterministic-layouts.bn",
        "STRUCT Zebra\n    last AS INTEGER\nEND STRUCT\nSTRUCT Alpha\n    first AS INTEGER\nEND STRUCT\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let expected = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower reference module")
        .as_module()
        .field_names
        .clone();

    for _ in 0..16 {
        let actual = bn_frontend::lowering::lower_validated(&program, &model)
            .expect("repeat lowering")
            .as_module()
            .field_names
            .clone();
        assert_eq!(actual, expected);
    }
}
