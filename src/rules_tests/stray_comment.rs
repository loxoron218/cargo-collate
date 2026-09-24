//! Stray comment detection inside test modules.

use crate::{
    lexer::{literal::strip_strings, marker::plain_comment_at},
    rules_tests::range::{in_ranges, stray_gap, test_ranges},
    scan::SourceFile,
};

/// True if `line` is a full-line comment.
fn is_full_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// True if `line` is a doc comment that never counts as stray.
fn is_doc_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    (trimmed.starts_with("///") && !trimmed.starts_with("////")) || trimmed.starts_with("//!")
}

/// End of the full-line comment block starting at `idx`.
fn comment_block_end(lines: &[String], idx: usize, ranges: &[(usize, usize)]) -> usize {
    let mut j = idx;
    while let Some(line) = lines.get(j) {
        if !is_full_comment(line) || (j != idx && !in_ranges(ranges, j)) {
            break;
        }
        j = j.saturating_add(1);
    }
    j
}

/// Findings and resume index for the comment at `idx`, if any.
///
/// Classification runs on blanked lines while display keeps original lines.
fn stray_at(file: &SourceFile, ranges: &[(usize, usize)], idx: usize) -> (Vec<String>, usize) {
    let Some(line) = file.stripped.get(idx) else {
        return (Vec::new(), idx.saturating_add(1));
    };
    if !strip_strings(line).contains("//") || !in_ranges(ranges, idx) {
        return (Vec::new(), idx.saturating_add(1));
    }
    if is_doc_comment(line) {
        return (Vec::new(), idx.saturating_add(1));
    }
    if !is_full_comment(line) {
        return (trailing_comment_hit(file, idx), idx.saturating_add(1));
    }
    let end = comment_block_end(&file.stripped, idx, ranges);
    if file
        .stripped
        .get(idx..end)
        .is_some_and(comment_block_exempt)
    {
        return (Vec::new(), end);
    }
    let hits = (idx..end)
        .filter_map(|k| {
            file.lines
                .get(k)
                .map(|original| format!("{}:{}:{original}", file.path, k.saturating_add(1)))
        })
        .collect();
    (hits, end)
}

/// True if a comment block is an exempt errors section with summary.
fn comment_block_exempt(block: &[String]) -> bool {
    block
        .iter()
        .position(|b| b.contains("# Errors") || b.contains("# Panics"))
        .is_some_and(|h| h > 0)
}

/// Trailing finding at `idx`, unless it documents an exempt section.
fn trailing_comment_hit(file: &SourceFile, idx: usize) -> Vec<String> {
    let (Some(code), Some(line)) = (file.stripped.get(idx), file.lines.get(idx)) else {
        return Vec::new();
    };
    let stripped = strip_strings(code);
    let comment =
        plain_comment_at(&stripped).map_or("", |p| stripped.get(p..).map_or("", |part| part));
    if comment.contains("# Errors") || comment.contains("# Panics") {
        Vec::new()
    } else {
        vec![format!("{}:{}:{line}", file.path, idx.saturating_add(1))]
    }
}

/// Stray comments inside test modules.
#[must_use]
pub fn stray_test_comments(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            let ranges = test_ranges(&file.lines, stray_gap);
            let mut idx = 0;
            let mut out = Vec::new();
            while idx < file.lines.len() {
                let (hits, next) = stray_at(file, &ranges, idx);
                out.extend(hits);
                idx = next;
            }
            out
        })
        .filter(|l| l.contains("//"))
        .collect()
}
