//! Test-module and layout rules parent index.

pub mod boundary;
pub mod inner_gate;
pub mod mod_groups;
pub mod order;
pub mod parent_gate;
pub mod range;
pub mod stray_comment;
pub mod test_docs;

#[cfg(test)]
mod checks;

use crate::{
    rules_simple::{cfg_test_files, multi_cfg_files},
    rules_tests::{
        boundary::{anyhow_leak, anyhow_site, missing_context},
        inner_gate::inner_test_gates,
        mod_groups::mixed_mod_groups,
        order::module_order,
        parent_gate::ungated_test_modules,
        stray_comment::stray_test_comments,
    },
    scan::{SourceFile, load_files},
    show,
};

/// Test-module and error-type rules.
pub fn check_test_rules(output: &mut String, files: &[SourceFile]) -> bool {
    let test_sources = load_files(&cfg_test_files(files));
    let mut fail = false;
    fail |= show(
        output,
        "Stray comments inside cfg-test modules with docs exempt.",
        &stray_test_comments(&test_sources),
    );
    fail |= show(
        output,
        "Files with more than one inline cfg-test module; one mod tests per file.",
        &multi_cfg_files(files),
    );
    fail |= show(
        output,
        "Inner cfg-test gates; use cfg(test) mod tests instead.",
        &inner_test_gates(files),
    );
    fail |= show(
        output,
        "Top-level test code without a parent cfg(test) gate; gate this module from the parent \
         instead.",
        &ungated_test_modules(files),
    );
    fail |= show(
        output,
        "Anyhow in public API; library uses typed thiserror enums.",
        &anyhow_leak(files),
    );
    fail |= show(
        output,
        "Anyhow outside tests and binaries; library uses typed errors.",
        &anyhow_site(files),
    );
    fail |= show(
        output,
        "Bare ? without context, including inline anyhow! errors; use .context() or \
         .with_context().",
        &missing_context(files),
    );
    fail |= show(
        output,
        "Module order with tests at bottom; keep declarations at top with actual tests.",
        &module_order(files),
    );
    fail |= show(
        output,
        "Mixed mod groups; separate macro_use, ordinary, and cfg(test) blocks with blank lines.",
        &mixed_mod_groups(files),
    );
    fail
}
