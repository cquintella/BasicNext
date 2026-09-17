//! Frontend lowering boundary: sources → validated IR. Consumed by the CLI,
//! LSP and DAP; never by a backend.

pub use bn_frontend::lowering::{lower, lower_graph, lower_graph_validated, lower_validated};
