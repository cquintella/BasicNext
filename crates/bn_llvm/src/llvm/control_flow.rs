use super::*;

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
    offset: usize,
    destination: ValueId,
    ty: &'static str,
    incoming: Vec<(BlockId, ValueId)>,
}

impl EmittedControlFlow {
    pub(super) fn label(&mut self, text: &mut String, label: String) {
        let _ = writeln!(text, "{label}:");
        self.current_label = label;
    }

    pub(super) fn finish_block(&mut self, block: BlockId) {
        self.exits.insert(block.0, self.current_label.clone());
    }

    pub(super) fn defer_phi(
        &mut self,
        offset: usize,
        destination: ValueId,
        ty: &'static str,
        incoming: &[(BlockId, ValueId)],
    ) {
        self.phis.push(PendingPhi {
            offset,
            destination,
            ty,
            incoming: incoming.to_vec(),
        });
    }

    pub(super) fn resolve(self, text: &mut String) -> Result<(), String> {
        // Descending byte offsets preserve every recorded insertion position.
        // Only deferred Phi instructions are inserted; existing text (including
        // other functions and emitter-internal Phi nodes) is never rewritten.
        for phi in self.phis.into_iter().rev() {
            let incoming = phi
                .incoming
                .iter()
                .filter_map(|(block, value)| {
                    self.exits
                        .get(&block.0)
                        .map(|label| format!("[ %v{}, %{label} ]", value.0))
                })
                .collect::<Vec<_>>()
                .join(", ");
            if incoming.is_empty() {
                return Err(format!(
                    "Phi %v{} has no reachable emitted predecessor",
                    phi.destination.0
                ));
            }
            text.insert_str(
                phi.offset,
                &format!("  %v{} = phi {} {incoming}\n", phi.destination.0, phi.ty),
            );
        }
        Ok(())
    }
}
