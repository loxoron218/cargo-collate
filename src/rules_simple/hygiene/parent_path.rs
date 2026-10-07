//! Parent-module path detectors.

use crate::{
    lexer::word::has_word,
    rules_simple::hygiene::{line_hits, marker_at},
    scan::SourceFile,
};

/// True if `code` holds a parent-module path outside strings.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `super::` appears.
#[must_use]
pub fn has_parent_path(code: &str) -> bool {
    if !has_word(code, "super") {
        return false;
    }
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        marker_at(code, &chars, i, "super")
            && chars.get(i.saturating_add(5)) == Some(&':')
            && chars.get(i.saturating_add(6)) == Some(&':')
    })
}

/// Parent-module findings across files, strings and line tails ignored.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn parent_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_parent_path)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::parent_path::{has_parent_path, parent_hits},
        scan::support::test_file,
    };

    #[test]
    fn parent_paths_detected() {
        assert!(has_parent_path("use super::foo;"));
        assert!(!has_parent_path("use crate::foo;"));
    }

    #[test]
    fn clean_files_silent() {
        let files = vec![test_file("src/a.rs", &["use crate::foo;"])];
        assert!(parent_hits(&files).is_empty(), "crate paths stay silent");
    }
}
