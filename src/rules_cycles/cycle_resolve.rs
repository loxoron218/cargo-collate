//! Import path resolution for cycle detection.
//!
//! Maps resolved module paths onto owning files, drops direct parent-child
//! pairs, and detects cfg-test-gated statements.

use std::collections::BTreeMap;

use crate::{rules_simple::cfg_test::is_real_cfg_line, scan::SourceFile};

/// Indexed modules backing the dependency graph.
#[derive(Debug)]
pub struct ModuleIndex {
    /// Owning file path per package and dotted module, smallest path wins.
    pub by_module: BTreeMap<(String, String), String>,
    /// Dotted module path per indexed file path.
    pub dot_of: BTreeMap<String, String>,
}

/// Owning file of the longest indexed module prefix of a resolved path.
///
/// # Arguments
///
/// * `full` - Resolved absolute module path with the item tail included.
/// * `pkg` - Package dir of the importing file.
/// * `index` - Module lookup with dotted names per file.
/// * `source` - Importing file path for self-edge checks.
///
/// # Returns
///
/// * `Option<String>` - Target file path, unless the match is the source.
#[must_use]
pub fn longest_module_target(
    full: &[String],
    pkg: &str,
    index: &ModuleIndex,
    source: &str,
) -> Option<String> {
    let mut len = full.len();
    while len > 0 {
        let dot = full
            .iter()
            .take(len)
            .cloned()
            .collect::<Vec<String>>()
            .join("::");
        len = len.saturating_sub(1);
        let Some(target) = index.by_module.get(&(pkg.to_owned(), dot)) else {
            continue;
        };
        if target.as_str() == source {
            return None;
        }
        return Some(target.clone());
    }
    None
}

/// True if one file module strictly contains the other.
///
/// Parent aggregation and child back-references across one nesting level stay
/// silent; only sideways cycles flag.
///
/// # Arguments
///
/// * `index` - Module lookup with dotted names per file.
/// * `source` - Importing file path.
/// * `target` - Imported file path.
///
/// # Returns
///
/// * `bool` - True for direct parent-child pairs.
#[must_use]
pub fn is_prefix_pair(index: &ModuleIndex, source: &str, target: &str) -> bool {
    let (Some(from), Some(into)) = (index.dot_of.get(source), index.dot_of.get(target)) else {
        return false;
    };
    if from == into {
        return false;
    }
    prefix_of(from, into) || prefix_of(into, from)
}

/// True if `outer` is a strict dotted prefix of `inner` by whole segments.
///
/// # Arguments
///
/// * `outer` - Candidate parent module path.
/// * `inner` - Candidate child module path.
///
/// # Returns
///
/// * `bool` - True when `inner` extends `outer`.
#[must_use]
pub fn prefix_of(outer: &str, inner: &str) -> bool {
    if outer.is_empty() {
        return !inner.is_empty();
    }
    inner.len() > outer.len()
        && inner.starts_with(outer)
        && inner
            .as_bytes()
            .get(outer.len())
            .is_some_and(|b| *b == b':')
}

/// True if a `use` statement carries an attached cfg-test marker.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `attr_start` - Zero-based line where attached attributes start.
/// * `use_idx` - Zero-based line where the `use` keyword sits.
///
/// # Returns
///
/// * `bool` - True when the attached block gates on cfg-test.
#[must_use]
pub fn stmt_test_gated(file: &SourceFile, attr_start: usize, use_idx: usize) -> bool {
    for idx in attr_start..=use_idx {
        if file
            .lines
            .get(idx)
            .is_some_and(|line| is_real_cfg_line(line))
        {
            return true;
        }
    }
    false
}
