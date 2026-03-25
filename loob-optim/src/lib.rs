//! Generic optimization algorithms for sequencing problems.
//!
//! This crate is intentionally abstract — it knows about distance matrices
//! and node orderings, but nothing about music or YouTube.

pub mod cost;
pub mod greedy;
pub mod sa;

pub use cost::{AsymmetricCostMatrix, CostFn};
