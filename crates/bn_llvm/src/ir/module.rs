// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The LLVM module `bnc` builds in memory and renders once at the end
//! (bucket 0.6.5 T.5.6, the Swift architecture): typed instructions and
//! block labels, plus `Raw` text that typed lowering has not replaced yet.
//! `Raw` is transitional; T.5.5 deletes it.

use std::fmt;

use std::collections::BTreeSet;

use super::{
    emit::InstSink,
    instructions::{InstructionEntry, LlvmInst},
    operands::LlvmOperand,
};

/// Adds the global names (`@name`) written in `text` to `names`.
fn scan_globals(text: &str, names: &mut BTreeSet<String>) {
    for (index, _) in text.match_indices('@') {
        let name: String = text[index + 1..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '$')
            })
            .collect();
        if !name.is_empty() {
            names.insert(name);
        }
    }
}

/// One piece of a module, in emission order.
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    /// A typed instruction, rendered on its own line.
    Instruction(InstructionEntry),
    /// The label that starts a basic block.
    Label(String),
    /// LLVM text not yet in the typed model, rendered as written.
    Raw(String),
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instruction(entry) => writeln!(f, "{entry}"),
            Self::Label(name) => writeln!(f, "{name}:"),
            Self::Raw(text) => f.write_str(text),
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

    /// Appends LLVM text not yet in the typed model (transitional).
    pub fn push_str(&mut self, text: &str) {
        if !text.is_empty() {
            self.append(Piece::Raw(text.to_owned()));
        }
    }

    /// Appends one character of LLVM text (transitional).
    pub fn push(&mut self, character: char) {
        self.append(Piece::Raw(character.to_string()));
    }

    /// Whether nothing was emitted yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
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
    /// functions it calls and the constants it reads. `Raw` text is scanned
    /// until T.5.5 removes it.
    #[must_use]
    pub fn referenced_globals(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for piece in &self.pieces {
            match piece {
                Piece::Instruction(entry) => {
                    for operand in entry.inst.operands() {
                        if let LlvmOperand::Global(name) = operand {
                            names.insert(name.trim_start_matches('@').to_owned());
                        }
                    }
                }
                Piece::Raw(text) => scan_globals(text, &mut names),
                Piece::Label(_) => {}
            }
        }
        names
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

impl fmt::Write for LlvmModule {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push_str(text);
        Ok(())
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

impl From<String> for LlvmModule {
    fn from(text: String) -> Self {
        let mut module = Self::new();
        module.push_str(&text);
        module
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::LlvmModule;
    use crate::ir::{InstSink as _, LlvmInst, LlvmOperand, LlvmType};

    #[test]
    fn a_module_renders_what_was_emitted_and_inserts_at_a_position() {
        let mut module = LlvmModule::new();
        module.push_str("define void @f() {\n");
        module.label("b0");
        let position = module.position();
        module.emit(LlvmInst::Ret { val: None });
        let phi = LlvmInst::Phi {
            ty: LlvmType::I32,
            incoming: vec![(LlvmOperand::int(1), "b1".into())],
        };
        module.insert_assign(position, "x", phi);
        let _ = writeln!(module, "}}");
        let expected = "define void @f() {\nb0:\n  %x = phi i32 [ 1, %b1 ]\n  ret void\n}\n";
        assert_eq!(module.render(), expected);
        let typed = LlvmInst::store(LlvmType::I32, LlvmOperand::int(1), LlvmOperand::reg("s"));
        module.emit(typed);
        assert!(module.render().ends_with("  store i32 1, ptr %s\n"));
    }
}
