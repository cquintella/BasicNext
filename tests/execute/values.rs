// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;

#[test]
fn inferred_constants_and_typeof_use_canonical_types() {
    let source = r#"FUNCTION Start() AS VOID
CONST integer_value = 5
CONST float_value = 10.2
CONST boolean_value = TRUE
CONST string_value = "SS"
PRINT TYPEOF(integer_value), TYPEOF(float_value), TYPEOF(boolean_value), TYPEOF(string_value)
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute inferred constants and TYPEOF");
    assert_eq!(output, "INT32 FLOAT64 BOOLEAN STRING\n");
}

#[test]
fn executes_bnpallet_exported_color_constants() {
    let (_, output) = run_path("tests/grammar/valid/bnpallet.bn").expect("execute BNPallet");
    assert_eq!(output, "255 16711680 6697881\n");
}

#[test]
fn floating_division_uses_ieee_special_values() {
    let source = "FUNCTION Start() AS VOID\nPRINT 1 / 0, 0 / 0\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "INF NAN\n");
}

#[test]
fn checked_integer_overflow_is_a_runtime_error() {
    let source = "FUNCTION Start() AS VOID\nLET value AS INT8 = 127\nvalue += 1\nEND FUNCTION\n";
    let error = run(source, "").expect_err("INT8 overflow must fail");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn executes_vectors_foreach_input_and_math() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS INTEGER[3] = [1, 2, 3]\nLET total AS INTEGER = 0\nFOR EACH item AS INTEGER IN values\ntotal += item\nEND FOR\nLET line AS STRING OR EOF = INPUT()\nPRINT total, Math.SQRT(9.0), line\nEND FUNCTION\n";
    let (_, output) = run(source, "ready\n").expect("execute source");
    assert_eq!(output, "6 3.0 ready\n");
}

#[test]
fn inherited_fields_upcasts_and_virtual_methods_execute() {
    let source = r#"
CLASS Animal
    PUBLIC name AS STRING = "animal"
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "A"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        SUPER()
    END FUNCTION
    PUBLIC OVERRIDE FUNCTION Speak() AS STRING
        RETURN SUPER.Speak() + "D"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Animal = NEW Dog()
    PRINT pet.name, pet.Speak()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute inheritance");
    assert_eq!(output, "animal AD\n");
}

#[test]
fn inherited_members_survive_a_three_class_chain() {
    let source = r#"
CLASS Animal
    PUBLIC species AS STRING = "animal"
END CLASS

CLASS Mammal EXTENDS Animal
END CLASS

CLASS Dog EXTENDS Mammal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET animal AS Animal = NEW Dog()
    PRINT animal.species
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute inherited field chain");
    assert_eq!(code, 0);
    assert_eq!(output, "animal\n");
}

#[test]
fn inherited_static_members_keep_the_base_storage() {
    let source = r"
CLASS Animal
    PUBLIC STATIC count AS INTEGER = 1
END CLASS

CLASS Dog EXTENDS Animal
END CLASS

FUNCTION Start() AS VOID
    Dog.count += 1
    PRINT Animal.count, Dog.count
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute inherited STATIC field");
    assert_eq!(output, "2 2\n");
}

#[test]
fn super_method_uses_the_nearest_declaring_ancestor() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Mammal EXTENDS Animal
    PUBLIC OVERRIDE FUNCTION Speak() AS STRING
        RETURN "mammal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Mammal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC OVERRIDE FUNCTION Speak() AS STRING
        RETURN SUPER.Speak()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET dog AS Dog = NEW Dog()
    PRINT dog.Speak()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute nearest SUPER method");
    assert_eq!(output, "mammal\n");
}

#[test]
fn inherited_destructors_run_from_derived_to_base() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    RELEASE pet
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute destructor chain");
    assert_eq!(output, "dog\nanimal\n");
}

#[test]
fn derived_constructor_implicitly_calls_a_parameterless_base_constructor() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute implicit SUPER");
    assert_eq!(output, "animal\ndog\n");
}

