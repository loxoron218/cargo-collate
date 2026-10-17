//! Missing underline detectors for keyboard mnemonics.
//!
//! Parent index for the capability. The `mnemonic_widgets` submodule holds
//! the mnemonic-capable inventory, `underline_trigger` holds builder and
//! underline matching, `mnemonic_text` holds literal scans, `label_calls`
//! holds label-argument spans, and `underline_body` holds balanced body
//! scans, so mnemonic `_X` label text keeps underline enablement. No code
//! stays exempt: test files flag like library code.

pub mod label_calls;
pub mod mnemonic_text;
pub mod mnemonic_widgets;
pub mod underline_body;
pub mod underline_trigger;

use crate::{rules_ui::missing_underline::underline_body::underline_hits, scan::SourceFile};

/// Mnemonic label text without underline enablement.
///
/// Flags every `fn` whose label-bearing call arguments hold a word-opening
/// `"_X"` literal on a mnemonic-capable widget without
/// `.use_underline(true)` or `.set_use_underline(true)` in the same `fn`.
/// Icon-only builders, dynamic labels, and non-mnemonic widgets stay
/// silent; test modules, integration trees, and dialogs all flag otherwise.
/// String literals and line tails never count as enablement since bodies scan
/// blanked code while literals scan original text.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with a fix hint appended.
#[must_use]
pub fn missing_underline_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = underline_hits(files);
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::missing_underline::missing_underline_hits, scan::builders::test_file};

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
        let hits = missing_underline_hits(&files);
        assert_eq!(hits.len(), 1, "labeled mnemonic builder flags");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
            "fn line flagged"
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
            missing_underline_hits(&icon).is_empty(),
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
            missing_underline_hits(&dynamic).is_empty(),
            "dynamic labels stay silent"
        );
    }

    #[test]
    fn satisfied_builders_stay_silent() {
        let builder = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .label(\"_Open\")",
                "        .use_underline(true)",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_underline_hits(&builder).is_empty(),
            "builder spelling stays silent"
        );
        let setter = vec![test_file(
            "src/b.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().label(\"_Open\").build();",
                "    btn.set_use_underline(true);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_underline_hits(&setter).is_empty(),
            "setter spelling stays silent"
        );
    }

    #[test]
    fn direct_mnemonics_flag() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::new(\"_Open\");",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            missing_underline_hits(&files).len(),
            1,
            "direct mnemonic flags"
        );
        let clean = vec![test_file(
            "src/b.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::new(\"_Open\");",
                "    btn.set_use_underline(true);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_underline_hits(&clean).is_empty(),
            "satisfied direct stays silent"
        );
    }

    #[test]
    fn non_widgets_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["fn build_box() -> Box {", "    Box::builder().build()", "}"],
            ),
            test_file("src/b.rs", &["fn helper() -> i32 { 1 }"]),
            test_file(
                "src/c.rs",
                &[
                    "fn build_about() -> AboutDialog {",
                    "    AboutDialog::builder().title(\"_About\").build()",
                    "}",
                ],
            ),
        ];
        assert!(
            missing_underline_hits(&files).is_empty(),
            "non-mnemonic builders stay silent"
        );
    }

    #[test]
    fn placeholders_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button(name: &str) -> Button {",
                "    let btn = Button::builder().label(name).build();",
                "    btn.set_tooltip_text(&format!(\"{app_id}\"));",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_underline_hits(&files).is_empty(),
            "placeholders stay silent"
        );
    }

    #[test]
    fn lookalikes_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "fn f() {",
                    "    let s = \"Button::builder().build()\";",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "fn build_button() -> Button {",
                    "    let btn = Button::builder()",
                    "        .use_underline(true)",
                    "        .build();",
                    "    btn",
                    "}",
                ],
            ),
        ];
        assert!(
            missing_underline_hits(&files).is_empty(),
            "strings and satisfied builders stay silent"
        );
    }

    #[test]
    fn gated_code_flags() {
        let inline = vec![test_file(
            "src/a.rs",
            &[
                "#[cfg(test)]",
                "mod tests {",
                "    fn build_button() -> Button {",
                "        let btn = Button::builder().label(\"_Open\").build();",
                "        btn",
                "    }",
                "}",
            ],
        )];
        assert_eq!(
            missing_underline_hits(&inline).len(),
            1,
            "inline test modules flag"
        );
        let integration = vec![test_file(
            "tests/flow.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::new(\"_Open\");",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            missing_underline_hits(&integration).len(),
            1,
            "integration tests flag"
        );
    }
}
