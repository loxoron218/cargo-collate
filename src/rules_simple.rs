//! Text-level rules without cross-line state.
//!
//! Parent index for the capability. `cfg_test` flags repeated inline suites,
//! `comment_lines` flags plain markers, while sibling modules cover
//! dependencies, hygiene, layout, manifests, naming, sync, tracing, and
//! underscore identifiers.

pub mod cfg_test;
pub mod comment_lines;
pub mod dependencies;
pub mod expect;
pub mod hygiene;
pub mod layout;
pub mod manifest;
pub mod naming;
pub mod sync;
pub mod tracing;
pub mod tracing_core;
pub mod tracing_mask;
pub mod tracing_parse;
pub mod tracing_string;
pub mod underscore;
