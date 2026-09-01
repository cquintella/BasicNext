#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn declare_standard_members(&mut self) {
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
        self.members.insert(
            "HOST.Clock".into(),
            HashMap::from([
                (
                    "Timestamp".into(),
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
                    "Monotonic".into(),
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
        self.members.insert(
            "HOST.FileSystem".into(),
            HashMap::from([
                (
                    "File".into(),
                    Member {
                        ty: Type::TypeName("FS.File".into()),
                        is_static: true,
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
            ]),
        );
        self.members.insert("Float".into(), HashMap::new());
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
        for (namespace, result) in [
            ("Date", Type::Named("DATE".into())),
            ("Time", Type::Named("TIME".into())),
            ("TimeZone", Type::Named("TIMEZONE".into())),
            ("Timestamp", Type::Integer(IntegerType::Int64)),
        ] {
            self.members.insert(
                namespace.into(),
                HashMap::from([(
                    "Parse".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(result),
                        },
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                )]),
            );
        }
        self.members
            .get_mut("Timestamp")
            .expect("Timestamp namespace")
            .insert(
                "Format".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![Type::Integer(IntegerType::Int64)],
                        return_type: Box::new(Type::String),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            );
        self.declare_host_members_1();
        self.declare_host_members_2();
        self.declare_host_members_3();
        self.declare_host_members_4();
        self.declare_host_members_5();
        self.declare_host_members_6();
        self.declare_host_members_7();
        self.declare_host_members_8();
        self.declare_host_members_9();
        self.declare_host_members_10();
        self.declare_host_members_11();
        self.declare_host_members_12();
        self.declare_host_members_13();
        self.declare_host_members_14();
        self.declare_host_members_15();
        self.declare_host_members_16();
        self.declare_host_members_17();
    }
}
