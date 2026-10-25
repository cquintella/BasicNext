#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn declare_standard_members(&mut self) {
        // `Error` members come from the HOST catalog (`host_spec`) below.
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
                        visibility: MemberVisibility::Public,
                        overrides: false,
                        span: default_span(),
                        mutable: false,
                    },
                )]),
            );
        }
        self.members
            .get_mut("TimeZone")
            .expect("TimeZone namespace")
            .insert(
                "Offset".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![
                            Type::Named("TIMEZONE".into()),
                            Type::Integer(IntegerType::Int64),
                        ],
                        return_type: Box::new(Type::Integer(IntegerType::Int32)),
                    },
                    is_static: true,
                    visibility: MemberVisibility::Public,
                    overrides: false,
                    span: default_span(),
                    mutable: false,
                },
            );
        self.members
            .get_mut("TimeZone")
            .expect("TimeZone namespace")
            .insert(
                "IsDST".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![
                            Type::Named("TIMEZONE".into()),
                            Type::Integer(IntegerType::Int64),
                        ],
                        return_type: Box::new(Type::Boolean),
                    },
                    is_static: true,
                    visibility: MemberVisibility::Public,
                    overrides: false,
                    span: default_span(),
                    mutable: false,
                },
            );
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
                    visibility: MemberVisibility::Public,
                    overrides: false,
                    span: default_span(),
                    mutable: false,
                },
            );
        self.members
            .get_mut("Timestamp")
            .expect("Timestamp namespace")
            .insert(
                "AtZone".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![
                            Type::Integer(IntegerType::Int64),
                            Type::Named("TIMEZONE".into()),
                        ],
                        return_type: Box::new(Type::String),
                    },
                    is_static: true,
                    visibility: MemberVisibility::Public,
                    overrides: false,
                    span: default_span(),
                    mutable: false,
                },
            );
        self.members
            .get_mut("Timestamp")
            .expect("Timestamp namespace")
            .insert(
                "FromLocal".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![
                            Type::Named("DATE".into()),
                            Type::Named("TIME".into()),
                            Type::Named("TIMEZONE".into()),
                        ],
                        return_type: Box::new(Type::Integer(IntegerType::Int64)),
                    },
                    is_static: true,
                    visibility: MemberVisibility::Public,
                    overrides: false,
                    span: default_span(),
                    mutable: false,
                },
            );
        for (owner, members) in crate::host_spec::catalog().members {
            self.members.insert(
                owner,
                members
                    .into_iter()
                    .map(|(name, member)| (name, member.into()))
                    .collect(),
            );
        }
    }
}
