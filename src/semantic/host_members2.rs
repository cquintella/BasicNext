#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn declare_host_members_7(&mut self) {
        self.members.insert(
            "HOST.Console".into(),
            HashMap::from([
                (
                    "Cls".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Beep".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "PrintAt".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Integer(IntegerType::Int32),
                                Type::String,
                            ],
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "NumCols".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int32)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "NumRows".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int32)),
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
    pub(crate) fn declare_host_members_8(&mut self) {
        self.members.insert(
            "HOST.Net".into(),
            HashMap::from([
                (
                    "Address".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.Address".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Endpoint".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.Endpoint".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "CIDR".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.CIDR".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Addresses".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.Addresses".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "TCPStream".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.TCPStream".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "TCPListener".into(),
                    Member {
                        ty: Type::TypeName("HOST.Net.TCPListener".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "TCPConnect".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Named("HOST.Net.Endpoint".into()),
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.TCPStream".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "TCPListen".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Vector {
                                    element: Box::new(Type::Named("HOST.Net.Endpoint".into())),
                                    dimensions: vec![u64::MAX],
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.TCPListener".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "UDPBind".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Named("HOST.Net.Endpoint".into())],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.UDPSocket".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Ping".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Named("HOST.Net.Address".into()),
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.PingReply".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Neighbor".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Named("HOST.Net.Address".into())],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Address".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Resolve".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String, Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Addresses".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Reverse".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Named("HOST.Net.Address".into()),
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::String,
                                Type::Named("Error".into()),
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
    pub(crate) fn declare_host_members_9(&mut self) {
        self.members.insert(
            "HOST.Net.Address".into(),
            HashMap::from([
                (
                    "Parse".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Address".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "ToString".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::String),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsIPv4".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsIPv6".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsLoopback".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsPrivate".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsLinkLocal".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "IsMulticast".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
    }

    pub(crate) fn declare_host_members_10(&mut self) {
        self.members.insert(
            "HOST.Net.Endpoint".into(),
            HashMap::from([
                (
                    "Create".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Named("HOST.Net.Address".into()),
                                Type::Integer(IntegerType::UInt16),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Endpoint".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Address".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("HOST.Net.Address".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Port".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::UInt16)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
    }
}
