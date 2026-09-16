use super::{Catalog, SpecIntegerType, SpecMember, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_exec(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Exec".into(),
        BTreeMap::from([(
            "Run".into(),
            SpecMember {
                ty: SpecType::Function {
                    parameters: vec![
                        SpecType::String,
                        SpecType::Vector {
                            element: Box::new(SpecType::String),
                            dimensions: vec![u64::MAX],
                        },
                    ],
                    return_type: Box::new(SpecType::Alternative(vec![
                        SpecType::Named("HOST.Exec.Result".into()),
                        SpecType::Named("Error".into()),
                    ])),
                },
                is_static: false,
                private: false,
                mutable: false,
            },
        )]),
    );
    catalog.members.insert(
        "HOST.Exec.Result".into(),
        BTreeMap::from([
            (
                "ReturnCode".into(),
                SpecMember {
                    ty: SpecType::Integer(SpecIntegerType::Int64),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Stdout".into(),
                SpecMember {
                    ty: SpecType::String,
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Stderr".into(),
                SpecMember {
                    ty: SpecType::String,
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}
