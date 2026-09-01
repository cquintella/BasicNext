#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn declare_host_members_11(&mut self) {
        self.members.insert(
            "HOST.Net.CIDR".into(),
            HashMap::from([
                (
                    "Parse".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.CIDR".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Contains".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Named("HOST.Net.Address".into())],
                            return_type: Box::new(Type::Boolean),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Network".into(),
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
                    "PrefixLength".into(),
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

    pub(crate) fn declare_host_members_12(&mut self) {
        self.members.insert(
            "HOST.Net.Addresses".into(),
            HashMap::from([
                (
                    "Count".into(),
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
                    "Get".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Integer(IntegerType::Int32)],
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
            ]),
        );
    }

    pub(crate) fn declare_host_members_13(&mut self) {
        self.members.insert(
            "HOST.Net.TCPStream".into(),
            HashMap::from([
                (
                    "Close".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Read".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Pointer {
                                    element: Box::new(Type::Integer(IntegerType::Byte)),
                                    length: PointerLength::Dynamic,
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Integer(IntegerType::Int32),
                                Type::EndOfFile,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Write".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Pointer {
                                    element: Box::new(Type::Integer(IntegerType::Byte)),
                                    length: PointerLength::Dynamic,
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "LocalEndpoint".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Endpoint".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "RemoteEndpoint".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Endpoint".into()),
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

    pub(crate) fn declare_host_members_14(&mut self) {
        self.members.insert(
            "HOST.Net.UDPSocket".into(),
            HashMap::from([
                (
                    "Close".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "SendTo".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Named("HOST.Net.Endpoint".into()),
                                Type::Pointer {
                                    element: Box::new(Type::Integer(IntegerType::Byte)),
                                    length: PointerLength::Dynamic,
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Receive".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.UDPPacket".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "LocalEndpoint".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Endpoint".into()),
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

    pub(crate) fn declare_host_members_15(&mut self) {
        self.members.insert(
            "HOST.Net.TCPListener".into(),
            HashMap::from([
                (
                    "Accept".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Integer(IntegerType::Int32)],
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
                    "LocalEndpoint".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("HOST.Net.Endpoint".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Close".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
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

    pub(crate) fn declare_host_members_16(&mut self) {
        self.members.insert(
            "HOST.Net.UDPPacket".into(),
            HashMap::from([
                (
                    "Source".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("HOST.Net.Endpoint".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Size".into(),
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
                    "Truncated".into(),
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
                    "WasTruncated".into(),
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
                    "CopyTo".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Pointer {
                                    element: Box::new(Type::Integer(IntegerType::Byte)),
                                    length: PointerLength::Dynamic,
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Integer(IntegerType::Int32),
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
}