#[test]
fn constructor_and_destructor_dispatch_are_pinned_to_the_running_class() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT SELF.Name()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT SELF.Name()
    END FUNCTION

    PUBLIC FUNCTION Name() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC OVERRIDE FUNCTION Name() AS STRING
        RETURN "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    RELEASE pet
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute lifecycle dispatch");
    assert_eq!(output, "animal\nanimal\n");
}

#[test]
fn derived_field_initializers_run_after_the_base_constructor() {
    let source = r#"
CLASS Animal
    PUBLIC name AS STRING = "unset"
    PUBLIC FUNCTION CONSTRUCTOR()
        SELF.name = "ok"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC label AS STRING = SELF.name
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    PRINT pet.label
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute derived field initializer order");
    assert_eq!(output, "ok\n");
}

#[test]
fn field_initializer_dispatch_is_pinned_to_the_running_class() {
    let source = r#"
CLASS Animal
    PUBLIC tag AS STRING = SELF.Kind()
    PUBLIC FUNCTION Kind() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
    PUBLIC OVERRIDE FUNCTION Kind() AS STRING
        RETURN "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    PRINT pet.tag
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute field initializer dispatch");
    assert_eq!(output, "animal\n");
}

#[test]
fn primitive_and_vector_bindings_receive_defaults() {
    let source = "FUNCTION Start() AS VOID\nLET count AS INTEGER\nLET ready AS BOOLEAN\nLET text AS STRING\nLET values AS INTEGER[2]\nPRINT count, ready, text, values[1]\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "0 FALSE  0\n");
}

#[test]
fn local_vector_dimensions_are_evaluated_once_and_stay_fixed() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = 2
    LET values AS INTEGER[count]
    count = 4
    PRINT LEN(values)
    values = [7, 8]
    PRINT values[0], values[1]
END FUNCTION
";
    let (_, output) = run(source, "").expect("declaration-time vector dimension");
    assert_eq!(output, "2\n7 8\n");
}

#[test]
fn local_vector_dimension_rejections_are_runtime_errors() {
    for (text, code) in [
        (
            "FUNCTION Start() AS VOID\nLET count AS INTEGER = -1\nLET values AS INTEGER[count]\nEND FUNCTION\n",
            "INVALID_VECTOR_DIMENSION",
        ),
        (
            "FUNCTION Start() AS VOID\nLET count AS UINT64 = 9223372036854775808\nLET values AS INTEGER[count]\nEND FUNCTION\n",
            "NUMERIC_OVERFLOW",
        ),
        (
            "FUNCTION Start() AS VOID\nLET count AS INTEGER = 2\nLET values AS INTEGER[count]\nvalues = [1]\nEND FUNCTION\n",
            "VECTOR_LENGTH_MISMATCH",
        ),
    ] {
        assert_eq!(
            run(text, "")
                .expect_err("invalid runtime vector dimension must fail")
                .code,
            code
        );
    }
}

#[test]
fn static_fields_use_type_defaults_when_initializer_is_omitted() {
    let source = include_str!("../grammar/valid/static-field-type-default.bn");
    let (code, output) = run(source, "").expect("execute default STATIC fields");
    assert_eq!(code, 0);
    assert_eq!(output, "0\n\nFALSE\n10\n");
}

#[test]
fn initializes_static_fields_once_in_source_order() {
    let (code, output) = run_path("tests/modules/statics/main.bn").expect("execute statics");
    assert_eq!(code, 0);
    assert_eq!(output, "1\n2\n");
}

#[test]
fn static_initialization_cycle_is_a_runtime_error() {
    let error = run_path("tests/modules/static-cycle/main.bn").expect_err("static cycle must fail");
    assert_eq!(error.code, "STATIC_INITIALIZATION_CYCLE");
}

#[test]
fn executes_len_and_sizeof_counts_and_byte_sizes() {
    let source = include_str!("../grammar/valid/len-and-sizeof.bn");
    let (code, output) = run(source, "").expect("execute LEN and SIZEOF");
    assert_eq!(code, 0);
    assert_eq!(output, "1 4\n1 8\n4 5\n3 12\n6 24\n2 3 0\n");
}

