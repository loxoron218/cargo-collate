//! Dot-ok error-swallow detectors.

use crate::{rules_simple::hygiene::line_hits, scan::SourceFile};

/// True if `code` holds a dot-ok error swallow outside strings.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `.ok()` appears.
#[must_use]
pub fn has_dot_ok(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        chars.get(i).is_some_and(|c| *c == '.')
            && chars.get(i.saturating_add(1)) == Some(&'o')
            && chars.get(i.saturating_add(2)) == Some(&'k')
            && chars.get(i.saturating_add(3)) == Some(&'(')
            && chars.get(i.saturating_add(4)) == Some(&')')
    })
}

/// Dot-ok findings across files, strings and line tails ignored.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn dot_ok_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_dot_ok)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::dot_ok::{dot_ok_hits, has_dot_ok},
        scan::support::test_file,
    };

    #[test]
    fn dot_ok_detected() {
        assert!(has_dot_ok("a.ok()"));
        assert!(!has_dot_ok("let x = 1;"));
    }

    #[test]
    fn clean_files_silent() {
        let files = vec![test_file("src/a.rs", &["fn f() {}"])];
        assert!(dot_ok_hits(&files).is_empty(), "clean files stay silent");
    }
}
