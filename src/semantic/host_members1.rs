#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn declare_host_members_1(&mut self) {
        self.members.insert(
            "Error".into(),
            HashMap::from([
                (
                    "Code".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Message".into(),
                    Member {
                        ty: Type::String,
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
    }

    pub(crate) fn declare_host_members_2(&mut self) {
        self.members.insert(
            "HOST.Clock".into(),
            HashMap::from([
                (
                    "Now".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Timer".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
    }

    pub(crate) fn declare_host_members_3(&mut self) {
        self.members.insert(
            "HOST.Random".into(),
            HashMap::from([
                (
                    "Random".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Float(FloatType::Float64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Seed".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
    }

    pub(crate) fn declare_host_members_4(&mut self) {
        self.members.insert(
            "HOST.FileSystem".into(),
            HashMap::from([
                (
                    "File".into(),
                    Member {
                        ty: Type::TypeName("FS.File".into()),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Exists".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Boolean,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Open".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String, Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("FS.File".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "READ".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "WRITE".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "APPEND".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "DeleteFile".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
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

    #[allow(clippy::too_many_lines)]
    pub(crate) fn declare_host_members_5(&mut self) {
        self.members.insert(
            "FS.File".into(),
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
                    "ReadLine".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::String,
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
                    "ReadAll".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
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
                (
                    "ReadBytes".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Pointer {
                                element: Box::new(Type::Integer(IntegerType::Byte)),
                                length: PointerLength::Dynamic,
                            }],
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
                            parameters: vec![Type::String],
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
                    "WriteBytes".into(),
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
                    "WriteLine".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
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
                    "SetTimeouts".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Integer(IntegerType::Int32),
                            ],
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
                    "ShutdownRead".into(),
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
                    "ShutdownWrite".into(),
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

    pub(crate) fn declare_host_members_6(&mut self) {
        self.members.insert("Float".into(), HashMap::new());
    }
}