#[test]
fn executes_struct_copy_class_identity_and_interface_dispatch() {
    let source = r"
STRUCT Inner
    N AS INTEGER = 1
END STRUCT

STRUCT Point
    X AS FLOAT = 0.0
    Y AS FLOAT = 0.0
END STRUCT

INTERFACE Valued
    FUNCTION Value() AS INTEGER
END INTERFACE

CLASS Counter IMPLEMENTS Valued
    PRIVATE value AS INTEGER = 0

    PUBLIC FUNCTION CONSTRUCTOR(initial AS INTEGER)
        SELF.value = initial
    END FUNCTION

    PUBLIC FUNCTION Value() AS INTEGER
        RETURN SELF.value
    END FUNCTION

    PUBLIC FUNCTION Increment(amount AS INTEGER) AS VOID
        SELF.value += amount
    END FUNCTION
END CLASS

FUNCTION Mutate(point AS Point) AS VOID
    point.X = 99.0
END FUNCTION

FUNCTION Start() AS VOID
    LET origin AS Point
    LET moved AS Point = origin
    moved.X = 10.0
    Mutate(origin)
    LET nested AS Inner
    LET copy AS Inner = nested
    copy.N = 2
    LET points AS Point[2]
    PRINT origin.X, moved.X, nested.N, copy.N
    PRINT origin = moved, origin = origin, points[0].X, SIZEOF(origin)

    LET counter AS Counter = NEW Counter(1)
    LET alias AS Counter = counter
    alias.Increment(1)
    LET valued AS Valued = counter
    LET counters AS Counter[1] = [counter]
    PRINT counter.Value(), valued.Value(), counters[0].Value()
    PRINT counter = alias, counter = NEW Counter(2)
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute struct/class/interface program");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "0.0 10.0 1 2\nFALSE TRUE 0.0 16\n2 2 2\nTRUE FALSE\n"
    );
}

#[test]
fn failed_constructor_does_not_expose_an_object() {
    let source = r"
CLASS Boom
    PUBLIC FUNCTION CONSTRUCTOR()
        LET value AS INT8 = 127
        value += 1
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET boom AS Boom = NEW Boom()
END FUNCTION
";
    let error = run(source, "").expect_err("constructor overflow must fail");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn executes_imported_class_constructor_and_methods() {
    let (code, output) = run_path("tests/modules/objects/main.bn").expect("execute imported Box");
    assert_eq!(code, 0);
    assert_eq!(output, "7 7 1\n");
}

#[test]
fn executes_nested_record_store_inside_imported_module() {
    let (code, output) = run_path("tests/modules/imported-nested-record/main.bn")
        .expect("execute imported nested record store");
    assert_eq!(code, 0);
    assert_eq!(output, "7\n");
}

#[test]
fn executes_a_qualified_imported_interface() {
    let (code, output) = run_path("tests/modules/qualified-interface/main.bn")
        .expect("execute qualified imported interface");
    assert_eq!(code, 0);
    assert_eq!(output, "Puck\n");
}

#[test]
fn executes_a_class_derived_from_an_imported_base() {
    let (code, output) = run_path("tests/modules/imported-inheritance/main.bn")
        .expect("execute imported class inheritance");
    assert_eq!(code, 0);
    assert_eq!(output, "animal animal dog\n");
}

#[test]
fn executes_numeric_pointers_and_aliases() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    value[0] = 42
    LET alias AS POINTER TO INTEGER = value
    PRINT alias[0]
    RELEASE alias
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute pointer alias");
    assert_eq!(code, 0);
    assert_eq!(output, "42\n");
}

#[test]
fn fixed_pointer_region_is_initialized_and_bounds_checked() {
    let source = r"
FUNCTION Start() AS VOID
    LET samples AS POINTER TO FLOAT[4] = NEW FLOAT[4]
    samples[0] = 1.5
    samples[3] = 2.0
    PRINT samples[0], samples[3]
    RELEASE samples
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute fixed pointer");
    assert_eq!(code, 0);
    assert_eq!(output, "1.5 2.0\n");
}

