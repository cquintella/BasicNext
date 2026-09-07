use super::{Catalog, SpecIntegerType, SpecMember, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_7(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Console".into(),
        BTreeMap::from([
            (
                "Cls".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Named("VOID".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Beep".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Named("VOID".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "PrintAt".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::String,
                        ],
                        return_type: Box::new(SpecType::Named("VOID".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "NumCols".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Integer(SpecIntegerType::Int32)),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "NumRows".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Integer(SpecIntegerType::Int32)),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}

#[allow(clippy::too_many_lines)]
pub(super) fn declare_8(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net".into(),
        BTreeMap::from([
            (
                "Address".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.Address".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Endpoint".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.Endpoint".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "CIDR".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.CIDR".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Addresses".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.Addresses".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "TCPStream".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.TCPStream".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "TCPListener".into(),
                SpecMember {
                    ty: SpecType::TypeName("HOST.Net.TCPListener".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "TCPConnect".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.TCPStream".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "TCPListen".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Vector {
                                element: Box::new(SpecType::Named("HOST.Net.Endpoint".into())),
                                dimensions: vec![u64::MAX],
                            },
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.TCPListener".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "UDPBind".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Named("HOST.Net.Endpoint".into())],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.UDPSocket".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Ping".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Named("HOST.Net.Address".into()),
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.PingReply".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Neighbor".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Named("HOST.Net.Address".into())],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Address".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Resolve".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::String,
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Addresses".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Reverse".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Named("HOST.Net.Address".into()),
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
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
        ]),
    );
}

#[allow(clippy::too_many_lines)]
pub(super) fn declare_9(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.Address".into(),
        BTreeMap::from([
            (
                "Parse".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Address".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "ToString".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::String),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsIPv4".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsIPv6".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsLoopback".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsPrivate".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsLinkLocal".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "IsMulticast".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}

pub(super) fn declare_10(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.Endpoint".into(),
        BTreeMap::from([
            (
                "Create".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Named("HOST.Net.Address".into()),
                            SpecType::Integer(SpecIntegerType::UInt16),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            ),
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
                "Port".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Integer(SpecIntegerType::UInt16)),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}
