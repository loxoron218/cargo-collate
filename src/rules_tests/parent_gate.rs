//! Top-level test code without a parent `#[cfg(test)]` gate.
//!
//! Test-only files gate through a parent `#[cfg(test)]` file-module
//! declaration; a declared-but-ungated file holding top-level `#[test]`
//! items flags its first item, so the inner-gate ban never pushes test code
//! into production builds ungated. Parents resolve by path (sibling index,
//! legacy `mod.rs`, or crate root), never by bare stem, so same-named files
//! in other crates stay out of scope.

use std::collections::HashMap;

use crate::{
    lexer::{
        visibility::match_file_mod_decl,
        word::{declared_fn_name, has_word},
    },
    rules_docs::missing_docs::attachment::{is_test_attr, scan_above},
    rules_tests::range::{brace_delta, code_of},
    scan::{SourceFile, indexed_lines},
};

/// True if `path` lives in a covered source tree.
///
/// Integration tests, benches, examples, and fuzz targets build under
/// different harnesses where `cfg(test)` gating cannot apply, so only plain
/// `src/` trees count.
fn in_covered_tree(path: &str) -> bool {
    let in_src = path.contains("/src/") || path.starts_with("src/");
    let exotic = ["tests", "benches", "examples", "fuzz"]
        .iter()
        .any(|leaf| path.split('/').any(|component| component == *leaf));
    in_src && !exotic
}

/// Candidate parent indexes declaring `path` as a direct child.
///
/// Sibling `dir.rs` indexes, legacy `dir/mod.rs`, and crate roots
/// (`dir/lib.rs`, `dir/main.rs`) all count; anything else cannot declare it,
/// which keeps same-named files in other crates invisible here.
fn parent_candidates(path: &str) -> Vec<String> {
    let Some((dir, _)) = path.rsplit_once('/') else {
        return Vec::new();
    };
    [
        format!("{dir}.rs"),
        format!("{dir}/mod.rs"),
        format!("{dir}/lib.rs"),
        format!("{dir}/main.rs"),
    ]
    .into_iter()
    .collect()
}

/// Line of the column-zero `mod` declaration for `stem` in `lines`, if any.
///
/// Column-zero keeps inline modules from posing as file parents; formatting
/// keeps every top-level item unindented, and the semicolon keeps bodied
/// modules out.
fn decl_line(lines: &[String], stem: &str) -> Option<usize> {
    lines.iter().position(|line| {
        line.contains(';')
            && (line.starts_with("mod ") || line.starts_with("pub mod "))
            && match_file_mod_decl(line).as_deref() == Some(stem)
    })
}

/// True if `line` gates for test builds through a combined predicate.
///
/// Covers `all(test, …)` and `any(test, …)` shapes alongside exact
/// `cfg(test)` markers; `not(test)` never counts, and lines without `cfg`
/// stay silent so nearby test-named items never leak in.
fn has_combined_test_gate(line: &str) -> bool {
    let code = code_of(line);
    if code.contains("/*") || code.contains("*/") || !code.contains("cfg") {
        return false;
    }
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let positive = compact.replace("not(test)", "");
    has_word(&positive, "test")
}

/// True if the `mod` declaration at `decl` in `lines` carries test gating.
///
/// Joins up to three attribute lines above the declaration, so single-line
/// combined gates count while unrelated items above stay out of range.
fn decl_test_gated(lines: &[String], decl: usize) -> bool {
    let start = decl.saturating_sub(3);
    lines
        .get(start..decl)
        .is_some_and(|above| above.iter().any(|line| has_combined_test_gate(line)))
}

/// True if `file` has a gated parent declaration in `files`.
fn has_gated_parent(files: &HashMap<&str, &SourceFile>, file: &SourceFile, stem: &str) -> bool {
    parent_candidates(&file.path).iter().any(|parent| {
        files.get(parent.as_str()).is_some_and(|candidate| {
            decl_line(&candidate.lines, stem)
                .is_some_and(|decl| decl_test_gated(&candidate.lines, decl))
        })
    })
}

/// True if `file` has any parent declaration in `files`, gated or not.
fn has_parent(files: &HashMap<&str, &SourceFile>, file: &SourceFile, stem: &str) -> bool {
    parent_candidates(&file.path).iter().any(|parent| {
        files
            .get(parent.as_str())
            .is_some_and(|candidate| decl_line(&candidate.lines, stem).is_some())
    })
}

/// True if the `fn` at `idx` is a top-level test item.
fn is_test_fn(file: &SourceFile, idx: usize, line: &str, stripped: &str) -> bool {
    let code = code_of(stripped);
    if code.contains("/*") || code.contains("*/") {
        return false;
    }
    if declared_fn_name(&code).is_none() {
        return false;
    }
    is_test_attr(line, &code) || scan_above(&file.stripped, idx).tested
}

