//! Tests for mixed mod-group detection.

use crate::{rules_tests::mod_groups::mixed_mod_groups, scan::support::test_file};

#[test]
fn clean_example_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[macro_use]",
            "mod macros;",
            "",
            "pub mod auth;",
            "mod content;",
            "pub mod favorites;",
            "mod http_client;",
            "pub mod requests;",
            "mod response;",
            "pub mod retrieval;",
            "pub mod service;",
            "",
            "#[cfg(test)]",
            "pub mod fixture;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "grouped mods stay silent"
    );
}

#[test]
fn pub_split_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["pub mod auth;", "", "mod content;"],
    )];
    let hits = mixed_mod_groups(&files);
    assert_eq!(hits.len(), 1, "visibility split flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":3:")),
        "second line flagged"
    );
}

#[test]
fn missing_blank_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["#[macro_use]", "mod macros;", "pub mod auth;"],
    )];
    assert_eq!(
        mixed_mod_groups(&files).len(),
        1,
        "missing blank between groups flags"
    );
    let joined = vec![test_file(
        "src/a.rs",
        &["pub mod auth;", "#[cfg(test)]", "pub mod fixture;"],
    )];
    assert_eq!(
        mixed_mod_groups(&joined).len(),
        1,
        "missing blank before cfg-test flags"
    );
}

#[test]
fn wrong_order_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["pub mod auth;", "", "#[macro_use]", "mod macros;"],
    )];
    assert_eq!(
        mixed_mod_groups(&files).len(),
        1,
        "macro_use after ordinary flags"
    );
    let late = vec![test_file(
        "src/a.rs",
        &["#[cfg(test)]", "pub mod fixture;", "", "pub mod auth;"],
    )];
    assert_eq!(
        mixed_mod_groups(&late).len(),
        1,
        "ordinary after cfg-test flags"
    );
}

#[test]
fn inline_and_other_cfg_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "pub mod auth;",
            "mod content;",
            "#[cfg(feature = \"gated\")]",
            "mod gated;",
            "#[cfg(test)]",
            "mod tests {",
            "}",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "inline tests and feature gates stay silent"
    );
    let named = vec![test_file("src/a.rs", &["pub mod auth;", "mod macro_use;"])];
    assert!(
        mixed_mod_groups(&named).is_empty(),
        "a module named macro_use stays ordinary"
    );
}

#[test]
fn same_line_attrs_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[macro_use] mod macros;",
            "",
            "pub mod auth;",
            "mod content;",
            "",
            "#[cfg(test)] pub mod fixture;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "same-line attributes stay silent"
    );
}

#[test]
fn detached_attr_flagged() {
    let files = vec![test_file("src/a.rs", &["#[macro_use]", "", "mod macros;"])];
    assert_eq!(
        mixed_mod_groups(&files).len(),
        1,
        "blank inside attribute statement flags"
    );
}

#[test]
fn trailing_attr_mod_detected() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(any(",
            "    target_os = \"linux\",",
            "    target_os = \"macos\"))] mod first;",
            "",
            "mod second;",
        ],
    )];
    let hits = mixed_mod_groups(&files);
    assert_eq!(
        hits.len(),
        1,
        "trailing-attribute mod participates in grouping"
    );
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":5:")),
        "second mod flagged for blank inside ordinary group"
    );
}

#[test]
fn trailing_cfg_test_mod_grouped_last() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "pub mod auth;",
            "mod content;",
            "",
            "#[cfg(",
            "    test",
            ")] mod fixture;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "trailing split cfg(test) still groups last"
    );
}

#[test]
fn trailing_mod_neighbor_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(any(",
            "    feature = \"a\"))] mod first;",
            "mod second;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "previous trailing-shape mod never attaches as neighbor attribute"
    );
}

#[test]
fn multiline_cfg_blocks_attach() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(any(",
            "    target_os = \"linux\",",
            "    target_os = \"macos\",",
            "))]",
            "pub mod gated;",
            "",
            "#[cfg(test)]",
            "pub mod fixture;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "attached multi-line block stays silent"
    );
    let split = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(any(",
            "    target_os = \"linux\",",
            "))]",
            "pub mod first;",
            "",
            "#[cfg(target_os = \"android\")]",
            "pub mod second;",
        ],
    )];
    let hits = mixed_mod_groups(&split);
    assert_eq!(hits.len(), 1, "blank inside ordinary group flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":7:")),
        "second block flagged like single-line gates"
    );
}

#[test]
fn split_cfg_test_marker_grouped_last() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "pub mod auth;",
            "mod content;",
            "",
            "#[cfg(",
            "    test",
            ")]",
            "pub mod fixture;",
        ],
    )];
    assert!(
        mixed_mod_groups(&files).is_empty(),
        "split cfg(test) marker still groups last"
    );
}

#[test]
fn multiline_detached_attr_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["#[cfg(", "    test", ")]", "", "pub mod fixture;"],
    )];
    assert_eq!(
        mixed_mod_groups(&files).len(),
        1,
        "blank inside multi-line attribute statement flags"
    );
}
