//! `wisp-core` – L2 file-system abstractions and shared cross-cutting types.
//!
//! All other crates in the workspace depend on this crate.  It has zero UI
//! dependencies.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod errors;
pub mod fs;
pub mod scanner;
pub mod trash;
pub mod types;

pub use errors::{CoreError, CoreResult};
