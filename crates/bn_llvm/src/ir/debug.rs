// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed debug-information metadata nodes (`DIFile`, `DICompileUnit`,
//! `DISubprogram`, `DILocation`) and their canonical numbered rendering.

use std::{collections::HashMap, fmt};

use super::operands::escape_llvm;

/// Reference to a numbered metadata node: renders as `!<id>`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MdRef(pub u32);

impl fmt::Display for MdRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "!{}", self.0)
    }
}

/// Debug-information format selected by the module flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugFormat {
    /// DWARF with the given version (ELF, Mach-O, Wasm).
    Dwarf(u32),
    /// `CodeView`, which the MSVC linker turns into a PDB (Windows).
    CodeView,
}

/// One metadata node of the subset Basic Next emits.
#[derive(Clone, Debug, PartialEq)]
pub enum DebugNode {
    File {
        filename: String,
        directory: String,
    },
    CompileUnit {
        file: MdRef,
        producer: String,
        optimized: bool,
    },
    /// `DISubroutineType(types: !{})`: parameter and return types are not
    /// described, which is enough for line tables and frame names.
    SubroutineType,
    Subprogram {
        name: String,
        file: MdRef,
        line: u32,
        ty: MdRef,
        unit: MdRef,
    },
    Location {
        line: u32,
        column: u32,
        scope: MdRef,
    },
}

impl fmt::Display for DebugNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File {
                filename,
                directory,
            } => write!(
                f,
                "!DIFile(filename: \"{}\", directory: \"{}\")",
                escape_llvm(filename),
                escape_llvm(directory)
            ),
            Self::CompileUnit {
                file,
                producer,
                optimized,
            } => write!(
                f,
                "distinct !DICompileUnit(language: DW_LANG_C, file: {file}, producer: \"{}\", isOptimized: {optimized}, runtimeVersion: 0, emissionKind: FullDebug)",
                escape_llvm(producer)
            ),
            Self::SubroutineType => write!(f, "!DISubroutineType(types: !{{}})"),
            Self::Subprogram {
                name,
                file,
                line,
                ty,
                unit,
            } => write!(
                f,
                "distinct !DISubprogram(name: \"{}\", scope: {file}, file: {file}, line: {line}, type: {ty}, scopeLine: {line}, spFlags: DISPFlagDefinition, unit: {unit})",
                escape_llvm(name)
            ),
            Self::Location {
                line,
                column,
                scope,
            } => write!(
                f,
                "!DILocation(line: {line}, column: {column}, scope: {scope})"
            ),
        }
    }
}

/// Numbered metadata table for one LLVM module. Files, the subroutine type
/// and locations are interned, so equal nodes share one number.
#[derive(Debug)]
pub struct DebugMetadata {
    nodes: Vec<DebugNode>,
    files: HashMap<(String, String), MdRef>,
    locations: HashMap<(u32, u32, MdRef), MdRef>,
    unit: MdRef,
    subroutine_type: MdRef,
    format: DebugFormat,
}

impl DebugMetadata {
    /// Starts a table whose compile unit belongs to the entry file.
    #[must_use]
    pub fn new(
        filename: &str,
        directory: &str,
        producer: &str,
        optimized: bool,
        format: DebugFormat,
    ) -> Self {
        let mut table = Self {
            nodes: Vec::new(),
            files: HashMap::new(),
            locations: HashMap::new(),
            unit: MdRef(0),
            subroutine_type: MdRef(0),
            format,
        };
        let file = table.file(filename, directory);
        table.unit = table.push(DebugNode::CompileUnit {
            file,
            producer: producer.to_owned(),
            optimized,
        });
        table.subroutine_type = table.push(DebugNode::SubroutineType);
        table
    }

    fn push(&mut self, node: DebugNode) -> MdRef {
        let id = MdRef(u32::try_from(self.nodes.len()).expect("metadata node count fits u32"));
        self.nodes.push(node);
        id
    }

    /// Interns a `DIFile`.
    pub fn file(&mut self, filename: &str, directory: &str) -> MdRef {
        let key = (filename.to_owned(), directory.to_owned());
        if let Some(id) = self.files.get(&key) {
            return *id;
        }
        let id = self.push(DebugNode::File {
            filename: key.0.clone(),
            directory: key.1.clone(),
        });
        self.files.insert(key, id);
        id
    }

