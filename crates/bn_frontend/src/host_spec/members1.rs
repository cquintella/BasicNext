use super::{Catalog, SpecFloatType, SpecIntegerType, SpecMember, SpecPointerLength, SpecType};
use std::collections::BTreeMap;

pub(super) fn declare_1(catalog: &mut Catalog) {
    catalog.members.insert(
        "Error".into(),
        BTreeMap::from([
            (
                "Code".into(),
                SpecMember {
                    ty: SpecType::Integer(SpecIntegerType::Int32),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Message".into(),
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

pub(super) fn declare_2(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Clock".into(),
        BTreeMap::from([
            (
                "Now".into(),
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
            (
                "Timer".into(),
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

pub(super) fn declare_3(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.Random".into(),
        BTreeMap::from([
            (
                "Random".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Float(SpecFloatType::Float64)),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Seed".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Integer(SpecIntegerType::Int32)],
                        return_type: Box::new(SpecType::Named("VOID".into())),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
        ]),
    );
}

pub(super) fn declare_4(catalog: &mut Catalog) {
    catalog.members.insert(
        "HOST.FileSystem".into(),
        BTreeMap::from([
            (
                "File".into(),
                SpecMember {
                    ty: SpecType::TypeName("FS.File".into()),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "Exists".into(),
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
            (
                "Open".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::String,
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::Named("FS.File".into()),
                            SpecType::Named("Error".into()),
                        ])),
                    },
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "READ".into(),
                SpecMember {
                    ty: SpecType::Integer(SpecIntegerType::Int32),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "WRITE".into(),
                SpecMember {
                    ty: SpecType::Integer(SpecIntegerType::Int32),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "APPEND".into(),
                SpecMember {
                    ty: SpecType::Integer(SpecIntegerType::Int32),
                    is_static: false,
                    private: false,
                    mutable: false,
                },
            ),
            (
                "DeleteFile".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
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

#[allow(clippy::too_many_lines)]
pub(super) fn declare_5(catalog: &mut Catalog) {
    catalog.members.insert(
        "FS.File".into(),
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
                "ReadLine".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
                        return_type: Box::new(SpecType::Alternative(vec![
                            SpecType::String,
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
                "ReadAll".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: Vec::new(),
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
                "ReadBytes".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::Pointer {
                            element: Box::new(SpecType::Integer(SpecIntegerType::Byte)),
                            length: SpecPointerLength::Dynamic,
                        }],
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
                        parameters: vec![SpecType::String],
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
                "WriteBytes".into(),
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
                "WriteLine".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![SpecType::String],
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
                "SetTimeouts".into(),
                SpecMember {
                    ty: SpecType::Function {
                        parameters: vec![
                            SpecType::Integer(SpecIntegerType::Int32),
                            SpecType::Integer(SpecIntegerType::Int32),
                        ],
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
                "ShutdownRead".into(),
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
                "ShutdownWrite".into(),
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

pub(super) fn declare_6(catalog: &mut Catalog) {
    catalog.members.insert("Float".into(), BTreeMap::new());
}
