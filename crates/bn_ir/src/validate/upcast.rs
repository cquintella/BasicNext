// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If applicable, see the MPL-2.0 license.

//! Upcast and signature compatibility validation for calls and returns.

use crate::Module;
use bn_types::Type;

/// Determines whether an actual argument type is compatible with the expected parameter type,
/// taking into account nominal upcasting (`ClassModel::is_upcast`).
#[must_use]
pub fn call_types_compatible_with_module(module: &Module, actual: &Type, expected: &Type) -> bool {
    call_types_compatible_inner(Some(module), actual, expected)
}

/// Fallback compatibility check without module context (used by tests or non-nominal types).
#[allow(dead_code)]
#[must_use]
pub fn call_types_compatible(actual: &Type, expected: &Type) -> bool {
    call_types_compatible_inner(None, actual, expected)
}

fn call_types_compatible_inner(module: Option<&Module>, actual: &Type, expected: &Type) -> bool {
    if types_compatible(actual, expected)
        || alternative_widens(actual, expected)
        || (is_numeric_type(actual) && is_numeric_type(expected))
    {
        return true;
    }

    if module.is_some_and(|module| is_upcast(module, actual, expected)) {
        return true;
    }

    // Standard-library reduction calls use a scalar declaration type for
    // the overloaded one-vector form. Semantic analysis has already
    // restricted that form to the catalogued BNMath functions; the IR
    // handoff preserves the numeric element/result contract here.
    if matches!(
        actual,
        Type::Vector { element, dimensions }
            if dimensions.len() == 1
                && is_numeric_type(element)
                && is_numeric_type(expected)
    ) {
        return true;
    }

    if let (
        Type::Vector {
            element: actual_element,
            dimensions: actual_dimensions,
        },
        Type::Vector {
            element: expected_element,
            dimensions: expected_dimensions,
        },
    ) = (actual, expected)
        && call_types_compatible_inner(module, actual_element, expected_element)
        && actual_dimensions.len() == expected_dimensions.len()
        && actual_dimensions.iter().zip(expected_dimensions).all(
            |(actual_dimension, expected_dimension)| {
                actual_dimension == expected_dimension
                    || *actual_dimension == u64::MAX
                    || *expected_dimension == u64::MAX
            },
        )
    {
        return true;
    }

    false
}

/// Whether `actual` is a class whose ancestors or their interfaces include the
/// nominal type `expected` (`ClassModel::is_upcast`). Shared by the argument
/// and result boundaries.
#[must_use]
pub fn is_upcast(module: &Module, actual: &Type, expected: &Type) -> bool {
    matches!(
        (nominal_name(actual), nominal_name(expected)),
        (Some(actual), Some(expected)) if module.class_model.is_upcast(&actual, &expected)
    )
}

fn nominal_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(name) => Some(name.clone()),
        Type::ImportedNamed { module, name } => Some(format!("#{}.{}", module.0, name)),
        _ => None,
    }
}

fn is_numeric_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::FloatLiteral
    )
}

fn is_void_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "VOID")
}

pub(crate) fn alternative_widens(actual: &Type, expected: &Type) -> bool {
    matches!(
        (actual, expected),
        (Type::Alternative(members), Type::Alternative(accepted))
            if members
                .iter()
                .all(|member| accepted.iter().any(|option| types_compatible(member, option)))
    )
}

fn alternative_sets_compatible(actual: &[Type], expected: &[Type]) -> bool {
    actual.len() == expected.len()
        && actual.iter().all(|actual_option| {
            expected
                .iter()
                .any(|expected_option| types_compatible(actual_option, expected_option))
        })
        && expected.iter().all(|expected_option| {
            actual
                .iter()
                .any(|actual_option| types_compatible(actual_option, expected_option))
        })
}

