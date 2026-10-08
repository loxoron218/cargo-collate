//! Balanced `fn` bodies for accessibility checks.
//!
//! Holds brace-balanced body end detection plus body text joining so the
//! driver scans one spelling of `fn` ranges.

use std::collections::BTreeSet;

use crate::{
    rules_tests::range::{brace_delta, code_of},
    rules_ui::missing_labels::labels_trigger::{has_accessible_label, has_builder_trigger},
    scan::SourceFile,
};

/// End line (exclusive) of the `fn` body starting at `start`, if any.
///
/// Accumulates [`brace_delta`] from the declaration line so multiline
/// signatures resolve; single-line bodies close on their own line while
/// declarations without a block return `None`.
///
/// # Arguments
///
/// * `lines` - String-blanked file lines aligned with the original.
/// * `start` - Zero-based index of the `fn` declaration line.
///
/// # Returns
///
/// * `Option<usize>` - Exclusive end index past the balanced closer.
#[must_use]
pub fn fn_body_end(lines: &[String], start: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut opened = false;
    let mut cursor = start;
    while let Some(line) = lines.get(cursor) {
        let code = code_of(line);
        opened = opened || code.contains('{');
        depth = depth.saturating_add(brace_delta(line));
        cursor = cursor.saturating_add(1);
        if opened && depth <= 0 {
            return Some(cursor);
        }
    }
    None
}

/// Body text of the `fn` from `start` to `end` with tails cut.
///
/// # Arguments
///
/// * `lines` - String-blanked file lines aligned with the original.
/// * `start` - Zero-based index of the `fn` declaration line.
/// * `end` - Exclusive end index past the balanced closer.
///
/// # Returns
///
/// * `String` - Joined code lines for trigger and label scans.
#[must_use]
pub fn fn_body_text(lines: &[String], start: usize, end: usize) -> String {
    let mut body = String::new();
    let mut cursor = start;
    while cursor < end {
        if let Some(line) = lines.get(cursor) {
            body.push_str(&code_of(line));
            body.push('\n');
        }
        cursor = cursor.saturating_add(1);
    }
    body
}

/// Finding for the `fn` at `idx`, if it builds without a label.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `idx` - Zero-based index of the `fn` declaration line.
/// * `end` - Exclusive end index past the balanced closer.
/// * `aliases` - In-scope label names from imports plus the fallback.
///
/// # Returns
///
/// * `Option<String>` - Hit line when the body builds without a label.
#[must_use]
pub fn fn_missing_label(
    file: &SourceFile,
    idx: usize,
    end: usize,
    aliases: &BTreeSet<String>,
) -> Option<String> {
    let body = fn_body_text(&file.stripped, idx, end);
    if !has_builder_trigger(&body) || has_accessible_label(&body, aliases) {
        return None;
    }
    file.lines.get(idx).map(|line| {
        format!(
            "{}:{}:{line} help: add update_property(&[Label(...)]) in this fn",
            file.path,
            idx.saturating_add(1)
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{
        rules_ui::missing_labels::labels_body::{fn_body_end, fn_body_text, fn_missing_label},
        scan::support::test_file,
    };

    #[test]
    fn bodies_balance_single_and_multi() {
        let single = vec!["fn f() {}".to_owned()];
        assert_eq!(fn_body_end(&single, 0), Some(1), "single line closes");
        let multi = vec![
            "fn f(".to_owned(),
            "x: i32,".to_owned(),
            ") {".to_owned(),
            "1".to_owned(),
            "}".to_owned(),
        ];
        assert_eq!(fn_body_end(&multi, 0), Some(5), "multiline closes");
        let decl = vec!["fn f();".to_owned()];
        assert_eq!(fn_body_end(&decl, 0), None, "declaration stays empty");
    }

    #[test]
    fn joined_text_cuts_tails() {
        let lines = vec!["fn f() { // tail".to_owned(), "}".to_owned()];
        let body = fn_body_text(&lines, 0, 2);
        assert!(!body.contains("tail"), "tails stay cut");
    }

    #[test]
    fn bare_builders_flag_labels_silent() {
        let aliases = BTreeSet::from(["Label".to_owned()]);
        let bare = test_file("src/a.rs", &["fn f() {", "Button::builder().build();", "}"]);
        assert!(
            fn_missing_label(&bare, 0, 3, &aliases).is_some(),
            "bare builder flags"
        );
        let clean = test_file(
            "src/b.rs",
            &[
                "fn f() {",
                "Button::builder().build();",
                "x.update_property(&[Label(\"\")]);",
                "}",
            ],
        );
        assert!(
            fn_missing_label(&clean, 0, 4, &aliases).is_none(),
            "labelled body stays silent"
        );
    }
}