#[test]
fn zero_length_pointer_rejects_every_index() {
    let source = r"
FUNCTION Start() AS VOID
    LET empty AS POINTER TO INTEGER[] = NEW INTEGER[0]
    PRINT empty[0]
END FUNCTION
";
    let error = run(source, "").expect_err("empty region has no element");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn released_pointer_binding_leaves_other_aliases_live() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    LET alias AS POINTER TO INTEGER = value
    RELEASE value
    alias[0] = 1
    PRINT alias[0]
END FUNCTION
";
    // 0.5 (Carlos lock 2026-09-16): RELEASE drops one strong on `value` only;
    // `alias` keeps the region alive.
    let (code, output) = run(source, "").expect("alias survives release of one binding");
    assert_eq!(code, 0);
    assert_eq!(output, "1\n");
}

#[test]
fn second_delete_is_double_delete() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    RELEASE value
    RELEASE value
END FUNCTION
";
    let error = run(source, "").expect_err("second delete must fail");
    assert_eq!(error.code, "DOUBLE_RELEASE");
}

#[test]
fn null_pointer_access_and_delete_are_diagnosed() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER OR NULL = NULL
    value[0] = 1
END FUNCTION
";
    let error = run(source, "").expect_err("NULL index must fail");
    assert_eq!(error.code, "NULL_POINTER_ACCESS");
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER OR NULL = NULL
    RELEASE value
    PRINT value IS NULL
END FUNCTION
";
    // A NULL binding holds no region: RELEASE ends the binding only, and any
    // later use is use-after-release.
    let error = run(source, "").expect_err("released NULL binding is unusable");
    assert_eq!(error.code, "USE_AFTER_RELEASE");
}

#[test]
fn computed_pointer_length_mismatch_is_a_runtime_error() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = 3
    LET fixed AS POINTER TO INTEGER[2] = NEW INTEGER[count]
END FUNCTION
";
    let error = run(source, "").expect_err("length mismatch must fail");
    assert_eq!(error.code, "POINTER_LENGTH_MISMATCH");
}

#[test]
fn negative_allocation_count_is_invalid() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = -1
    LET samples AS POINTER TO INTEGER[] = NEW INTEGER[count]
END FUNCTION
";
    let error = run(source, "").expect_err("negative count must fail");
    assert_eq!(error.code, "ALLOCATION_SIZE_INVALID");
}

#[test]
fn class_delete_runs_the_destructor_once() {
    let source = r#"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "destroyed"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    RELEASE box
    PRINT "after"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute destructor");
    assert_eq!(code, 0);
    assert_eq!(output, "destroyed\nafter\n");
}

#[test]
fn arc_returned_object_remains_live_until_caller_scope_exit() {
    let source = include_str!("../grammar/valid/arc-returned-object.bn");
    let (code, output) = run(source, "").expect("returned object remains live");
    assert_eq!(code, 0);
    assert_eq!(output, "VALUE 7\nDEINIT\n");
}

#[test]
fn arc_vector_aliases_release_each_ownership_without_double_free() {
    let source = include_str!("../grammar/valid/arc-vector-aliases.bn");
    let (code, output) = run(source, "").expect("aggregate aliases retain owner");
    assert_eq!(code, 0);
    assert_eq!(output, "VALUE 7\nDEINIT\n");
}

#[test]
fn arc_weak_reference_survives_unrelated_allocation_then_expires() {
    let source = include_str!("../grammar/valid/arc-weak-survives-unrelated-allocation.bn");
    let (code, output) = run(source, "").expect("weak observer follows its object");
    assert_eq!(code, 0);
    assert_eq!(output, "LIVE 7\nEXPIRED\n");
}

#[test]
fn arc_pointer_alias_survives_release_of_one_binding() {
    let source = include_str!("../grammar/valid/arc-pointer-alias-release.bn");
    let (code, output) = run(source, "").expect("F14: alias keeps the region alive");
    assert_eq!(code, 0);
    assert_eq!(output, "7\n3\n7\n7\nDONE\n");
}

