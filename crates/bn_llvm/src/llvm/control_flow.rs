use super::{BlockId, HashMap, ValueId};
use crate::ir::{LlvmInst, LlvmOperand};

/// Physical exits and Phi relocations belong to one emitted function. A BN
/// block may expand into several LLVM blocks; its successor's Phi must name
/// the final physical block, including predecessors emitted later (backedges).
#[derive(Default)]
pub(super) struct EmittedControlFlow {
    current_label: String,
    exits: HashMap<u32, String>,
    phis: Vec<PendingPhi>,
}

struct PendingPhi {
    position: usize,
    destination: ValueId,
    ty: crate::ir::LlvmType,
    incoming: Vec<(BlockId, ValueId)>,
}

impl EmittedControlFlow {
    pub(super) fn label(&mut self, text: &mut crate::ir::LlvmModule, label: String) {
        crate::ir::InstSink::label(text, &label);
        self.current_label = label;
    }

    pub(super) fn finish_block(&mut self, block: BlockId) {
        self.exits.insert(block.0, self.current_label.clone());
    }

    pub(super) fn defer_phi(
        &mut self,
        position: usize,
        destination: ValueId,
        ty: crate::ir::LlvmType,
        incoming: &[(BlockId, ValueId)],
    ) {
        self.phis.push(PendingPhi {
            position,
            destination,
            ty,
            incoming: incoming.to_vec(),
        });
    }

    pub(super) fn resolve(self, text: &mut crate::ir::LlvmModule) -> Result<(), String> {
        // Inserting from the last recorded position back keeps the earlier
        // positions valid. Only deferred Phi instructions are inserted; nothing
        // already emitted (other functions, emitter-internal Phi nodes) changes.
        for phi in self.phis.into_iter().rev() {
            let incoming: Vec<(LlvmOperand, String)> = (phi.incoming.iter())
                .filter_map(|(block, value)| {
                    let label = self.exits.get(&block.0)?;
                    Some((LlvmOperand::reg(format!("v{}", value.0)), label.clone()))
                })
                .collect();
            if incoming.is_empty() {
                return Err(format!(
                    "Phi %v{} has no reachable emitted predecessor",
                    phi.destination.0
                ));
            }
            let inst = LlvmInst::Phi {
                ty: phi.ty,
                incoming,
            };
            text.insert_assign(phi.position, format!("v{}", phi.destination.0), inst);
        }
        Ok(())
    }
}
