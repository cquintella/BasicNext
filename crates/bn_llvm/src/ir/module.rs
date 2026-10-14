// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The LLVM module `bnc` builds in memory and renders once at the end
//! (bucket 0.6.5 T.5, the Swift architecture): declarations, globals,
//! functions, block labels and instructions, all typed. There is no way to
//! write LLVM text into it.

use std::fmt::{self, Write as _};

use std::collections::BTreeSet;

use super::{
    abi::Signature,
    debug::MdRef,
    emit::InstSink,
    function::LlvmFunction,
    global::LlvmGlobal,
    instructions::{InstructionEntry, LlvmInst},
    operands::LlvmOperand,
};

/// Adds the globals `inst` reads to `names`.
fn record_globals(inst: &LlvmInst, names: &mut BTreeSet<String>) {
    for operand in inst.operands() {
        if let LlvmOperand::Global(name) = operand {
            names.insert(name.trim_start_matches('@').to_owned());
        }
    }
}

/// Makes the debug metadata nodes [`LlvmModule::render_debug`] attaches.
pub trait DebugScopes {
    /// The subprogram of function `name`, declared at `line` of `source`.
    fn subprogram(&mut self, source: u64, line: u32, name: &str) -> MdRef;
    /// The location `line`:`column` inside `scope`.
    fn location(&mut self, line: u32, column: u32, scope: MdRef) -> MdRef;
}

/// One piece of a module, in emission order.
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    /// A typed instruction, rendered on its own line.
    Instruction(InstructionEntry),
    /// The label that starts a basic block.
    Label(String),
    /// A declared function.
    Declaration(Signature),
    /// A global variable or constant.
    Global(LlvmGlobal),
    /// A whole function built with the typed builder.
    Function(LlvmFunction),
    /// The `define ... {` line of a function emitted instruction by instruction.
    FunctionStart(LlvmFunction),
    /// The `}` that closes such a function.
    FunctionEnd,
    /// An empty line between items.
    Blank,
    /// A comment line: `; text`.
    Comment(String),
    /// `bnc -g`: the next function opened describes `name`, declared at
    /// `line` of source `source`. Renders nothing by itself.
    DebugFunction {
        source: u64,
        line: u32,
        name: String,
    },
    /// `bnc -g`: the instructions that follow were lowered from `line`:`column`.
    DebugLocation { line: u32, column: u32 },
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instruction(entry) => writeln!(f, "{entry}"),
            Self::Label(name) => writeln!(f, "{name}:"),
            Self::Declaration(signature) => writeln!(f, "{signature}"),
            Self::Global(global) => writeln!(f, "{global}"),
            Self::Function(function) => write!(f, "{function}"),
            Self::FunctionStart(function) => writeln!(f, "{}", function.header()),
            Self::FunctionEnd => writeln!(f, "}}"),
            Self::Blank => writeln!(f),
            Self::Comment(text) => writeln!(f, "; {text}"),
            Self::DebugFunction { .. } | Self::DebugLocation { .. } => Ok(()),
        }
    }
}

/// An LLVM module under construction. Its rendering is the `.ll` text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LlvmModule {
    pieces: Vec<Piece>,
    /// The last number `fresh_tag` returned.
    tags: usize,
}

impl LlvmModule {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn append(&mut self, piece: Piece) {
        self.pieces.push(piece);
    }

    /// Declares a function if not already declared.
    pub fn declare(&mut self, signature: Signature) {
        if self.pieces.iter().any(|p| match p {
            Piece::Declaration(s) => s.name == signature.name,
            _ => false,
        }) {
            return;
        }
        self.append(Piece::Declaration(signature));
    }

    /// Defines a global.
    pub fn global(&mut self, global: LlvmGlobal) {
        self.append(Piece::Global(global));
    }

    /// Defines a whole function.
    pub fn function(&mut self, function: LlvmFunction) {
        self.append(Piece::Function(function));
    }

    /// Opens a function whose body the emitter writes next.
    pub fn begin_function(&mut self, header: LlvmFunction) {
        self.append(Piece::FunctionStart(header));
    }

    /// Closes the function [`Self::begin_function`] opened.
    pub fn end_function(&mut self) {
        self.append(Piece::FunctionEnd);
    }

    /// An empty line.
    pub fn blank(&mut self) {
        self.append(Piece::Blank);
    }

    /// Marks the next function opened as describing `name` (`bnc -g`).
    pub fn mark_function(&mut self, source: u64, line: u32, name: impl Into<String>) {
        self.append(Piece::DebugFunction {
            source,
            line,
            name: name.into(),
        });
    }

    /// Marks the source position of the instructions that follow (`bnc -g`).
    pub fn mark_location(&mut self, line: u32, column: u32) {
        self.append(Piece::DebugLocation { line, column });
    }

    /// A comment line.
    pub fn comment(&mut self, text: impl Into<String>) {
        self.append(Piece::Comment(text.into()));
    }

