//! Plain line-comment detectors with doc markers exempt.

use crate::{
    lexer::{literal::strip_strings, marker::has_plain_line_comment},
    scan::{SourceFile, collect_line_hits},
};

/// Plain line markers with doc markers exempt.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
#[must_use]
pub fn line_comments(files: &[SourceFile]) -> Vec<String> {
    collect_line_hits(files, |_, _, _, code| {
        has_plain_line_comment(&strip_strings(code))
    })
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::comment_lines::line_comments, scan::support::test_file};

    #[test]
    fn raw_string_comment_lines_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "let q = r#\"",
                "// look, a comment",
                "https://example.com/x",
                "\"#;",
                "fn f() {}",
            ],
        )];
        assert!(
            line_comments(&files).is_empty(),
            "raw interiors never count as comments"
        );
    }

    #[test]
    fn plain_markers_flagged() {
        let files = vec![test_file("src/a.rs", &["fn f() {} // note"])];
        assert_eq!(line_comments(&files).len(), 1);
    }
}
