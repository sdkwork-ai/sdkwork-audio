//! SDKWork Audio generation service ports.
//!
//! Declares the repository port traits and the shared DTO/entity types that
//! service crates depend on. Concrete `*-repository-sqlx` crates implement
//! these ports and are injected at runtime by the assembly; service crates
//! must never depend on a concrete repository crate.

pub mod entities;
pub mod repositories;

pub use entities::*;
pub use repositories::*;