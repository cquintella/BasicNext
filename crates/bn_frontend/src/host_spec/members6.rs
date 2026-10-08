use super::{Catalog, SpecMember, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_env(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Env".into(),
        BTreeMap::from([
            (
                "Get".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::String,
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Has".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Boolean,
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}
