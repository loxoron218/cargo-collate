//! Builder-chain detectors for missing default classes.
//!
//! Flags inventoried widget `Type::builder()` chains whose `css_classes`
//! list omits always-required or triggered content classes, so programmatic
//! builders keep `toggle`, `view`, `image-button`, and their kin instead of
//! wiping them. Chains without `css_classes` stay silent for the
//! builder-pattern lint while dynamic variables stay silent here.

use crate::{
    rules_ui::{
        builder_pattern::{
            file_scan_hits,
            trigger::{builder_widget_at, chain_build_at, has_method_call},
        },
        missing_css_classes::{
            class_require::collect_missing,
            css_parse::{list_for, missing_hint, original_slice},
        },
        missing_labels::labels_trigger::is_builder_open_at,
    },
    scan::SourceFile,
};

/// Newlines inside `segment`.
///
/// # Arguments
///
/// * `segment` - Builder chain text between markers.
///
/// # Returns
///
/// * `usize` - Count of newline bytes in the segment.
fn span_lines(segment: &str) -> usize {
    let mut count: usize = 0;
    for byte in segment.bytes() {
        if byte == b'\n' {
            count = count.saturating_add(1);
        }
    }
    count
}

/// Widget with its stripped chain text at `idx`, if complete.
///
/// Bundles inventory resolution with terminator pairing so callers read one
/// shaped lookup instead of repeating boundary checks.
///
/// # Arguments
///
/// * `text` - Joined code text with strings blanked and tails cut.
///
/// * `idx` - Byte index of the candidate `::builder` marker.
///
/// # Returns
///
/// * `Option<(String, String)>` - Widget basename with its stripped segment.
fn chain_segment_at(text: &str, idx: usize) -> Option<(String, String)> {
    let widget =
        builder_widget_at(text, idx).filter(|_| is_builder_open_at(text.as_bytes(), idx))?;
    let end = chain_build_at(text, idx)?;
    let segment = text.get(idx..end)?.to_owned();
    Some((widget.to_owned(), segment))
}

/// Required classes missing from the chain at `idx`, if any.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// * `text` - Joined code text with strings blanked and tails cut.
///
/// * `idx` - Byte index of the candidate `::builder` marker.
///
/// * `lineno` - One-based line number holding `idx`.
///
/// # Returns
///
/// * `Option<String>` - Hit line with a fix hint when defaults are missing.
fn chain_hit_at(file: &SourceFile, text: &str, idx: usize, lineno: usize) -> Option<String> {
    let (widget, stripped) = chain_segment_at(text, idx)?;
    if !has_method_call(&stripped, "css_classes") {
        return None;
    }
    let build_lineno = lineno.saturating_add(span_lines(&stripped));
    let original = original_slice(file, lineno, build_lineno.saturating_add(1));
    let classes = list_for(&original, "css_classes")?;
    let missing = collect_missing(&widget, &stripped, &classes);
    if missing.is_empty() {
        return None;
    }
    let line = file
        .lines
        .get(lineno.saturating_sub(1))
        .map_or("", String::as_str);
    Some(format!(
        "{}:{lineno}:{line} help: add {} to this {widget} builder or use add_css_class after build",
        file.path,
        missing_hint(&missing)
    ))
}

/// Builder-chain findings for one file.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
fn file_chain_hits(file: &SourceFile) -> Vec<String> {
    file_scan_hits(file, chain_hit_at)
}

/// Builder-chain default-class findings across files.
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
pub fn css_chain_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = Vec::new();
    for file in files {
        hits.extend(file_chain_hits(file));
    }
    hits
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::missing_css_classes::css_chain::css_chain_hits, scan::support::test_file,
    };

    #[test]
    fn toggle_without_default_flags() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::ToggleButton;",
                "fn toggle() {",
                "    let t = ToggleButton::builder().css_classes([\"flat\"]).build();",
                "}",
            ],
        )];
        let hits = css_chain_hits(&files);
        assert_eq!(hits.len(), 1, "missing toggle flags");
        assert!(
            hits.first().is_some_and(|hit| hit.contains("toggle")),
            "hint names toggle"
        );
    }

    #[test]
    fn toggle_with_default_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::ToggleButton;",
                "fn toggle() {",
                "    let t = ToggleButton::builder()",
                "        .css_classes([\"toggle\", \"flat\"])",
                "        .tooltip_text(\"Toggle\")",
                "        .build();",
                "}",
            ],
        )];
        assert!(css_chain_hits(&files).is_empty(), "present stays silent");
    }

    #[test]
    fn labeled_button_needs_text() {
        let flagged = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn action() {",
                "    let b = Button::builder()",
                "        .label(\"Open\")",
                "        .css_classes([\"suggested-action\"])",
                "        .build();",
                "}",
            ],
        )];
        assert_eq!(flagged.len(), 1, "triggered absent flags");
        assert!(
            css_chain_hits(&flagged)
                .first()
                .is_some_and(|hit| { hit.contains("text-button") }),
            "hint names text-button"
        );
        let plain = vec![test_file(
            "src/b.rs",
            &[
                "use gtk::Button;",
                "fn action() {",
                "    let b = Button::builder().css_classes([\"flat\"]).build();",
                "}",
            ],
        )];
        assert!(
            css_chain_hits(&plain).is_empty(),
            "untriggered stays silent"
        );
    }

    #[test]
    fn bare_chain_belongs_elsewhere() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn action() {",
                "    let b = Button::builder().build();",
                "}",
            ],
        )];
        assert!(
            css_chain_hits(&files).is_empty(),
            "absent call belongs to chain lint"
        );
    }
}
