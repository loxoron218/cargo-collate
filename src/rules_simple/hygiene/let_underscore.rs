//! Let-underscore discard detectors.

use crate::{lexer::word::word_match_at, rules_simple::hygiene::line_hits, scan::SourceFile};

/// True if `code` holds a whole-pattern `let _` discard outside strings.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `let _` discards a value.
#[must_use]
pub fn has_let_underscore(code: &str) -> bool {
    (0..code.len()).any(|i| let_discard_at(code, i))
}

/// True if `let` opens a whole-pattern discard at byte `i`.
///
/// Skips an optional `mut`, then requires a bare `_` with no identifier
/// continuation, followed by `=` directly or a `:` type before `=`.
fn let_discard_at(code: &str, i: usize) -> bool {
    if !word_match_at(code, "let", i) {
        return false;
    }
    let bytes = code.as_bytes();
    let mut j = skip_gap(bytes, i.saturating_add(3));
    if word_match_at(code, "mut", j) {
        j = skip_gap(bytes, j.saturating_add(3));
    }
    if bytes.get(j) != Some(&b'_') || ident_continues(bytes, j.saturating_add(1)) {
        return false;
    }
    j = skip_gap(bytes, j.saturating_add(1));
    if bytes.get(j) == Some(&b'=') {
        return true;
    }
    if bytes.get(j) == Some(&b':') {
        return code
            .get(j.saturating_add(1)..)
            .is_some_and(|tail| tail.contains('='));
    }
    false
}

/// First index at or past `j` holding a non-whitespace byte.
fn skip_gap(bytes: &[u8], mut j: usize) -> usize {
    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
        j = j.saturating_add(1);
    }
    j
}

/// True if the byte at `j` continues an identifier.
fn ident_continues(bytes: &[u8], j: usize) -> bool {
    bytes
        .get(j)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Let-underscore findings across files, strings and line tails ignored.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn let_underscore_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_let_underscore)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::let_underscore::{has_let_underscore, let_underscore_hits},
        scan::builders::test_file,
    };

    #[test]
    fn discards_detected() {
        assert!(has_let_underscore("let _ = f();"));
        assert!(has_let_underscore("let _: bool = close();"));
        assert!(has_let_underscore("let _ : bool = close();"));
        assert!(has_let_underscore("let mut _ = g();"));
        assert!(has_let_underscore("if let _ = opt {}"));
    }

    #[test]
    fn non_discards_silent() {
        assert!(!has_let_underscore("let _x = 1;"));
        assert!(!has_let_underscore("let __ = 1;"));
        assert!(!has_let_underscore("let _;"));
        assert!(!has_let_underscore("let (a, _) = pair;"));
        assert!(!has_let_underscore("for _ in it {}"));
        assert!(!has_let_underscore("fn f() {}"));
    }

    #[test]
    fn blanked_spans_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &["let s = \"let _ = x\";", "// let _ = x"],
        )];
        assert!(
            let_underscore_hits(&files).is_empty(),
            "strings and comments stay silent"
        );
    }

    #[test]
    fn hits_resolve() {
        let files = vec![test_file("src/a.rs", &["fn f() {}", "let _ = f();"])];
        let hits = let_underscore_hits(&files);
        assert_eq!(hits.len(), 1, "one discard reports one hit");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:2:")),
            "hit points at the discard line"
        );
    }
}
