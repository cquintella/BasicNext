use super::{Catalog, SpecIntegerType, SpecMember, SpecPointerLength, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_11(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.CIDR".into(),
        BTreeMap::from([
            (
                "Parse".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.CIDR".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Contains".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Named("HOST.Net.Address".into())],
                        return_type: Box::new(SpecType::Boolean),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Network".into(),
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
                "PrefixLength".into(),
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

pub(super) fn declare_12(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.Addresses".into(),
        BTreeMap::from([
            (
                "Count".into(),
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
                "Get".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Integer(SpecIntegerType::Int32)],
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
        ]),
    );
}

pub(super) fn declare_13(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.TCPStream".into(),
        BTreeMap::from([
            (
                "Close".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("VOID".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Read".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Pointer {
                                element: Box::new(SpecType::Integer(SpecIntegerType::Byte)),
                                length: SpecPointerLength::Dynamic,
                            },
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::EndOfFile,
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Write".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Pointer {
                                element: Box::new(SpecType::Integer(SpecIntegerType::Byte)),
                                length: SpecPointerLength::Dynamic,
                            },
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "LocalEndpoint".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "RemoteEndpoint".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
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

pub(super) fn declare_14(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.UDPSocket".into(),
        BTreeMap::from([
            (
                "Close".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("VOID".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "SendTo".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
                            SpecType::Pointer {
                                element: Box::new(SpecType::Integer(SpecIntegerType::Byte)),
                                length: SpecPointerLength::Dynamic,
                            },
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Receive".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.UDPPacket".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "LocalEndpoint".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
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

pub(super) fn declare_15(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.TCPListener".into(),
        BTreeMap::from([
            (
                "Accept".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Integer(SpecIntegerType::Int32)],
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
                "LocalEndpoint".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("HOST.Net.Endpoint".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Close".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("VOID".into()),
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

pub(super) fn declare_16(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Net.UDPPacket".into(),
        BTreeMap::from([
            (
                "Source".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Named("HOST.Net.Endpoint".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Size".into(),
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
                "Truncated".into(),
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
                "WasTruncated".into(),
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
                "CopyTo".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Pointer {
                                element: Box::new(SpecType::Integer(SpecIntegerType::Byte)),
                                length: SpecPointerLength::Dynamic,
                            },
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Integer(SpecIntegerType::Int32),
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
