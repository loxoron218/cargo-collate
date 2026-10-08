//! Balanced `fn` bodies for underline checks.
//!
//! Holds original-text joining plus body-level missing-underline findings, so
//! the driver scans one spelling of `fn` ranges.

use crate::{
    lexer::word::declared_fn_name,
    rules_tests::range::code_of,
    rules_ui::{
        missing_labels::labels_body::{fn_body_end, fn_body_text},
        missing_underline::{
            label_calls::{call_has_literal, label_calls},
            underline_trigger::{has_mnemonic_builder, has_underline},
        },
    },
    scan::SourceFile,
};

/// Finding for the `fn` at `idx`, if its label mnemonics lack underlines.
///
/// Flags mnemonic-capable builder chains whose label-bearing call arguments
/// hold a word-opening `"_X"` literal without either underline spelling in
/// the same body, plus mnemonic-associated constructors (`Button::new`,
/// `with_mnemonic`) with such literals. Icon-only builders, dynamic
/// `.label(var)` text, and non-mnemonic widgets stay silent.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `idx` - Zero-based index of the `fn` declaration line.
/// * `end` - Exclusive end index past the balanced closer.
///
/// # Returns
///
/// * `Option<String>` - Hit line when the body misses underlines.
fn fn_missing_underline(file: &SourceFile, idx: usize, end: usize) -> Option<String> {
    let body = fn_body_text(&file.stripped, idx, end);
    if has_underline(&body) {
        return None;
    }
    let code: Vec<String> = file.stripped.get(idx..end).map_or_else(Vec::new, |slice| {
        slice.iter().map(|line| code_of(line)).collect()
    });
    let orig = file.lines.get(idx..end)?;
    let builder = has_mnemonic_builder(&body);
    let anchored = label_calls(&code)
        .iter()
        .any(|call| (call.assoc_mnemonic || builder) && call_has_literal(orig, call));
    if !anchored {
        return None;
    }
    file.lines.get(idx).map(|line| {
        format!(
            "{}:{}:{line} help: add .use_underline(true) to this builder or \
             .set_use_underline(true) in this fn",
            file.path,
            idx.saturating_add(1)
        )
    })
}

/// Findings for one file.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
fn file_scan(file: &SourceFile) -> Vec<String> {
    let mut hits = Vec::new();
    for (idx, stripped) in file.stripped.iter().enumerate() {
        if declared_fn_name(&code_of(stripped))
            .as_ref()
            .is_none_or(String::is_empty)
        {
            continue;
        }
        let Some(end) = fn_body_end(&file.stripped, idx) else {
            continue;
        };
        if let Some(hit) = fn_missing_underline(file, idx, end) {
            hits.push(hit);
        }
    }
    hits
}

/// Underline findings across files.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
#[must_use]
pub fn underline_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = Vec::new();
    for file in files {
        hits.extend(file_scan(file));
    }
    hits
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::missing_underline::underline_body::underline_hits, scan::support::test_file,
    };

    #[test]
    fn labeled_builders_flag() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().label(\"_Open\").build();",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            underline_hits(&files).len(),
            1,
            "labeled mnemonic builder flags"
        );
    }

    #[test]
    fn unlabeled_builders_stay_silent() {
        let icon = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().icon_name(\"x\").build();",
                "    btn",
                "}",
            ],
        )];
        assert!(
            underline_hits(&icon).is_empty(),
            "icon-only builders stay silent"
        );
        let dynamic = vec![test_file(
            "src/b.rs",
            &[
                "fn build_button(label: &str) -> Button {",
                "    let btn = Button::builder().label(label).build();",
                "    btn",
                "}",
            ],
        )];
        assert!(
            underline_hits(&dynamic).is_empty(),
            "dynamic labels stay silent"
        );
    }

    #[test]
    fn satisfied_bodies_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .use_underline(true)",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        assert!(
            underline_hits(&files).is_empty(),
            "satisfied builder stays silent"
        );
    }
}
