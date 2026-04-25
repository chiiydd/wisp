//! `wisp-core` – L2 File-system abstractions and shared cross-cutting types.
//!
//! All other crates in the workspace depend on this crate.  It has zero UI
//! dependencies.

pub mod config;
pub mod errors;
pub mod fs;
pub mod types;

pub use errors::{CoreError, CoreResult};
