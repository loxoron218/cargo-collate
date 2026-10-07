//! Split module-doc detectors for `//!` blocks.

use crate::scan::SourceFile;

/// True if stripped `line` is a full-line `//!` module-doc line.
///
/// Leading whitespace stays allowed so indented inner modules still scan;
/// string-blanked text keeps `//!` inside raw strings empty and silent.
fn is_mod_doc_line(stripped: &str) -> bool {
    stripped.trim_start().starts_with("//!")
}

/// Index of the nearest non-blank line at or above `from`, if any.
///
/// Blank runs of any length resolve to the line they follow, so split
/// module docs flag once on the resuming `//!` line.
fn prev_non_blank(stripped: &[String], from: usize) -> Option<usize> {
    let mut cursor = from;
    loop {
        if stripped
            .get(cursor)
            .is_some_and(|line| !line.trim().is_empty())
        {
            return Some(cursor);
        }
        cursor = cursor.checked_sub(1)?;
    }
}

/// Split module-doc findings for one file.
///
/// Flags a `//!` line preceded by at least one empty line whose nearest
/// non-blank predecessor is another `//!` line.
fn check_file_split_mod_doc(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    for (idx, stripped) in file.stripped.iter().enumerate() {
        if !is_mod_doc_line(stripped) {
            continue;
        }
        let Some(prev_idx) = idx.checked_sub(1) else {
            continue;
        };
        if file
            .stripped
            .get(prev_idx)
            .is_none_or(|line| !line.trim().is_empty())
        {
            continue;
        }
        let Some(above) = prev_non_blank(&file.stripped, prev_idx) else {
            continue;
        };
        if file
            .stripped
            .get(above)
            .is_none_or(|line| !is_mod_doc_line(line))
        {
            continue;
        }
        if let Some(line) = file.lines.get(idx) {
            out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
        }
    }
    out
}

/// Split `//!` blocks separated by empty lines.
///
/// Paragraph breaks in module docs use a bare `//!` line, never an empty
/// line; a `//!` line resumes after one or more empty lines flags itself
/// when the nearest non-blank line above is another `//!` line.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn split_mod_doc_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_split_mod_doc).collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::split_mod_doc::split_mod_doc_hits, scan::support::test_file,
    };

    #[test]
    fn split_mod_doc_flags_bare_blank() {
        let files = vec![test_file(
            "src/a.rs",
            &["//! First line.", "", "//! Second line."],
        )];
        assert_eq!(split_mod_doc_hits(&files).len(), 1);
        let clean = vec![test_file(
            "src/a.rs",
            &["//! First line.", "//!", "//! Second line."],
        )];
        assert!(
            split_mod_doc_hits(&clean).is_empty(),
            "bare //! separators stay silent"
        );
    }

    #[test]
    fn split_mod_doc_edge_shapes() {
        let single = vec![test_file("src/a.rs", &["//! Only line."])];
        assert!(
            split_mod_doc_hits(&single).is_empty(),
            "single line stays silent"
        );
        let short = vec![test_file("src/a.rs", &["//! One.", "//! Two."])];
        assert!(
            split_mod_doc_hits(&short).is_empty(),
            "contiguous lines stay silent"
        );
        let ws_blank = vec![test_file("src/a.rs", &["//! One.", "   ", "//! Two."])];
        assert_eq!(
            split_mod_doc_hits(&ws_blank).len(),
            1,
            "whitespace counts as blank"
        );
        let outer = vec![test_file("src/a.rs", &["/// One.", "", "/// Two."])];
        assert!(
            split_mod_doc_hits(&outer).is_empty(),
            "outer docs stay silent"
        );
        let run = vec![test_file("src/a.rs", &["//! One.", "", "", "//! Two."])];
        assert_eq!(
            split_mod_doc_hits(&run).len(),
            1,
            "blank runs flag once on the resuming line"
        );
        let code_between = vec![test_file(
            "src/a.rs",
            &["//! One.", "", "fn f() {}", "", "//! Two."],
        )];
        assert!(
            split_mod_doc_hits(&code_between).is_empty(),
            "code between blocks stays silent"
        );
        let bof_blank = vec![test_file("src/a.rs", &["", "//! One."])];
        assert!(
            split_mod_doc_hits(&bof_blank).is_empty(),
            "leading blank stays silent"
        );
        let in_string = vec![test_file(
            "src/a.rs",
            &[
                "let q = r#\"",
                "//! One.",
                "",
                "//! Two.",
                "\"#;",
                "fn f() {}",
            ],
        )];
        assert!(
            split_mod_doc_hits(&in_string).is_empty(),
            "raw interiors never flag"
        );
    }
}
