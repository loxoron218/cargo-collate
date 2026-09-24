//! Test-only source-file builders.
//!
//! Scan-discovery tests live here as plain test functions rather than in a
//! nested `mod tests`: this file already carries the single `cfg(test)`
//! gate, and a second one would trip the multi-block lint on `scan.rs`.

use crate::{
    lexer::literal::stripped_lines,
    scan::{SourceFile, discover_roots, scope_for},
};

/// Build a test-only source file from lines.
#[must_use]
pub fn test_file(path: &str, lines: &[&str]) -> SourceFile {
    let owned: Vec<String> = lines.iter().map(|s| (*s).to_owned()).collect();
    let stripped = stripped_lines(&owned);
    SourceFile {
        path: path.to_owned(),
        ws: String::new(),
        lines: owned,
        stripped,
    }
}

#[test]
fn roots_cover_everything() {
    assert_eq!(discover_roots(), vec![".".to_owned()]);
}

#[test]
fn scope_shares_root() {
    assert_eq!(scope_for("a/src/x.rs", "/w"), "/w");
    assert_eq!(scope_for("src/x.rs", "/w"), "/w");
    assert_eq!(scope_for("fuzz/fuzz_targets/x.rs", "/w"), "/w");
    assert!(
        scope_for("", "/w").is_empty(),
        "empty paths stay package-only"
    );
    assert!(
        scope_for("src/x.rs", "").is_empty(),
        "empty roots stay package-only"
    );
}
