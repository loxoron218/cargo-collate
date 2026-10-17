//! Missing accessibility-label detectors for widget builders.
//!
//! Parent index for the capability. The `labels_alias` submodule resolves
//! import aliases, `labels_trigger` holds builder and label matching,
//! `labels_body` holds balanced body scans, and `widget_builders` holds the
//! widget inventory, so every programmatic widget keeps an accessibility
//! label. No code stays exempt: test files flag like library code.

pub mod labels_alias;
pub mod labels_body;
pub mod labels_trigger;
pub mod widget_builders;

use std::collections::BTreeSet;

use crate::{
    lexer::word::declared_fn_name,
    rules_harness::range::code_of,
    rules_ui::missing_labels::{
        labels_alias::label_aliases,
        labels_body::{fn_body_end, fn_missing_label},
    },
    scan::SourceFile,
};

/// Finding for one declaration line, if it builds without a label.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `aliases` - In-scope label names from imports plus the fallback.
/// * `idx` - Zero-based index of the candidate line.
/// * `stripped` - String-blanked candidate line.
///
/// # Returns
///
/// * `Option<String>` - Hit line when the `fn` builds without a label.
fn fn_hit_at(
    file: &SourceFile,
    aliases: &BTreeSet<String>,
    idx: usize,
    stripped: &str,
) -> Option<String> {
    if declared_fn_name(&code_of(stripped))
        .as_ref()
        .is_none_or(String::is_empty)
    {
        return None;
    }
    let end = fn_body_end(&file.stripped, idx)?;
    fn_missing_label(file, idx, end, aliases)
}

/// Findings for one file with no exemptions.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `aliases` - In-scope label names from imports plus the fallback.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
fn file_missing_labels(file: &SourceFile, aliases: &BTreeSet<String>) -> Vec<String> {
    file.stripped
        .iter()
        .enumerate()
        .filter_map(|(idx, stripped)| fn_hit_at(file, aliases, idx, stripped))
        .collect()
}

/// Widget builders without an accessibility label in the same `fn`.
///
/// Flags every `fn` whose balanced body holds an inventoried widget
/// `Type::builder` plus `.build(` without `update_property` plus an
/// import-resolved label alias. Nothing stays exempt: test modules,
/// integration trees, layout types, and dialogs all flag. String literals
/// and line tails never count since bodies scan blanked code.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with a fix hint appended.
#[must_use]
pub fn missing_labels_hits(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            let aliases = label_aliases(&file.lines);
            file_missing_labels(file, &aliases)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::missing_labels::missing_labels_hits, scan::builders::test_file};

    #[test]
    fn builders_without_labels_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn",
                "}",
            ],
        )];
        let hits = missing_labels_hits(&files);
        assert_eq!(hits.len(), 1, "bare builder flags");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
            "fn line flagged"
        );
    }

    #[test]
    fn labels_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn.update_property(&[Label(\"Select item\")]);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_labels_hits(&files).is_empty(),
            "labelled builder stays silent"
        );
    }

    #[test]
    fn generic_object_builders_need_labels() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn build_overlay() -> ToastOverlay {",
                "    let overlay = glib::Object::builder::<ToastOverlay>()",
                "        .property(\"child\", &content)",
                "        .build();",
                "    overlay",
                "}",
            ],
        )];
        assert_eq!(
            missing_labels_hits(&files).len(),
            1,
            "unlabeled generic builder flags"
        );
        let clean = vec![test_file(
            "src/b.rs",
            &[
                "fn build_overlay() -> ToastOverlay {",
                "    let overlay = glib::Object::builder::<ToastOverlay>()",
                "        .property(\"child\", &content)",
                "        .build();",
                "    overlay.update_property(&[Label(\"Overlay\")]);",
                "    overlay",
                "}",
            ],
        )];
        assert!(
            missing_labels_hits(&clean).is_empty(),
            "labelled generic builder stays silent"
        );
    }

    #[test]
    fn aliases_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::accessible::Property::Label as PropertyLabel;",
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn.update_property(&[PropertyLabel(\"Select item\")]);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_labels_hits(&files).is_empty(),
            "aliased label stays silent"
        );
        let grouped = vec![test_file(
            "src/b.rs",
            &[
                "use gtk::{Button, accessible::Property::Label as A11yLabel};",
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn.update_property(&[A11yLabel(\"Select item\")]);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_labels_hits(&grouped).is_empty(),
            "grouped alias stays silent"
        );
    }

    #[test]
    fn non_widget_builders_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "fn build_client() -> Client {",
                    "    Client::builder().build()",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "fn signals() -> Vec<Signal> {",
                    "    vec![Signal::builder(\"x\").build()]",
                    "}",
                ],
            ),
        ];
        assert!(
            missing_labels_hits(&files).is_empty(),
            "non-widget builders stay silent"
        );
    }

    #[test]
    fn multiline_shapes_resolve() {
        let flagged = vec![test_file(
            "src/a.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .icon_name(\"x\")",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            missing_labels_hits(&flagged).len(),
            1,
            "multiline builder flags"
        );
        let clean = vec![test_file(
            "src/b.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .icon_name(\"x\")",
                "        .build();",
                "    btn.update_property(&[",
                "        Label(\"Select item\")",
                "    ]);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            missing_labels_hits(&clean).is_empty(),
            "multiline label stays silent"
        );
    }

    #[test]
    fn lookalikes_stay_silent() {
        let files = vec![
            test_file("src/a.rs", &["fn helper() -> i32 { 1 }"]),
            test_file(
                "src/b.rs",
                &[
                    "fn f() {",
                    "    let s = \"Button::builder().build()\";",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "fn f() {",
                    "    let btn = Button::builder().build();",
                    "    btn.update_property(&[Labels(\"x\")]);",
                    "}",
                ],
            ),
        ];
        let hits = missing_labels_hits(&files);
        assert_eq!(hits.len(), 1, "only bare builder flags");
        assert!(
            hits.first().is_some_and(|hit| hit.starts_with("src/c.rs")),
            "near-miss label still flags"
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
                "        let btn = Button::builder().build();",
                "        btn",
                "    }",
                "}",
            ],
        )];
        assert_eq!(
            missing_labels_hits(&inline).len(),
            1,
            "inline test modules flag"
        );
        let integration = vec![test_file(
            "tests/flow.rs",
            &[
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            missing_labels_hits(&integration).len(),
            1,
            "integration tests flag"
        );
    }
}
