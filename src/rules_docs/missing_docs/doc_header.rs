//! Missing `//!` module docs on files.
//!
//! Rustc `missing_docs` only covers public items and Clippy
//! `missing_docs_in_private_items` stays silent in test builds, so test
//! helpers like `stray/tests.rs` keep no header warning-free. This syntactic
//! detector requires `//!` on line 1 of every file instead.
//!
//! Split from the missing-docs driver to keep single-file detectors under
//! the project file-length limit.

use crate::scan::SourceFile;

/// True if `line` opens with a `//!` module doc marker.
///
/// Quadruple `////` sequences stay exempt like `///` handling.
fn is_mod_doc_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//!") && !trimmed.starts_with("////")
}

/// Finding for `file` when line 1 holds no `//!` docs, if any.
///
/// Only the first line counts; blanks, attributes, or code before `//!`
/// flag. The stripped line decides so string contents never count, while
/// the original line displays in the hit.
fn file_missing_mod_doc(file: &SourceFile) -> Option<String> {
    let stripped = file.stripped.first().map_or("", String::as_str);
    if is_mod_doc_line(stripped) {
        return None;
    }
    let line = file.lines.first().map_or("", String::as_str);
    Some(format!("{}:1:{line}", file.path))
}

/// Files without leading `//!` module docs.
///
/// Every file needs `//!` on line 1, including test helpers, integration
/// tests, and inner-gated files; unlike `missing_fn_docs` there is no test
/// exemption.
pub fn missing_mod_docs(files: &[SourceFile]) -> Vec<String> {
    files.iter().filter_map(file_missing_mod_doc).collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_docs::missing_docs::doc_header::missing_mod_docs, scan::support::test_file};

    #[test]
    fn missing_first_line_flagged() {
        let files = vec![
            test_file("src/a.rs", &["use foo::Bar;"]),
            test_file("src/b.rs", &["//! Docs."]),
            test_file("src/c.rs", &[""]),
        ];
        let hits = missing_mod_docs(&files);
        assert_eq!(hits.len(), 2, "bare and blank flag");
        assert!(
            hits.iter().any(|hit| hit.starts_with("src/a.rs:1:")),
            "bare file flagged"
        );
        assert!(
            hits.iter().any(|hit| hit.starts_with("src/c.rs:1:")),
            "blank file flagged"
        );
    }

    #[test]
    fn late_docs_flagged() {
        let files = vec![
            test_file("src/a.rs", &["", "//! Docs."]),
            test_file("src/b.rs", &["#[cfg(test)]", "//! Docs."]),
            test_file("src/c.rs", &["/*! Docs. */"]),
        ];
        assert_eq!(missing_mod_docs(&files).len(), 3, "non-first docs flag");
    }

    #[test]
    fn doc_shapes() {
        let files = vec![
            test_file("src/a.rs", &["//! Docs."]),
            test_file("src/b.rs", &["  //! Indented."]),
        ];
        assert!(
            missing_mod_docs(&files).is_empty(),
            "plain and indented stay silent"
        );
        let bad = vec![
            test_file("src/c.rs", &["//// Rule."]),
            test_file("src/d.rs", &["/// Item docs."]),
            test_file("src/e.rs", &["// Plain comment."]),
            test_file("src/f.rs", &["let s = \"//! Docs.\";"]),
        ];
        assert_eq!(missing_mod_docs(&bad).len(), 4, "lookalikes flag");
    }

    #[test]
    fn helpers_flagged() {
        let files = vec![
            test_file("src/y/cases.rs", &["use crate::x;"]),
            test_file("tests/x.rs", &["fn helper() {}"]),
            test_file("src/d.rs", &["#![cfg(test)]", "fn helper() {}"]),
        ];
        let hits = missing_mod_docs(&files);
        assert_eq!(hits.len(), 3, "test code flags");
        assert!(
            hits.iter().any(|hit| hit.starts_with("src/y/cases.rs:1:")),
            "helper file flagged"
        );
    }

    #[test]
    fn empty_file_flagged() {
        let files = vec![test_file("src/a.rs", &[])];
        let hits = missing_mod_docs(&files);
        assert_eq!(hits.len(), 1, "empty flags");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
            "empty points at line 1"
        );
    }
}
