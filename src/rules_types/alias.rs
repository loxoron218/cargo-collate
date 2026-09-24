//! Unnecessary type alias detection.
//!
//! Parent index for the capability: `discovery` collects free-standing
//! `type` aliases, `extract` pulls each use's enclosing type, `sites`
//! substitutes, scores, and renders the warning-free verdict, `scope`
//! decides which mentions can resolve to the alias, `generics` maps
//! parameters to applied arguments, and `macros` masks unresolvable
//! interiors. Neighboring lints stay ignored since they fix without a new
//! `type` alias. Capability tests live in `cases` with regression coverage
//! in `regression`, both gated for test builds.

pub mod discovery;
pub mod extract;
pub mod generics;
pub mod head;
pub mod macros;
pub mod modules;
pub mod scope;
pub mod sites;

#[cfg(test)]
mod cases;
#[cfg(test)]
mod regression;
