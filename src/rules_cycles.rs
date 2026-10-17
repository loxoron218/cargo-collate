//! Circular module dependencies across the source tree.
//!
//! Parent index for the capability. `cycle_graph` maps files onto modules
//! and collects import edges, `cycle_resolve` maps paths onto owning files,
//! and `cycle_search` prunes acyclic files and formats one hit per cycle
//! participant.

pub mod cycle_graph;
pub mod cycle_resolve;
pub mod cycle_search;

use crate::{
    rules_cycles::{
        cycle_graph::{build_edges, index_modules},
        cycle_search::{cycle_members, format_hits},
    },
    scan::SourceFile,
};

/// Files participating in circular module dependencies.
///
/// Only sideways `crate`-, `self`-, `super`-, own-package-rooted, and bare
/// local imports build edges; external crates, globs, self-edges,
/// test-gated imports, and direct parent-child pairs stay silent. Each
/// participant reports its cycle starting from itself for determinism.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines, one per file in a cycle, sorted.
#[must_use]
pub fn circular_deps(files: &[SourceFile]) -> Vec<String> {
    let index = index_modules(files);
    if index.dot_of.is_empty() {
        return Vec::new();
    }
    let graph = build_edges(files, &index);
    let live = cycle_members(&graph.adj);
    format_hits(files, &index, &graph, &live)
}

#[cfg(test)]
mod tests {
    use crate::{rules_cycles::circular_deps, scan::builders::test_file};

    #[test]
    fn pair_cycle_flagged() {
        let files = vec![
            test_file("src/alpha.rs", &["use crate::beta::Thing;"]),
            test_file("src/beta.rs", &["use crate::alpha::Widget;"]),
        ];
        let hits = circular_deps(&files);
        assert_eq!(hits.len(), 2, "two-file cycle flags both files");
        assert!(
            hits.iter().all(|hit| hit.contains("circular dependency")),
            "hits name the cycle"
        );
    }

    #[test]
    fn triple_cycle_flagged() {
        let files = vec![
            test_file("src/a_one.rs", &["use crate::b_two::Thing;"]),
            test_file("src/b_two.rs", &["use crate::c_three::Thing;"]),
            test_file("src/c_three.rs", &["use crate::a_one::Thing;"]),
        ];
        assert_eq!(circular_deps(&files).len(), 3, "three-file cycle flags all");
    }

    #[test]
    fn diamond_stays_silent() {
        let files = vec![
            test_file(
                "src/top_main.rs",
                &["use crate::{left_arm::A, right_arm::B};"],
            ),
            test_file("src/left_arm.rs", &["use crate::base_leg::A;"]),
            test_file("src/right_arm.rs", &["use crate::base_leg::B;"]),
            test_file("src/base_leg.rs", &["pub struct A;", "pub struct B;"]),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "diamond without a back edge stays silent"
        );
    }

    #[test]
    fn one_way_stays_silent() {
        let files = vec![
            test_file("src/client.rs", &["use crate::server::Thing;"]),
            test_file("src/server.rs", &["pub struct Thing;"]),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "one-way imports stay silent"
        );
    }

    #[test]
    fn self_edge_stays_silent() {
        let files = vec![test_file(
            "src/solo.rs",
            &["use crate::solo::Helper;", "pub struct Helper;"],
        )];
        assert!(
            circular_deps(&files).is_empty(),
            "self-edges stay silent for this lint"
        );
    }

    #[test]
    fn external_imports_stay_silent() {
        let files = vec![
            test_file(
                "src/first_ext.rs",
                &["use std::collections::HashMap;", "use serde::Serialize;"],
            ),
            test_file("src/second_ext.rs", &["use first_ext::Thing;"]),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "one-way bare imports stay silent"
        );
    }

    #[test]
    fn gated_edge_stays_silent() {
        let files = vec![
            test_file("src/real_one.rs", &["pub struct Thing;"]),
            test_file(
                "src/real_two.rs",
                &[
                    "pub struct Widget;",
                    "#[cfg(test)]",
                    "mod tests {",
                    "    use crate::real_one::Thing;",
                    "}",
                ],
            ),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "edges inside test modules stay silent"
        );
        let gated = vec![
            test_file("src/real_one.rs", &["use crate::real_two::Widget;"]),
            test_file(
                "src/real_two.rs",
                &["#[cfg(test)]", "use crate::real_one::Thing;"],
            ),
        ];
        assert!(
            circular_deps(&gated).is_empty(),
            "cfg-test-gated uses stay silent"
        );
    }

    #[test]
    fn parent_child_stays_silent() {
        let files = vec![
            test_file("src/top.rs", &["use crate::top::inner::Gadget;"]),
            test_file("src/top/inner.rs", &["use super::Widget;"]),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "parent aggregation with child back-references stays silent"
        );
    }

    #[test]
    fn sibling_super_cycle_flagged() {
        let files = vec![
            test_file("src/pack/left.rs", &["use super::right::Gadget;"]),
            test_file("src/pack/right.rs", &["use super::left::Widget;"]),
        ];
        assert_eq!(
            circular_deps(&files).len(),
            2,
            "sideways super paths join the same cycle"
        );
    }

    #[test]
    fn bare_sibling_cycle_flagged() {
        let files = vec![
            test_file("src/uno.rs", &["use dos::Thing;"]),
            test_file("src/dos.rs", &["use uno::Widget;"]),
        ];
        assert_eq!(
            circular_deps(&files).len(),
            2,
            "bare local roots join the same cycle"
        );
    }

    #[test]
    fn root_aggregation_stays_silent() {
        let files = vec![
            test_file(
                "src/main.rs",
                &[
                    "pub mod alpha;",
                    "pub use alpha::Thing;",
                    "pub struct Root;",
                ],
            ),
            test_file("src/alpha.rs", &["use crate::Root;"]),
        ];
        assert!(
            circular_deps(&files).is_empty(),
            "root aggregation with root-item back-references stays silent"
        );
    }
}
