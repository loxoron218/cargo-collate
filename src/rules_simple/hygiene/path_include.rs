//! Path-attribute and include-macro detectors.

use crate::{
    lexer::word::has_word,
    rules_simple::hygiene::{line_hits, marker_at},
    scan::SourceFile,
};

/// True if `code` holds a path attribute outside strings.
///
/// Only `path` as the first token after the opener counts; arguments
/// named `path` inside other attributes stay excluded.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `#[path = ...]` appears.
pub fn has_path_attr(code: &str) -> bool {
    let mut from = 0;
    while let Some(tail) = code.get(from..) {
        let Some(rel) = tail.find("#[") else {
            return false;
        };
        let mut j = from.saturating_add(rel).saturating_add(2);
        while code.as_bytes().get(j).is_some_and(u8::is_ascii_whitespace) {
            j = j.saturating_add(1);
        }
        if code.get(j..).is_some_and(|rest| rest.starts_with("path"))
            && code
                .as_bytes()
                .get(j.saturating_add(4))
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
        {
            return true;
        }
        from = j.saturating_add(1);
    }
    false
}

/// True if `code` holds an include macro call outside strings.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `include!` appears.
#[must_use]
pub fn has_include_macro(code: &str) -> bool {
    if !has_word(code, "include") {
        return false;
    }
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        marker_at(code, &chars, i, "include") && chars.get(i.saturating_add(7)) == Some(&'!')
    })
}

/// True if `code` holds either path marker outside strings.
fn has_path_include(code: &str) -> bool {
    has_path_attr(code) || has_include_macro(code)
}

/// Path-attribute and include-macro findings across files.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn path_include_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_path_include)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::path_include::{
            has_include_macro, has_path_attr, path_include_hits,
        },
        scan::builders::test_file,
    };

    #[test]
    fn path_markers_detected() {
        assert!(has_path_attr("#[path = \"a.rs\"]"));
        assert!(!has_path_attr("#[template(path = \"a.html\")]"));
        assert!(has_include_macro("include!(\"a\")"));
    }

    #[test]
    fn clean_files_silent() {
        let files = vec![test_file("src/a.rs", &["fn f() {}"])];
        assert!(
            path_include_hits(&files).is_empty(),
            "clean files stay silent"
        );
    }
}