/// First ungated top-level test item in `file`, if any.
///
/// Gated files and files without a parent declaration stay exempt; only
/// declared-but-ungated files with depth-zero test items flag.
fn ungated_in_file(files: &HashMap<&str, &SourceFile>, file: &SourceFile) -> Option<String> {
    if !in_covered_tree(&file.path) {
        return None;
    }
    let stem = file.path.rsplit('/').next().unwrap_or(&file.path);
    let stem = stem.strip_suffix(".rs").unwrap_or(stem);
    if has_gated_parent(files, file, stem) || !has_parent(files, file, stem) {
        return None;
    }
    let mut depth: i32 = 0;
    for (idx, (line, stripped)) in indexed_lines(file) {
        if depth == 0 && is_test_fn(file, idx, line, stripped) {
            return Some(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
        }
        depth = depth.saturating_add(brace_delta(&code_of(stripped))).max(0);
    }
    None
}

/// Top-level test code in declared-but-ungated file modules.
///
/// Every test-only file needs a parent `#[cfg(test)]` gate; plain source
/// trees only, with special-harness trees and undeclared roots exempt.
#[must_use]
pub fn ungated_test_modules(files: &[SourceFile]) -> Vec<String> {
    let by_path: HashMap<&str, &SourceFile> = files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    files
        .iter()
        .filter_map(|file| ungated_in_file(&by_path, file))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_tests::parent_gate::ungated_test_modules, scan::support::test_file};

    #[test]
    fn gated_file_silent() {
        let files = vec![
            test_file("src/y.rs", &["#[cfg(test)]", "mod cases;"]),
            test_file("src/y/cases.rs", &["#[test]", "fn t() {}"]),
        ];
        assert!(
            ungated_test_modules(&files).is_empty(),
            "parent-gated files stay silent"
        );
    }

    #[test]
    fn ungated_file_flagged() {
        let files = vec![
            test_file("src/y.rs", &["mod cases;"]),
            test_file("src/y/cases.rs", &["#[test]", "fn t() {}"]),
        ];
        assert_eq!(
            ungated_test_modules(&files),
            vec!["src/y/cases.rs:2:fn t() {}".to_owned()],
            "first test item flagged"
        );
    }

    #[test]
    fn only_first_item_flagged() {
        let files = vec![
            test_file("src/y.rs", &["mod helper;"]),
            test_file(
                "src/y/helper.rs",
                &["#[test]", "fn a() {}", "#[test]", "fn b() {}"],
            ),
        ];
        assert_eq!(
            ungated_test_modules(&files).len(),
            1,
            "one hit per file suffices"
        );
    }

    #[test]
    fn root_and_legacy_roots_silent() {
        let root = vec![test_file("src/solo.rs", &["#[test]", "fn t() {}"])];
        assert!(
            ungated_test_modules(&root).is_empty(),
            "undeclared roots stay silent"
        );
        let legacy = vec![
            test_file("pkg/src/y/mod.rs", &["mod cases;"]),
            test_file("pkg/src/y/cases.rs", &["#[test]", "fn t() {}"]),
        ];
        assert_eq!(
            ungated_test_modules(&legacy).len(),
            1,
            "legacy mod.rs parents still bind"
        );
    }

    #[test]
    fn lib_and_main_roots_bind() {
        let files = vec![
            test_file("pkg/src/lib.rs", &["mod helper;"]),
            test_file("pkg/src/helper.rs", &["#[test]", "fn t() {}"]),
        ];
        assert_eq!(
            ungated_test_modules(&files).len(),
            1,
            "crate-root parents bind"
        );
        let gated = vec![
            test_file("pkg/src/lib.rs", &["#[cfg(test)]", "mod helper;"]),
            test_file("pkg/src/helper.rs", &["#[test]", "fn t() {}"]),
        ];
        assert!(
            ungated_test_modules(&gated).is_empty(),
            "gated crate roots stay silent"
        );
    }

    #[test]
    fn other_crate_stems_silent() {
        let files = vec![
            test_file("tools/lintcheck/main.rs", &["mod driver;"]),
            test_file("src/driver.rs", &["#[test]", "fn t() {}"]),
        ];
        assert!(
            ungated_test_modules(&files).is_empty(),
            "same-named files in other crates stay silent"
        );
    }

    #[test]
    fn combined_gates_silent() {
        let files = vec![
            test_file(
                "src/y.rs",
                &["#[cfg(all(test, feature = \"x\"))]", "mod cases;"],
            ),
            test_file("src/y/cases.rs", &["#[test]", "fn t() {}"]),
        ];
        assert!(
            ungated_test_modules(&files).is_empty(),
            "combined test predicates stay silent"
        );
        let negated = vec![
            test_file("src/z.rs", &["#[cfg(not(test))]", "mod plain;"]),
            test_file("src/z/plain.rs", &["#[test]", "fn t() {}"]),
        ];
        assert_eq!(
            ungated_test_modules(&negated).len(),
            1,
            "negated gates still flag"
        );
    }

    #[test]
    fn inline_suite_silent() {
        let files = vec![
            test_file("src/owner.rs", &["mod inner;"]),
            test_file(
                "src/owner/inner.rs",
                &["#[cfg(test)]", "mod tests {", "#[test]", "fn t() {}", "}"],
            ),
        ];
        assert!(
            ungated_test_modules(&files).is_empty(),
            "nested tests stay silent"
        );
    }

    #[test]
    fn special_trees_and_lookalikes_silent() {
        let integration = vec![test_file("tests/x.rs", &["#[test]", "fn t() {}"])];
        assert!(
            ungated_test_modules(&integration).is_empty(),
            "integration tests stay silent"
        );
        let benched = vec![
            test_file("benches/bench.rs", &["mod helper;"]),
            test_file("benches/helper.rs", &["#[bench]", "fn b() {}"]),
        ];
        assert!(
            ungated_test_modules(&benched).is_empty(),
            "bench trees stay silent"
        );
        let files = vec![
            test_file("src/y.rs", &["mod plain;"]),
            test_file("src/y/plain.rs", &["#[cfg(not(test))]", "fn f() {}"]),
        ];
        assert!(
            ungated_test_modules(&files).is_empty(),
            "negated gates stay silent"
        );
        let commented = vec![
            test_file("src/z.rs", &["mod documented;"]),
            test_file("src/z/documented.rs", &["// #[test]", "fn f() {}"]),
        ];
        assert!(
            ungated_test_modules(&commented).is_empty(),
            "commented markers stay silent"
        );
    }
}
