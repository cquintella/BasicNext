//! Language-owned description of the built-in HOST capability names.
//!
//! This module deliberately contains no runtime implementation. Semantic
//! analysis consumes this catalog and maps the entries to its internal types.

use std::collections::BTreeMap;

#[cfg(feature = "frontend-host-spec-path")]
#[path = "host_spec/members1.rs"]
mod members1;
#[cfg(not(feature = "frontend-host-spec-path"))]
#[path = "host_spec/members1.rs"]
mod members1;
#[cfg(feature = "frontend-host-spec-path")]
#[path = "host_spec/members2.rs"]
mod members2;
#[cfg(not(feature = "frontend-host-spec-path"))]
#[path = "host_spec/members2.rs"]
mod members2;
#[cfg(feature = "frontend-host-spec-path")]
#[path = "host_spec/members3.rs"]
mod members3;
#[cfg(not(feature = "frontend-host-spec-path"))]
#[path = "host_spec/members3.rs"]
mod members3;
#[cfg(feature = "frontend-host-spec-path")]
#[path = "host_spec/members4.rs"]
mod members4;
#[cfg(not(feature = "frontend-host-spec-path"))]
#[path = "host_spec/members4.rs"]
mod members4;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpecIntegerType {
    Byte,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpecFloatType {
    Float32,
    Float64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpecPointerLength {
    One,
    Fixed(u64),
    Dynamic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SpecType {
    Boolean,
    Integer(SpecIntegerType),
    IntegerLiteral(String),
    Float(SpecFloatType),
    FloatLiteral,
    String,
    Null,
    NotAvailable,
    EndOfFile,
    Named(String),
    TypeName(String),
    Function {
        parameters: Vec<SpecType>,
        return_type: Box<SpecType>,
    },
    Vector {
        element: Box<SpecType>,
        dimensions: Vec<u64>,
    },
    Pointer {
        element: Box<SpecType>,
        length: SpecPointerLength,
    },
    Alternative(Vec<SpecType>),
}

#[derive(Clone, Debug)]
pub(crate) struct SpecMember {
    pub(crate) ty: SpecType,
    pub(crate) is_static: bool,
    pub(crate) private: bool,
    pub(crate) mutable: bool,
}

#[derive(Default)]
pub(crate) struct Catalog {
    pub(crate) members: BTreeMap<String, BTreeMap<String, SpecMember>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Capability {
    Args,
    Console,
    Clock,
    Random,
    FileSystem,
    Net,
    NumProcs,
}

impl Capability {
    #[must_use]
    pub(crate) const fn owner(self) -> &'static str {
        match self {
            Self::Args => "HOST.Args",
            Self::Console => "HOST.Console",
            Self::Clock => "HOST.Clock",
            Self::Random => "HOST.Random",
            Self::FileSystem => "HOST.FileSystem",
            Self::Net => "HOST.Net",
            Self::NumProcs => "HOST.NumProcs",
        }
    }
}

#[must_use]
pub(crate) fn capability(name: &str) -> Option<Capability> {
    match name {
        "Args" => Some(Capability::Args),
        "Console" => Some(Capability::Console),
        "Clock" => Some(Capability::Clock),
        "Random" => Some(Capability::Random),
        "FileSystem" => Some(Capability::FileSystem),
        "Net" => Some(Capability::Net),
        "NumProcs" => Some(Capability::NumProcs),
        _ => None,
    }
}

pub(crate) fn catalog() -> Catalog {
    let mut catalog = Catalog::default();
    members1::declare_1(&mut catalog);
    members1::declare_2(&mut catalog);
    members1::declare_3(&mut catalog);
    members1::declare_4(&mut catalog);
    members1::declare_5(&mut catalog);
    members1::declare_6(&mut catalog);
    members2::declare_7(&mut catalog);
    members2::declare_8(&mut catalog);
    members2::declare_9(&mut catalog);
    members2::declare_10(&mut catalog);
    members3::declare_11(&mut catalog);
    members3::declare_12(&mut catalog);
    members3::declare_13(&mut catalog);
    members3::declare_14(&mut catalog);
    members3::declare_15(&mut catalog);
    members3::declare_16(&mut catalog);
    members4::declare_17(&mut catalog);
    catalog
}

#[cfg(test)]
mod tests {
    use super::capability;

    #[test]
    fn catalog_contains_every_public_host_capability() {
        for name in [
            "Args",
            "Console",
            "Clock",
            "Random",
            "FileSystem",
            "Net",
            "NumProcs",
        ] {
            assert!(capability(name).is_some(), "missing HOST capability {name}");
        }
    }

    #[test]
    fn withdrawn_and_unknown_capabilities_are_not_catalogued() {
        assert_eq!(capability("Main"), None);
        assert_eq!(capability("Network"), None);
    }

    #[test]
    fn member_catalog_loads_the_host_and_filesystem_namespaces() {
        let catalog = super::catalog();
        for owner in [
            "HOST.Clock",
            "HOST.Random",
            "HOST.FileSystem",
            "HOST.Console",
            "HOST.Net",
            "FS.File",
        ] {
            assert!(catalog.members.contains_key(owner), "missing {owner}");
        }
        assert_eq!(catalog.members["HOST.Clock"].len(), 2);
        assert_eq!(catalog.members["HOST.Random"].len(), 2);
        assert_eq!(catalog.members["HOST.Console"].len(), 5);
    }
}
