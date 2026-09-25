//! Tests for mixed import-group detection.

use std::collections::BTreeSet;

use crate::{
    rules_imports::{group_findings, mixed_import_groups},
    scan::support::test_file,
};

#[test]
fn clean_groups_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "",
            "use anyhow::Context;",
            "",
            "use crate::other::Thing;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "separated groups stay silent"
    );
}

#[test]
fn missing_blank_flagged() {
    let files = vec![test_file("src/a.rs", &["use std::a;", "use anyhow::b;"])];
    let hits = mixed_import_groups(&files);
    assert_eq!(hits.len(), 1, "missing blank flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":2:")),
        "second line flagged"
    );
}

#[test]
fn wrong_order_flagged() {
    let files = vec![test_file("src/a.rs", &["use crate::o;", "", "use std::a;"])];
    assert_eq!(mixed_import_groups(&files).len(), 1, "reversed order flags");
}

#[test]
fn repeated_split_flagged() {
    let files = vec![
        test_file(
            "src/a.rs",
            &[
                "use std::fs::File;",
                "",
                "use anyhow::Context;",
                "",
                "use std::path::Path;",
            ],
        ),
        test_file("src/b.rs", &["use std::c;", "", "use std::d;"]),
    ];
    assert_eq!(
        mixed_import_groups(&files).len(),
        2,
        "repeats and splits flag"
    );
}

#[test]
fn self_package_benches() {
    let good = test_file(
        "benches/speed.rs",
        &[
            "use criterion::{Criterion, criterion_group, criterion_main};",
            "",
            "use oxhidifi::playback::Converter;",
        ],
    );
    assert!(
        group_findings(&good, Some("oxhidifi"), &BTreeSet::new()).is_empty(),
        "separated self package stays silent"
    );
    let bad = test_file(
        "benches/speed.rs",
        &[
            "use criterion::{Criterion, criterion_group, criterion_main};",
            "use oxhidifi::playback::Converter;",
        ],
    );
    assert_eq!(
        group_findings(&bad, Some("oxhidifi"), &BTreeSet::new()).len(),
        1,
        "joined self package flags"
    );
}

#[test]
fn prompt_shape_silent() {
    let file = test_file(
        "tests/api.rs",
        &[
            "use std::sync::OnceLock;",
            "",
            "use qobuz_api::api::quality::MP3_320;",
            "",
            "use crate::{env_var_or, load_env_file};",
        ],
    );
    assert!(
        group_findings(&file, Some("qobuz_api"), &BTreeSet::new()).is_empty(),
        "prompt shape stays silent"
    );
}

#[test]
fn core_alloc_join_std() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "use core::marker::Sized;",
            "use alloc::vec::Vec;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "core and alloc join std"
    );
}

#[test]
fn consolidated_external_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use {",
            "    anyhow::Context,",
            "    serde::de,",
            "};",
            "",
            "use crate::other;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "consolidated externals stay silent"
    );
}

#[test]
fn mixed_consolidated_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["use {", "    anyhow::Context,", "    crate::other,", "};"],
    )];
    assert_eq!(
        mixed_import_groups(&files).len(),
        1,
        "mixed consolidated block flags"
    );
}

#[test]
fn cfg_attr_gap_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "",
            "#[cfg(feature = \"gated\")]",
            "use anyhow::Context;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "attributed imports stay silent"
    );
}

#[test]
fn inner_runs_checked() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "",
            "use anyhow::Context;",
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    use std::path::Path;",
            "    use crate::other;",
            "}",
        ],
    )];
    let hits = mixed_import_groups(&files);
    assert_eq!(hits.len(), 1, "inner missing blank flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":8:")),
        "inner line flagged"
    );
}

#[test]
fn sibling_modules_join_crate() {
    let files = vec![
        test_file("src/main.rs", &["use gating::thing;", "use crate::other;"]),
        test_file("src/gating.rs", &["pub fn thing() {}"]),
        test_file("tests/a.rs", &["use anyhow::b;", "", "use helper::c;"]),
        test_file("tests/helper.rs", &["pub fn c() {}"]),
    ];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "siblings join crate group"
    );
}

#[test]
fn cfg_unix_trailing_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "",
            "#[cfg(unix)]",
            "use std::os::unix::fs::MetadataExt;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "unconditional then cfg-gated trailing stays silent"
    );
}

#[test]
fn cfg_unix_leading_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(unix)]",
            "use std::os::unix::fs::MetadataExt;",
            "use std::fs::File;",
        ],
    )];
    let hits = mixed_import_groups(&files);
    assert_eq!(hits.len(), 1, "cfg-gated before unconditional flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":3:")),
        "unconditional line flagged for order"
    );
}

#[test]
fn cfg_missing_blank_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::fs::File;",
            "#[cfg(unix)]",
            "use std::os::unix::fs::MetadataExt;",
        ],
    )];
    let hits = mixed_import_groups(&files);
    assert_eq!(hits.len(), 1, "missing blank before cfg flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":3:")),
        "cfg use line flagged"
    );
}

#[test]
fn cfg_same_block_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["#[cfg(unix)]", "use std::a;", "#[cfg(unix)]", "use std::b;"],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "contiguous cfg block stays silent"
    );
}

#[test]
fn cfg_same_block_split_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(unix)]",
            "use std::a;",
            "",
            "#[cfg(unix)]",
            "use std::b;",
        ],
    )];
    let hits = mixed_import_groups(&files);
    assert_eq!(hits.len(), 1, "blank inside cfg block flags");
    assert!(
        hits.first().is_some_and(|hit| hit.contains(":5:")),
        "second cfg use flagged"
    );
}

#[test]
fn cfg_attr_stays_ordinary() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg_attr(unix, allow(dead_code))]",
            "use std::a;",
            "use std::b;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "cfg_attr stays in ordinary group"
    );
}

#[test]
fn detached_cfg_flagged() {
    let files = vec![test_file("src/a.rs", &["#[cfg(unix)]", "", "use std::a;"])];
    assert_eq!(
        mixed_import_groups(&files).len(),
        1,
        "blank inside attributed statement flags"
    );
}

#[test]
fn multiline_cfg_attaches() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::a;",
            "",
            "use anyhow::b;",
            "",
            "#[cfg(any(",
            "    target_os = \"linux\",",
            "))]",
            "use crate::c;",
        ],
    )];
    assert!(
        mixed_import_groups(&files).is_empty(),
        "attached multi-line cfg stays silent"
    );
}
