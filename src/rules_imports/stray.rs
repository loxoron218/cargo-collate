//! Stray `use` detection after code or inside blocks.
//!
//! Clippy ordering skips glob imports and block bodies while import grouping
//! restarts past code gaps, so a `use` after items or inside a function body
//! stays silent there; this rule flags those strays at file tops and inside
//! test modules. Macro definitions, `quote!`-family spans, and
//! `compile_error!` guards stay exempt: templates hold no hoistable imports
//! and guards expand to no items.

pub mod classify;
pub mod placement;

#[cfg(test)]
mod tests;

use crate::{rules_imports::stray::placement::check_file, scan::SourceFile};

/// Stray `use` imports after code or inside blocks across all files.
///
/// File tops and test modules keep a header of docs, attributes, `mod`
/// declarations, and `use` items; the first other item seals the header and
/// every later `use` at that level flags, as does any `use` inside a block.
#[must_use]
pub fn stray_uses(files: &[SourceFile]) -> Vec<String> {
    let mut out = Vec::new();
    for file in files {
        out.extend(check_file(file));
    }
    out
}