pub(crate) fn types_compatible(actual: &Type, expected: &Type) -> bool {
    actual == expected
        || matches!(
            (actual, expected),
            (Type::ImportedNamed { name: actual, .. }, Type::Named(expected))
                | (Type::Named(expected), Type::ImportedNamed { name: actual, .. })
                if expected.rsplit('.').next() == Some(actual.as_str())
        )
        || matches!(
            (actual, expected),
            (
                Type::ImportedNamed { name: actual_name, .. },
                Type::ImportedNamed {
                    name: expected_name,
                    ..
                }
            ) if actual_name == expected_name
        )
        || matches!(
            (actual, expected),
            (Type::Alternative(actual_options), Type::Alternative(expected_options))
                if alternative_sets_compatible(actual_options, expected_options)
        )
        || matches!(
            (actual, expected),
            (Type::IntegerLiteral(_), Type::Integer(_))
        )
        || matches!((actual, expected), (Type::FloatLiteral, Type::Float(_)))
        || matches!((actual, expected), (Type::Pointer { .. }, Type::Named(name)) if name == "POINTER")
        || matches!(
            (actual, expected),
            (
                Type::Pointer {
                    element: actual_element,
                    length: actual_length,
                },
                Type::Pointer {
                    element: expected_element,
                    length: expected_length,
                }
            ) if is_void_type(actual_element)
                || is_void_type(expected_element)
                || (types_compatible(actual_element, expected_element)
                    && (actual_length == expected_length
                        || matches!(expected_length, bn_types::PointerLength::Dynamic)))
        )
        || matches!(expected, Type::Alternative(options) if options.iter().any(|option| types_compatible(actual, option)))
        || matches!(
            (actual, expected),
            (
                Type::Vector {
                    element: actual_element,
                    dimensions: actual_dimensions,
                },
                Type::Vector {
                    element: expected_element,
                    dimensions: expected_dimensions,
                }
            ) if types_compatible(actual_element, expected_element)
                && actual_dimensions.len() == expected_dimensions.len()
                && actual_dimensions.iter().zip(expected_dimensions).all(
                    |(actual_dimension, expected_dimension)| {
                        actual_dimension == expected_dimension
                            || *actual_dimension == u64::MAX
                            || *expected_dimension == u64::MAX
                    }
                )
        )
        || matches!(
            (actual, expected),
            (
                Type::Pointer { element: actual_element, .. },
                Type::Vector {
                    element: expected_element,
                    dimensions,
                }
            ) if dimensions.len() == 1
                && is_numeric_type(actual_element)
                && is_numeric_type(expected_element)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Module;
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    fn make_test_module() -> Module {
        Module {
            source_name: None,
            functions: Vec::new(),
            field_names: Vec::new(),
            field_layouts: BTreeMap::new(),
            class_model: bn_types::class_model::ClassModel::default(),
            interfaces: BTreeSet::new(),
            bndata_providers: HashSet::new(),
            bnmath_providers: HashSet::new(),
            bnlog_providers: HashSet::new(),
            bnjson_providers: HashSet::new(),
            bnweb_providers: HashSet::new(),
            bndispatch_providers: HashSet::new(),
            bncrypto_providers: HashSet::new(),
            bnsqlite_providers: HashSet::new(),
            filesystem_import: None,
            clock_import: None,
            random_import: None,
            console_import: None,
            network_import: None,
            exec_import: None,
            env_import: None,
            bnlog_import: None,
            bnweb_import: None,
            bnsqlite_import: None,
        }
    }

    /// Declares `classes` as valid IR (a `$fields` prologue and an empty
    /// layout each), so a test fails only on the rule it exercises.
    fn declare_classes(
        module: &mut Module,
        classes: &[&str],
        span: bn_source::Span,
    ) -> Vec<crate::Function> {
        use crate::{
            BasicBlock, BlockId, FieldLayout, Function, FunctionKind, SymbolId, Terminator,
        };

        let mut functions = Vec::new();
        for class in classes {
            module.field_layouts.insert(
                (*class).to_string(),
                FieldLayout {
                    owner: (*class).into(),
                    fields: Vec::new(),
                    span,
                },
            );
            functions.push(Function {
                name: format!("{class}.$fields"),
                kind: FunctionKind::FieldInit,
                owner: Some((*class).into()),
                asynchronous: false,
                parameters: vec![SymbolId(0)],
                weak_symbols: HashSet::new(),
                return_type: Type::Named("VOID".into()),
                entry: BlockId(0),
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: Vec::new(),
                    terminator: Terminator::Return { value: None },
                }],
                span,
            });
        }
        functions
    }

    #[test]
    fn upcast_argument_accepted_by_call_types_compatible() {
        let mut module = make_test_module();
        module.class_model.add_base("Dog", "Animal");
        module.class_model.add_base("Puppy", "Dog");

        let dog_ty = Type::Named("Dog".into());
        let animal_ty = Type::Named("Animal".into());
        let puppy_ty = Type::Named("Puppy".into());
        let unrelated_ty = Type::Named("Car".into());

        assert!(call_types_compatible_with_module(
            &module, &dog_ty, &animal_ty
        ));
        assert!(call_types_compatible_with_module(
            &module, &puppy_ty, &animal_ty
        ));
        assert!(call_types_compatible_with_module(
            &module, &puppy_ty, &dog_ty
        ));
        assert!(!call_types_compatible_with_module(
            &module, &animal_ty, &dog_ty
        ));
        assert!(!call_types_compatible_with_module(
            &module,
            &unrelated_ty,
            &animal_ty
        ));
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn validation_rejects_argument_whose_class_is_not_an_upcast() {
        use crate::{
            BasicBlock, BlockId, Constant, Function, FunctionKind, Instruction, SymbolId,
            Terminator, ValueId,
        };
        use bn_source::{Position, Revision, SourceId, Span};

        let span = Span {
            start: Position {
                source_id: SourceId(1),
                revision: Revision(1),
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Position {
                source_id: SourceId(1),
                revision: Revision(1),
                offset: 0,
                line: 1,
                column: 1,
            },
        };

        let mut module = make_test_module();
        module.class_model.add_base("Dog", "Animal");
        let mut functions = declare_classes(&mut module, &["Animal", "Dog", "Car"], span);

        // Callee: TakeAnimal(a AS Animal) -> VOID
        let callee = Function {
            name: "TakeAnimal".into(),
            kind: FunctionKind::User,
            owner: None,
            asynchronous: false,
            parameters: vec![SymbolId(0)],
            weak_symbols: HashSet::new(),
            return_type: Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: None },
            }],
            span,
        };

        // Caller calls TakeAnimal with an unrelated class "Car"
        let caller = Function {
            name: "Start".into(),
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
                    Instruction::Allocate {
                        destination: ValueId(0),
                        type_name: "Car".into(),
                        arguments: Vec::new(),
                        ty: Type::Named("Car".into()),
                        span,
                    },
                    Instruction::Constant {
                        destination: ValueId(1),
                        value: Constant::Function("TakeAnimal".into()),
                        ty: Type::Function {
                            parameters: vec![Type::Named("Animal".into())],
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        span,
                    },
                    Instruction::Call {
                        destination: ValueId(2),
                        callee: ValueId(1),
                        arguments: vec![ValueId(0)],
                        ty: Type::Named("VOID".into()),
                        span,
                    },
                    Instruction::Release {
                        value: ValueId(0),
                        destructor: None,
                        span,
                    },
                ],
                terminator: Terminator::Return { value: None },
            }],
            span,
        };

        functions.extend([callee, caller]);
        module.functions = functions;
        let result = crate::validate(&module);
        assert!(
            result.is_err(),
            "validation should reject non-upcast argument"
        );
        let diag = result.unwrap_err();
        assert_eq!(diag.code, "INVALID_IR");
        assert!(diag.message.contains("call arguments"), "{}", diag.message);
    }

    /// A function returning `Animal` accepts a `Dog` value (upcast) and
    /// rejects an unrelated `Car`, through the same rule as arguments.
    #[test]
    fn return_accepts_an_upcast_and_rejects_an_unrelated_class() {
        use crate::{
            BasicBlock, BlockId, Function, FunctionKind, Instruction, Terminator, ValueId,
        };
        use bn_source::{Position, Revision, SourceId, Span};

        let position = Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset: 0,
            line: 1,
            column: 1,
        };
        let span = Span {
            start: position,
            end: position,
        };
        let returning = |class: &str| Function {
            name: "Make".into(),
            kind: FunctionKind::User,
            owner: None,
            asynchronous: false,
            parameters: Vec::new(),
            weak_symbols: HashSet::new(),
            return_type: Type::Named("Animal".into()),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Allocate {
                    destination: ValueId(0),
                    type_name: class.into(),
                    arguments: Vec::new(),
                    ty: Type::Named(class.into()),
                    span,
                }],
                terminator: Terminator::Return {
                    value: Some(ValueId(0)),
                },
            }],
            span,
        };

        let mut module = make_test_module();
        module.class_model.add_base("Dog", "Animal");
        let classes = declare_classes(&mut module, &["Animal", "Dog", "Car"], span);
        let with = |made: &str| {
            let mut functions = classes.clone();
            functions.push(returning(made));
            functions
        };
        module.functions = with("Dog");
        assert!(
            crate::validate(&module).is_ok(),
            "{:?}",
            crate::validate(&module)
        );

        module.functions = with("Car");
        let diagnostic = crate::validate(&module).expect_err("unrelated class returned");
        assert_eq!(diagnostic.code, "INVALID_IR");
    }
}
