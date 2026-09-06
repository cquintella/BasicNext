use super::{Catalog, SpecIntegerType, SpecMember, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_17(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.PingReply".into(),
        BTreeMap::from([
            (
                "Address".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Named("HOST.Net.Address".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "RoundTripMicroseconds".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Integer(SpecIntegerType::Int64)),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}
