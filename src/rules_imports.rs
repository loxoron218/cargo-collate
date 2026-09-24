//! Import-group separation for `use` blocks.
//!
//! Parent index for the capability. The `origin` submodule resolves package
//! names and local modules while `rank` classifies single imports; this file
//! keeps the driver plus the statement scan.

pub mod origin;
pub mod rank;
pub mod stray;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    rules_imports::{
        origin::{locals_for, package_names, top_modules_by_package},
        rank::{classify, is_use_start},
    },
    rules_simple::manifest::package_dir_for,
    scan::SourceFile,
};

/// One joined `use` statement with its group rank.
struct UseStmt {
    /// Zero-based line where the statement starts.
    start: usize,
    /// Zero-based line where the statement ends.
    end: usize,
    /// Group rank from `std` to `crate`.
    group: u8,
    /// True when one consolidated block mixes several groups.
    mixed: bool,
}

/// Mixed import groups across every `use` block in all files.
///
/// Groups stay ordered `std`, external, self-package, `crate`, separated by
/// blank lines with no blanks inside a group.
#[must_use]
pub fn mixed_import_groups(files: &[SourceFile]) -> Vec<String> {
    let modules = top_modules_by_package(files);
    let names = package_names(files);
    files
        .iter()
        .flat_map(|file| check_file(file, &modules, &names))
        .collect()
}

/// Findings for one scanned file with per-package lookups.
fn check_file(
    file: &SourceFile,
    modules: &BTreeMap<(String, String), BTreeSet<String>>,
    names: &BTreeMap<String, Option<String>>,
) -> Vec<String> {
    let pkg = package_dir_for(&file.path).to_owned();
    let pkg_name = names.get(&pkg).cloned().flatten();
    let fallback = BTreeSet::new();
    let locals = locals_for(modules, &pkg, &file.path, &fallback);
    group_findings(&file.path, &file.lines, pkg_name.as_deref(), locals)
}

/// Findings for file lines with a resolved package name and locals.
fn group_findings(
    path: &str,
    lines: &[String],
    pkg_name: Option<&str>,
    locals: &BTreeSet<String>,
) -> Vec<String> {
    let stmts = collect_uses(lines, pkg_name, locals);
    let mut out = Vec::new();
    for stmt in &stmts {
        if stmt.mixed {
            push_hit(&mut out, path, lines, stmt.start);
        }
    }
    let mut prev: Option<&UseStmt> = None;
    let mut peak = 0;
    for stmt in &stmts {
        let Some(before) = prev else {
            peak = stmt.group;
            prev = Some(stmt);
            continue;
        };
        if !same_run(lines, before.end.saturating_add(1), stmt.start) {
            peak = stmt.group;
            prev = Some(stmt);
            continue;
        }
        if pair_hit(lines, before, stmt, peak) {
            push_hit(&mut out, path, lines, stmt.start);
        }
        peak = peak.max(stmt.group);
        prev = Some(stmt);
    }
    out
}

/// True when an adjacent pair in one run needs a finding.
fn pair_hit(lines: &[String], before: &UseStmt, stmt: &UseStmt, peak: u8) -> bool {
    let blank = gap_has_blank(lines, before.end.saturating_add(1), stmt.start);
    let ordered = stmt.group >= peak;
    let separated = (stmt.group == before.group) != blank;
    !ordered || !separated
}

/// Push one `path:line:content` finding for the statement at `start`.
pub fn push_hit(out: &mut Vec<String>, path: &str, lines: &[String], start: usize) {
    if let Some(line) = lines.get(start) {
        out.push(format!("{path}:{}:{line}", start.saturating_add(1)));
    }
}

/// Joined `use` statements of one file with group ranks.
fn collect_uses(
    lines: &[String],
    pkg_name: Option<&str>,
    locals: &BTreeSet<String>,
) -> Vec<UseStmt> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < lines.len() {
        let Some(line) = lines.get(idx) else {
            break;
        };
        if !is_use_start(cut_line_comment(&strip_strings(line))) {
            idx = idx.saturating_add(1);
            continue;
        }
        let start = idx;
        let mut joined = cut_line_comment(&strip_strings(line)).to_owned();
        let end = stmt_end(lines, start, &mut joined);
        if !joined.contains(';') {
            break;
        }
        let (group, mixed) = classify(&joined, pkg_name, locals);
        out.push(UseStmt {
            start,
            end,
            group,
            mixed,
        });
        idx = end.saturating_add(1);
    }
    out
}

/// End line of the statement starting at `start`, joining into `joined`.
fn stmt_end(lines: &[String], start: usize, joined: &mut String) -> usize {
    let mut end = start;
    while !joined.contains(';') {
        let Some(next) = lines.get(end.saturating_add(1)) else {
            break;
        };
        end = end.saturating_add(1);
        joined.push('\n');
        joined.push_str(cut_line_comment(&strip_strings(next)));
    }
    end
}

/// True for lines ignored between two `use` statements.
fn is_gap_filler(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// True when the gap holds at least one blank line.
fn gap_has_blank(lines: &[String], from: usize, to: usize) -> bool {
    (from..to).any(|i| lines.get(i).is_some_and(|line| line.trim().is_empty()))
}

/// True when only filler lines sit between two statements.
fn same_run(lines: &[String], from: usize, to: usize) -> bool {
    (from..to).all(|i| {
        lines.get(i).is_some_and(|line| {
            let stripped = strip_strings(line);
            let code = cut_line_comment(&stripped);
            is_gap_filler(code)
        })
    })
}

#[cfg(test)]
mod tests {
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
            group_findings(&good.path, &good.lines, Some("oxhidifi"), &BTreeSet::new()).is_empty(),
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
            group_findings(&bad.path, &bad.lines, Some("oxhidifi"), &BTreeSet::new()).len(),
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
            group_findings(&file.path, &file.lines, Some("qobuz_api"), &BTreeSet::new()).is_empty(),
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
}
