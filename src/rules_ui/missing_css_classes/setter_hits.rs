//! Setter findings for missing default classes.
//!
//! Flags post-build `set_css_classes` lists whose receiver resolves to an
//! inventoried widget builder in the same `fn` while omitting required
//! classes. Unresolvable receivers stay silent instead of guessing across
//! functions.

use crate::{
    lexer::word::declared_fn_name,
    rules_tests::range::code_of,
    rules_ui::{
        missing_css_classes::{
            class_require::collect_missing,
            css_parse::{list_for, method_open_at, missing_hint, original_slice},
            setter_resolve::{resolve_widget, setter_var_at},
            setter_scan::{newlines_before, paren_end},
        },
        missing_labels::labels_body::{fn_body_end, fn_body_text},
    },
    scan::SourceFile,
};

/// Required classes missing from the setter at `dot`, if any.
///
/// Resolves the receiver through its `let` binding, slices the original
/// setter lines for list parsing, then checks always-required plus builder
/// triggers from the backing chain.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `fn_start` - Zero-based index of the `fn` declaration line.
///
/// * `dot` - Body-relative byte index of the setter dot.
///
/// # Returns
///
/// * `Option<String>` - Hit line when defaults are missing.
fn setter_hit_at(file: &SourceFile, body: &str, fn_start: usize, dot: usize) -> Option<String> {
    let open = method_open_at(body, dot, "set_css_classes")?;
    let var = setter_var_at(body, dot)?;
    let (widget, segment) = resolve_widget(body, &var, dot)?;
    let close = paren_end(body, open)?;
    let setter_line = fn_start
        .saturating_add(1)
        .saturating_add(newlines_before(body, dot));
    let end_line = fn_start
        .saturating_add(1)
        .saturating_add(newlines_before(body, close));
    let original = original_slice(file, setter_line, end_line);
    let classes = list_for(&original, "set_css_classes")?;
    let missing = collect_missing(&widget, &segment, &classes);
    if missing.is_empty() {
        return None;
    }
    let line = file
        .lines
        .get(setter_line.saturating_sub(1))
        .map_or("", String::as_str);
    Some(format!(
        "{}:{setter_line}:{line} help: add {} to this {widget} setter or use add_css_class after \
         build",
        file.path,
        missing_hint(&missing)
    ))
}

/// Setter findings for the `fn` at `idx`, if any.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// * `idx` - Zero-based index of the `fn` declaration line.
///
/// * `end` - Exclusive end index past the balanced closer.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
fn fn_setter_hits(file: &SourceFile, idx: usize, end: usize) -> Vec<String> {
    let body = fn_body_text(&file.stripped, idx, end);
    let bytes = body.as_bytes();
    let mut hits = Vec::new();
    let mut dot: usize = 0;
    while dot < bytes.len() {
        if let Some(hit) = setter_hit_at(file, &body, idx, dot) {
            hits.push(hit);
        }
        dot = dot.saturating_add(1);
    }
    hits
}

/// Setter findings for the declaration at `idx`, if it opens a `fn`.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// * `idx` - Zero-based line index under scan.
///
/// * `stripped` - String-blanked candidate line.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines for this declaration.
fn idx_setter_hits(file: &SourceFile, idx: usize, stripped: &str) -> Vec<String> {
    if declared_fn_name(&code_of(stripped))
        .as_ref()
        .is_none_or(String::is_empty)
    {
        return Vec::new();
    }
    fn_body_end(&file.stripped, idx).map_or_else(Vec::new, |end| fn_setter_hits(file, idx, end))
}

/// Setter findings for one file.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
fn file_setter_hits(file: &SourceFile) -> Vec<String> {
    file.stripped
        .iter()
        .enumerate()
        .flat_map(|(idx, stripped)| idx_setter_hits(file, idx, stripped))
        .collect()
}

/// Setter default-class findings across files.
///
/// Merges per-file scans in order so overlapping triggers report once.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
#[must_use]
pub fn setter_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = Vec::new();
    for file in files {
        hits.extend(file_setter_hits(file));
    }
    hits
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::missing_css_classes::setter_hits::setter_hits, scan::support::test_file,
    };

    #[test]
    fn spinner_setter_needs_class() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn spinner() {",
                "    let s = Spinner::builder().build();",
                "    s.set_css_classes(&[\"flat\"]);",
                "}",
            ],
        )];
        let hits = setter_hits(&files);
        assert_eq!(hits.len(), 1, "missing spinner flags");
        assert!(
            hits.first().is_some_and(|hit| hit.contains("spinner")),
            "hint names spinner"
        );
    }

    #[test]
    fn spinner_setter_satisfied() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn spinner() {",
                "    let s = Spinner::builder().build();",
                "    s.set_css_classes(&[\"spinner\"]);",
                "}",
            ],
        )];
        assert!(setter_hits(&files).is_empty(), "present stays silent");
    }

    #[test]
    fn bare_receivers_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn spinner(s: &Spinner) {",
                "    s.set_css_classes(&[\"flat\"]);",
                "}",
            ],
        )];
        assert!(setter_hits(&files).is_empty(), "arg receivers stay silent");
    }

    #[test]
    fn label_trigger_carries_to_setter() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn confirm() {",
                "    let b = Button::builder().label(\"Save\").build();",
                "    b.set_css_classes(&[\"suggested-action\"]);",
                "}",
            ],
        )];
        let hits = setter_hits(&files);
        assert_eq!(hits.len(), 1, "builder label carries");
        assert!(
            hits.first().is_some_and(|hit| hit.contains("text-button")),
            "hint names text-button"
        );
    }
}
