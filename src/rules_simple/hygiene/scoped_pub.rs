//! Scoped-visibility detectors.

use crate::{rules_simple::hygiene::line_hits, scan::SourceFile};

/// True if a scoped visibility opens at char `i`.
fn opens_scope_at(code: &str, chars: &[char], i: usize) -> bool {
    if !code.get(i..).is_some_and(|rest| rest.starts_with("pub")) {
        return false;
    }
    if chars
        .get(i.saturating_add(3))
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        return false;
    }
    if i > 0
        && chars
            .get(i.saturating_sub(1))
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        return false;
    }
    let mut j = i.saturating_add(3);
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j = j.saturating_add(1);
    }
    chars.get(j).is_some_and(|c| *c == '(')
}

/// True if `code` holds scoped pub visibility outside strings and tails.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `pub(...)` appears.
#[must_use]
pub fn has_scoped_pub(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| opens_scope_at(code, &chars, i))
}

/// Scoped-visibility findings across files, strings and line tails ignored.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn scoped_pub_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_scoped_pub)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::scoped_pub::{has_scoped_pub, scoped_pub_hits},
        scan::builders::test_file,
    };

    #[test]
    fn scoped_pub_detected() {
        assert!(has_scoped_pub("pub(crate) fn f() {}"));
        assert!(has_scoped_pub("let x = foo!(bar => pub(crate));"));
        assert!(!has_scoped_pub("pub fn f() {}"));
    }

    #[test]
    fn clean_files_silent() {
        let files = vec![test_file("src/a.rs", &["pub fn f() {}"])];
        assert!(scoped_pub_hits(&files).is_empty(), "plain pub stays silent");
    }
}