#[test]
fn arc_released_pointer_binding_rejects_index_len_and_pass() {
    for tail in ["PRINT p[0]", "PRINT LEN(p)", "PRINT Peek(p)"] {
        let source = format!(
            "FUNCTION Peek(q AS POINTER TO INTEGER[]) AS INTEGER\n    RETURN q[0]\nEND FUNCTION\nFUNCTION Start() AS VOID\n    LET p AS POINTER TO INTEGER[] = NEW INTEGER[2]\n    RELEASE p\n    {tail}\nEND FUNCTION\n"
        );
        let error = run(&source, "").expect_err(tail);
        assert_eq!(error.code, "USE_AFTER_RELEASE", "{tail}");
    }
}

#[test]
fn arc_pointer_binding_released_twice_is_double_release() {
    let source = "FUNCTION Start() AS VOID\n    LET p AS POINTER TO INTEGER[] = NEW INTEGER[2]\n    RELEASE p\n    RELEASE p\nEND FUNCTION\n";
    let error = run(source, "").expect_err("F15: double release");
    assert_eq!(error.code, "DOUBLE_RELEASE");
}

#[test]
fn release_unused_primary_ends_binding_without_trapping() {
    let source = include_str!("../grammar/valid/release-unused-primary.bn");
    let (code, output) = run(source, "").expect("unused released primary is valid");
    assert_eq!(code, 0);
    assert_eq!(output, "OK\n");
}

#[test]
fn class_scope_exit_runs_its_destructor() {
    let source = r#"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "destroyed"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    PRINT "end"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute scope cleanup");
    assert_eq!(code, 0);
    assert_eq!(output, "end\ndestroyed\n");
}

#[test]
fn release_of_self_in_a_destructor_ends_only_the_binding() {
    // SELF is the receiver parameter: RELEASE ends the binding and owns no
    // reference (0.6.md, "`RELEASE`"), so the object is destroyed once; a
    // second RELEASE SELF is a double release.
    let source = r#"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "gone"
        RELEASE SELF
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    RELEASE box
    PRINT "end"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("RELEASE SELF ends the binding");
    assert_eq!((code, output.as_str()), (0, "gone\nend\n"));
    let reused = source.replace(
        "        RELEASE SELF\n",
        "        RELEASE SELF\n        RELEASE SELF\n",
    );
    let error = run(&reused, "").expect_err("second RELEASE SELF must fail");
    assert_eq!(error.code, "DOUBLE_RELEASE");
}

/// `IS` on a class of a BN-written standard module through its alias
/// (`Str.Tokenizer`): the test uses the class identity its objects carry
/// (known issue K41; `TRUE` was impossible before).
#[test]
fn is_names_an_imported_bn_class_through_its_alias() {
    let source = r#"IMPORT BNString AS Str
FUNCTION Start() AS VOID
LET a AS Str.Tokenizer OR NULL = Str.Tokenizer.New("a&b", "&")
LET b AS Str.Tokenizer OR NULL = NULL
PRINT a IS NULL, a IS Str.Tokenizer, b IS NULL, b IS Str.Tokenizer
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute IS on an imported BN class");
    assert_eq!(output, "FALSE TRUE TRUE FALSE\n");
}

#[test]
fn a_destroyed_object_runs_its_destructors_then_releases_fields_derived_first() {
    // 0.6.md, "Zero strong → destructor": the destructor chain (most derived
    // first), then the strong fields in reverse declaration order, derived
    // fields before base fields (the generated `$release` functions).
    let source = r#"
CLASS Box
    PUBLIC label AS STRING

    PUBLIC FUNCTION CONSTRUCTOR(label AS STRING)
        SELF.label = label
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "box " + SELF.label
    END FUNCTION
END CLASS

CLASS Base
    PUBLIC first AS Box = NEW Box("base first")
    PUBLIC second AS Box = NEW Box("base second")

    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "base destructor"
    END FUNCTION
END CLASS

CLASS Derived EXTENDS Base
    PUBLIC third AS Box = NEW Box("derived first")
    PUBLIC fourth AS Box = NEW Box("derived second")

    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "derived destructor"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET d AS Derived = NEW Derived()
    PRINT "end", d.first.label
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("destruction order");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "end base first
derived destructor
base destructor
box derived second
box derived first
box base second
box base first
"
    );
}
