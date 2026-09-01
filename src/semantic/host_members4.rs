#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn declare_host_members_17(&mut self) {
        self.members.insert(
            "HOST.Net.PingReply".into(),
            HashMap::from([
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
                    "RoundTripMicroseconds".into(),
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
}