    /// A number no other `fresh_tag` call of this module returns, for unique
    /// register names.
    pub fn fresh_tag(&mut self) -> usize {
        self.tags += 1;
        self.tags
    }

    /// The point between the pieces emitted so far and the next one, where
    /// `insert_assign` can add an instruction later (a deferred `phi`).
    #[must_use]
    pub const fn position(&self) -> usize {
        self.pieces.len()
    }

    /// Inserts `%dest = inst` at `position`, a value `position` returned
    /// earlier; inserting from the last position back keeps the others valid.
    pub fn insert_assign(&mut self, position: usize, dest: impl Into<String>, inst: LlvmInst) {
        let piece = Piece::Instruction(InstructionEntry::assign(dest, inst));
        self.pieces.insert(position, piece);
    }

    /// The names, without `@`, of every global the module refers to: the
    /// functions it calls and the constants it reads.
    #[must_use]
    pub fn referenced_globals(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for piece in &self.pieces {
            match piece {
                Piece::Instruction(entry) => record_globals(&entry.inst, &mut names),
                Piece::Function(function) => {
                    for block in &function.blocks {
                        for entry in &block.instructions {
                            record_globals(&entry.inst, &mut names);
                        }
                    }
                }
                Piece::Declaration(_)
                | Piece::Global(_)
                | Piece::FunctionStart(_)
                | Piece::FunctionEnd
                | Piece::Blank
                | Piece::Comment(_)
                | Piece::DebugFunction { .. }
                | Piece::DebugLocation { .. } => {}
                Piece::Label(_) => {}
            }
        }
        names
    }

    /// The `.ll` text with debug attachments (`bnc -g`): a marked function
    /// opens with `!dbg` and its subprogram, and each of its instructions
    /// carries the location last marked (the function's line for the
    /// prologue). `scopes` makes the metadata nodes.
    pub fn render_debug(&self, scopes: &mut impl DebugScopes) -> String {
        let mut out = String::new();
        let mut pending: Option<(u64, u32, &str)> = None;
        let mut scope: Option<(MdRef, MdRef)> = None;
        for piece in &self.pieces {
            match piece {
                Piece::DebugFunction { source, line, name } => {
                    pending = Some((*source, *line, name));
                }
                Piece::DebugLocation { line, column } => {
                    if let Some((subprogram, _)) = scope {
                        scope = Some((subprogram, scopes.location(*line, *column, subprogram)));
                    }
                }
                Piece::FunctionStart(function) => match pending.take() {
                    Some((source, line, name)) => {
                        let subprogram = scopes.subprogram(source, line, name);
                        scope = Some((subprogram, scopes.location(line, 1, subprogram)));
                        let _ = writeln!(out, "{}", function.header_with_dbg(subprogram));
                    }
                    None => {
                        let _ = write!(out, "{piece}");
                    }
                },
                Piece::FunctionEnd => {
                    scope = None;
                    let _ = write!(out, "{piece}");
                }
                Piece::Instruction(entry) => match scope {
                    Some((_, location)) => {
                        let _ = writeln!(out, "{}", entry.clone().with_dbg(location.0));
                    }
                    None => {
                        let _ = write!(out, "{piece}");
                    }
                },
                _ => {
                    let _ = write!(out, "{piece}");
                }
            }
        }
        out
    }

    /// The `.ll` text of the module.
    #[must_use]
    pub fn render(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for LlvmModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pieces
            .iter()
            .try_for_each(|piece| write!(f, "{piece}"))
    }
}

impl InstSink for LlvmModule {
    fn assign(&mut self, dest: impl Into<String>, inst: LlvmInst) {
        self.append(Piece::Instruction(InstructionEntry::assign(dest, inst)));
    }

    fn emit(&mut self, inst: LlvmInst) {
        self.append(Piece::Instruction(InstructionEntry::void(inst)));
    }

    fn label(&mut self, name: &str) {
        self.append(Piece::Label(name.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::LlvmModule;
    use crate::ir::{InstSink as _, LlvmFunction, LlvmInst, LlvmOperand, LlvmType};

    #[test]
    fn a_module_renders_what_was_emitted_and_inserts_at_a_position() {
        let mut module = LlvmModule::new();
        module.begin_function(LlvmFunction::new_definition("f", LlvmType::Void, vec![]));
        module.label("b0");
        let position = module.position();
        module.emit(LlvmInst::Ret { val: None });
        let phi = LlvmInst::Phi {
            ty: LlvmType::I32,
            incoming: vec![(LlvmOperand::int(1), "b1".into())],
        };
        module.insert_assign(position, "x", phi);
        module.end_function();
        let expected = "define void @f() {\nb0:\n  %x = phi i32 [ 1, %b1 ]\n  ret void\n}\n";
        assert_eq!(module.render(), expected);
        let typed = LlvmInst::store(LlvmType::I32, LlvmOperand::int(1), LlvmOperand::reg("s"));
        module.emit(typed);
        assert!(module.render().ends_with("  store i32 1, ptr %s\n"));
    }
}