    /// Adds a `DISubprogram` for one defined function. It carries the Basic
    /// Next name only: without `linkageName` debuggers show `Twice`, not the
    /// `bn_Twice` symbol.
    pub fn subprogram(&mut self, name: &str, file: MdRef, line: u32) -> MdRef {
        self.push(DebugNode::Subprogram {
            name: name.to_owned(),
            file,
            line,
            ty: self.subroutine_type,
            unit: self.unit,
        })
    }

    /// Interns a `DILocation` inside `scope`.
    pub fn location(&mut self, line: u32, column: u32, scope: MdRef) -> MdRef {
        if let Some(id) = self.locations.get(&(line, column, scope)) {
            return *id;
        }
        let id = self.push(DebugNode::Location {
            line,
            column,
            scope,
        });
        self.locations.insert((line, column, scope), id);
        id
    }
}

impl fmt::Display for DebugMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Named metadata and module flags take the numbers after the nodes.
        let base = self.nodes.len();
        writeln!(f, "!llvm.dbg.cu = !{{{}}}", self.unit)?;
        writeln!(f, "!llvm.module.flags = !{{!{base}, !{}}}", base + 1)?;
        for (index, node) in self.nodes.iter().enumerate() {
            writeln!(f, "!{index} = {node}")?;
        }
        match self.format {
            DebugFormat::Dwarf(version) => {
                writeln!(f, "!{base} = !{{i32 7, !\"Dwarf Version\", i32 {version}}}")?;
            }
            DebugFormat::CodeView => writeln!(f, "!{base} = !{{i32 2, !\"CodeView\", i32 1}}")?,
        }
        writeln!(
            f,
            "!{} = !{{i32 2, !\"Debug Info Version\", i32 3}}",
            base + 1
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_numbered_nodes_and_module_flags() {
        let mut table = DebugMetadata::new("h.bn", "/src", "bnc", false, DebugFormat::Dwarf(4));
        let file = table.file("h.bn", "/src");
        assert_eq!(file, MdRef(0), "the entry file is interned once");
        let sp = table.subprogram("Twice", file, 1);
        let loc = table.location(2, 5, sp);
        assert_eq!(
            table.location(2, 5, sp),
            loc,
            "equal locations are interned"
        );
        assert_eq!(
            table.to_string(),
            "!llvm.dbg.cu = !{!1}\n\
             !llvm.module.flags = !{!5, !6}\n\
             !0 = !DIFile(filename: \"h.bn\", directory: \"/src\")\n\
             !1 = distinct !DICompileUnit(language: DW_LANG_C, file: !0, producer: \"bnc\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)\n\
             !2 = !DISubroutineType(types: !{})\n\
             !3 = distinct !DISubprogram(name: \"Twice\", scope: !0, file: !0, line: 1, type: !2, scopeLine: 1, spFlags: DISPFlagDefinition, unit: !1)\n\
             !4 = !DILocation(line: 2, column: 5, scope: !3)\n\
             !5 = !{i32 7, !\"Dwarf Version\", i32 4}\n\
             !6 = !{i32 2, !\"Debug Info Version\", i32 3}\n"
        );
    }

    #[test]
    fn codeview_flag_replaces_dwarf_version() {
        let table = DebugMetadata::new("a.bn", "C:\\x", "bnc", true, DebugFormat::CodeView);
        let text = table.to_string();
        assert!(text.contains("!{i32 2, !\"CodeView\", i32 1}"));
        assert!(!text.contains("Dwarf Version"));
        assert!(text.contains("isOptimized: true"));
    }

    #[test]
    fn metadata_strings_escape_quotes_backslashes_and_non_ascii() {
        let node = DebugNode::File {
            filename: "a\"b.bn".into(),
            directory: "C:\\dir\\é".into(),
        };
        assert_eq!(
            node.to_string(),
            "!DIFile(filename: \"a\\22b.bn\", directory: \"C:\\5Cdir\\5C\\C3\\A9\")"
        );
    }
}
