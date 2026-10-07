// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Explicit ownership (proposal `arc-shared-core-0.6.5`, "Lowering
//! discipline"): the one place that decides retains and releases. Slots own
//! references, values borrow them. `NEW` and a call returning an ARC value
//! yield an owned value (so do a vector literal, which takes its elements,
//! and a default `STRUCT` or vector); storing consumes one (a borrowed value is retained
//! first) and releases the previous content; an owned value nothing stores
//! is released at the end of its statement; every block releases its ARC
//! locals in reverse declaration order, on every exit.

#![allow(clippy::wildcard_imports)]
use super::*;

impl Builder<'_> {
    /// Whether values of `ty` carry strong references, so ownership
    /// operations apply: a class instance (it has a `$fields` function), an
    /// `INTERFACE` (it holds an object), a `STRUCT` (`$default`), a pointer
    /// region, a vector of those, or an
    /// alternative with one of those members; never a native library handle. Every `STRUCT` counts: one
    /// without strong fields makes `Retain` and `Release` do nothing, which
    /// is simpler than tracking which ones hold objects.
    pub(crate) fn is_arc(&self, ty: &Type) -> bool {
        let declared = |name: &str| {
            [".$fields", ".$default", ".$interface"]
                .iter()
                .any(|suffix| {
                    self.methods
                        .contains(&format!("{}{name}{suffix}", self.prefix))
                        || self.methods.contains(&format!("{name}{suffix}"))
                })
        };
        match ty {
            Type::Named(name) => declared(name),
            // A type of a natively implemented library (`BNJson`, `BNData`,
            // …) is a library handle whose lifetime the library manages.
            Type::ImportedNamed { module, .. } if self.model.standard_modules.contains(module) => {
                false
            }
            Type::ImportedNamed { module, name } => declared(&format!("#{}.{name}", module.0)),
            Type::Pointer { .. } => true,
            Type::Vector { element, .. } => self.is_arc(element),
            Type::Alternative(members) => members.iter().any(|member| self.is_arc(member)),
            _ => false,
        }
    }

    /// Whether the assignment target `target` is a weak field
    /// (`obj.next` with `next AS WEAK Node OR NULL`).
    pub(crate) fn is_weak_field(&self, target: &Expression) -> bool {
        let ExpressionKind::Member { name, .. } = &target.kind else {
            return false;
        };
        self.model
            .expression(target.span)
            .and_then(|resolved| resolved.member_target.as_ref())
            .and_then(|member| member.owner.as_deref())
            .is_some_and(|owner| {
                self.methods
                    .contains(&format!("{}{owner}.{name}.$weak", self.prefix))
                    || self.methods.contains(&format!("{owner}.{name}.$weak"))
            })
    }

    /// Whether a call of `callee` may run code of the program, so a `STOP`
    /// in it unwinds through this function: a function or method of the
    /// program, a super call, or a callee only known at run time (a
    /// `FUNCTION` value). An intrinsic, a language global, a library or a
    /// HOST member runs none (a library callback is the end of the release,
    /// 0.6.md, "`STOP`").
    pub(crate) fn may_run_program_code(&self, callee: ValueId) -> bool {
        match self.function_names.get(&callee) {
            Some(name) => {
                let name = name
                    .strip_prefix(bn_ir::names::SUPER_PREFIX)
                    .unwrap_or(name);
                !name.starts_with(bn_ir::names::SYNTHESISED_MARKER) && self.methods.contains(name)
            }
            None => true,
        }
    }

    /// After a call that may have stopped (0.6.md, "`STOP`"): when it did,
    /// release this function's temporaries and open scopes, and stop with
    /// the same code, so every running function releases its locals,
    /// innermost first. The values stay owned on the path that goes on.
    pub(crate) fn stop_check(&mut self, span: Span) {
        let stopping = self.value();
        let callee = self.function_constant(bn_ir::names::STOPPING, span);
        self.emit(Instruction::Call {
            destination: stopping,
            callee,
            arguments: Vec::new(),
            ty: Type::Boolean,
            span,
        });
        let landing = self.block();
        let next = self.block();
        // Set directly: `terminate` would release the temporaries on both
        // paths.
        self.blocks[self.current.0 as usize].terminator = Some(Terminator::Branch {
            condition: stopping,
            then_block: landing,
            else_block: next,
        });
        self.current = landing;
        for (value, span) in self.owned.clone() {
            self.emit(Instruction::Release {
                value,
                destructor: None,
                span,
            });
        }
        self.release_scopes(0);
        let code = self.value();
        let callee = self.function_constant(bn_ir::names::STOP_CODE, span);
        self.emit(Instruction::Call {
            destination: code,
            callee,
            arguments: Vec::new(),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            span,
        });
        self.blocks[self.current.0 as usize].terminator = Some(Terminator::Stop { code });
        self.current = next;
    }

    /// Records the type of the value `instruction` defines, and whether it
    /// arrives owned (`NEW`, a call returning an ARC value).
    pub(crate) fn note_result(&mut self, instruction: &Instruction) {
        if let Instruction::Constant {
            destination,
            value: Constant::Function(name),
            ..
        } = instruction
        {
            self.function_names.insert(*destination, name.clone());
        }
        let Some((value, ty)) = bn_ir::instruction_result(instruction) else {
            return;
        };
        let owned = matches!(
            instruction,
            Instruction::Allocate { .. }
                | Instruction::Call { .. }
                | Instruction::Vector { .. }
                | Instruction::Default { .. }
        ) && self.is_arc(ty);
        self.value_types.insert(value, ty.clone());
        if owned {
            self.owned.push((value, instruction.span()));
        }
    }

    /// The owned form of `value`, to be consumed by a store or `RETURN`: an
    /// owned value moves; a borrowed ARC value is retained first; a value
    /// that holds no reference is itself.
    pub(crate) fn owned_value(&mut self, value: ValueId, span: Span) -> ValueId {
        let Some(ty) = self.value_types.get(&value).cloned() else {
            return value;
        };
        if !self.is_arc(&ty) {
            return value;
        }
        if let Some(index) = self.owned.iter().position(|(owned, _)| *owned == value) {
            self.owned.remove(index);
            return value;
        }
        let retained = self.value();
        self.emit(Instruction::Retain {
            destination: retained,
            value,
            ty,
            span,
        });
        retained
    }

    /// Releases the owned values nothing consumed (temporaries), except
    /// `keep` (the value a `RETURN` hands to the caller).
    pub(crate) fn release_temporaries(&mut self, keep: Option<ValueId>) {
        let pending = std::mem::take(&mut self.owned);
        for (value, span) in pending {
            if Some(value) == keep {
                continue;
            }
            self.emit(Instruction::Release {
                value,
                destructor: None,
                span,
            });
        }
    }

    /// Records an ARC local of the innermost scope.
    pub(crate) fn declare_owner(&mut self, symbol: SymbolId, ty: Type, span: Span) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((symbol, ty, span));
        }
    }

    /// Releases the ARC locals of every scope from `depth` inward, innermost
    /// first and each scope in reverse declaration order. The scopes stay
    /// open: an early exit (`RETURN`, `EXIT`, `CONTINUE`) leaves them for the
    /// other paths.
    pub(crate) fn release_scopes(&mut self, depth: usize) {
        let locals = self.scopes[depth..]
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev().cloned())
            .collect::<Vec<_>>();
        for (symbol, ty, span) in locals {
            let taken = self.value();
            self.emit(Instruction::Take {
                destination: taken,
                symbol,
                ty,
                span,
            });
            self.emit(Instruction::Release {
                value: taken,
                destructor: None,
                span,
            });
        }
    }

    /// Lowers a block body in its own scope: its ARC locals are released
    /// when the block falls through.
    pub(crate) fn scoped_statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        self.scopes.push(Vec::new());
        let lowered = self.statements(statements);
        if lowered.is_ok() && !self.terminated() {
            self.release_scopes(self.scopes.len() - 1);
        }
        self.scopes.pop();
        lowered
    }
}
