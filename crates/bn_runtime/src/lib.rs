//! Runtime-owned memory primitives.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

mod heap;

pub use heap::{Handle, Heap};
