//! Read-only ARP/NDP neighbor lookup for HOST.Net.Neighbor.

use super::Address;

#[derive(Debug)]
pub enum NeighborError {
    Unsupported,
    NotFound,
}

impl NeighborError {
    pub fn message(&self) -> String {
        match self {
            Self::Unsupported => "direct-neighbor lookup unsupported on this host".into(),
            Self::NotFound => "direct-neighbor entry not found".into(),
        }
    }
}

/// Looks up an existing OS ARP/NDP entry for `address`.
///
/// # Errors
///
/// Returns unsupported or absent-entry failures. This Phase 0 macOS/Windows
/// build does not mutate the neighbor table and does not claim a working
/// lookup provider.
pub fn neighbor(_address: Address) -> Result<Address, NeighborError> {
    Err(NeighborError::Unsupported)
}
