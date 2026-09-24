//! Expect-attribute detectors for lint suppressions.

use crate::{rules_simple::hygiene::line_hits, scan::SourceFile};

/// Index after ASCII whitespace from `j`.
fn skip_ws(bytes: &[u8], mut j: usize) -> usize {
    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
        j = j.saturating_add(1);
    }
    j
}

/// Index after the attribute opener from `i`, if any.
///
/// Matches `#` `!` `[` with whitespace allowed between tokens, covering
/// both outer and inner forms.
fn opener_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'#') {
        return None;
    }
    let mut j = skip_ws(bytes, i.saturating_add(1));
    if bytes.get(j) == Some(&b'!') {
        j = skip_ws(bytes, j.saturating_add(1));
    }
    if bytes.get(j) != Some(&b'[') {
        return None;
    }
    Some(skip_ws(bytes, j.saturating_add(1)))
}

/// True if `expect` opens at `j` with an identifier boundary after it.
fn is_expect_at(code: &str, bytes: &[u8], j: usize) -> bool {
    code.get(j..j.saturating_add(6)) == Some("expect")
        && bytes
            .get(j.saturating_add(6))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// True if `code` holds an expect attribute outside strings.
///
/// Matches `#` `[` `expect` with whitespace allowed between tokens, so
/// `expected`, `expect_err`, and `.expect(` stay excluded.
#[must_use]
pub fn has_expect_attr(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hit = opener_end(bytes, i).is_some_and(|j| is_expect_at(code, bytes, j));
        if hit {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// Expect-attribute findings across files, strings and tails ignored.
pub fn expect_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_expect_attr)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::expect::{expect_hits, has_expect_attr},
        scan::support::test_file,
    };

    #[test]
    fn expect_attr_flagged() {
        assert!(has_expect_attr("#[expect(dead_code)]"));
        assert!(has_expect_attr("#[expect(clippy::all, reason = \"x\")]"));
        assert!(has_expect_attr("# [ expect ( clippy::foo ) ]"));
        assert!(has_expect_attr("    #[expect(dead_code)]"));
        assert!(has_expect_attr("#![expect(dead_code)]"));
        let files = vec![test_file(
            "src/a.rs",
            &["#[expect(dead_code)]", "fn f() {}"],
        )];
        assert_eq!(expect_hits(&files).len(), 1);
    }

    #[test]
    fn expect_lookalikes_silent() {
        assert!(!has_expect_attr("let expected = 1;"));
        assert!(!has_expect_attr("let x = foo.expect(\"msg\");"));
        assert!(!has_expect_attr("let x = foo.expect_err(\"msg\");"));
        assert!(!has_expect_attr("#[expected]"));
        assert!(!has_expect_attr("#[expectation]"));
        assert!(!has_expect_attr("fn f() {}"));
        let clean = vec![test_file("src/a.rs", &["fn f() {}", "let x = 1;"])];
        assert!(expect_hits(&clean).is_empty(), "clean files stay silent");
    }

    #[test]
    fn expect_in_strings_and_tails_silent() {
        let files = vec![
            test_file("src/a.rs", &[r##"let s = "#[expect(dead_code)]";"##]),
            test_file("src/b.rs", &["let x = 1; // #[expect(dead_code)]"]),
        ];
        assert!(
            expect_hits(&files).is_empty(),
            "strings and tails stay silent"
        );
    }
}
