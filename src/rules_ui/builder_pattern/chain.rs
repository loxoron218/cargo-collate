//! Incomplete builder-chain detectors for widget styling properties.
//!
//! Flags inventoried widget `Type::builder()` chains that reach `.build(`
//! without `.css_classes(`, `.tooltip_text(`, or `.can_focus(`, so every
//! programmatic widget keeps its styling, tooltip, and focus behavior inside
//! the builder.

use crate::{
    rules_ui::{
        builder_pattern::{
            file_scan_hits,
            trigger::{builder_widget_at, has_method_call, is_build_open_at},
        },
        missing_labels::labels_trigger::is_builder_open_at,
    },
    scan::SourceFile,
};

/// Builder properties required on every widget chain.
const REQUIRED_PROPS: [&str; 3] = ["css_classes", "tooltip_text", "can_focus"];

/// Maximum bytes from `::builder` to its `.build(` terminator.
///
/// Chains separate on `;` or a new `::builder` well inside this window, so a
/// longer gap means the builder never terminates rather than a missed chain.
const CHAIN_SCAN_LIMIT: usize = 4000;

/// Required properties missing from `segment`.
///
/// # Arguments
///
/// * `segment` - Builder chain text between `::builder` and `.build(`.
///
/// # Returns
///
/// * `Vec<&str>` - Missing property names in required order.
fn missing_props(segment: &str) -> Vec<&str> {
    REQUIRED_PROPS
        .iter()
        .copied()
        .filter(|prop| !has_method_call(segment, prop))
        .collect()
}

/// Index of the `.build(` terminating the chain at `from`, if any.
///
/// Stops on `;` or a new `::builder` first, so stored intermediates and
/// consecutive chains never pair across statements.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `from` - Byte index of the opening `::builder` marker.
///
/// # Returns
///
/// * `Option<usize>` - Byte index of the terminating dot, if it closes here.
fn chain_build_at(body: &str, from: usize) -> Option<usize> {
    let bytes = body.as_bytes();
    let limit = from.saturating_add(CHAIN_SCAN_LIMIT);
    let mut idx = from.saturating_add("::builder".len());
    while idx < bytes.len() && idx < limit {
        if is_build_open_at(bytes, idx) {
            return Some(idx);
        }
        if bytes.get(idx) == Some(&b';') {
            return None;
        }
        if is_builder_open_at(bytes, idx) {
            return None;
        }
        idx = idx.saturating_add(1);
    }
    None
}

/// Hint listing the missing builder calls.
///
/// # Arguments
///
/// * `missing` - Missing property names in required order.
///
/// # Returns
///
/// * `String` - Comma-separated `.name(...)` spellings.
fn missing_hint(missing: &[&str]) -> String {
    missing
        .iter()
        .map(|prop| format!(".{prop}(...)"))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Finding for the builder chain opening at `idx`, if incomplete.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `text` - Joined code text with strings blanked and tails cut.
/// * `idx` - Byte index of the candidate `::builder` marker.
/// * `lineno` - One-based line number holding `idx`.
///
/// # Returns
///
/// * `Option<String>` - Hit line with a fix hint when properties are missing.
fn chain_hit_at(file: &SourceFile, text: &str, idx: usize, lineno: usize) -> Option<String> {
    if !is_builder_open_at(text.as_bytes(), idx) {
        return None;
    }
    let widget = builder_widget_at(text, idx)?;
    let end = chain_build_at(text, idx)?;
    let segment = text.get(idx..end).map_or("", |part| part);
    let missing = missing_props(segment);
    if missing.is_empty() {
        return None;
    }
    let line = file
        .lines
        .get(lineno.saturating_sub(1))
        .map_or("", String::as_str);
    Some(format!(
        "{}:{lineno}:{line} help: add {} to this {widget} builder",
        file.path,
        missing_hint(&missing)
    ))
}

/// Incomplete builder-chain findings for one file.
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

/// Incomplete builder-chain findings across files.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
pub fn chain_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(file_chain_hits).collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::builder_pattern::chain::chain_hits, scan::support::test_file};

    #[test]
    fn incomplete_builders_flag_missing() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn build_button() -> Button {",
                "    let btn = Button::builder().build();",
                "    btn",
                "}",
            ],
        )];
        let hits = chain_hits(&files);
        assert_eq!(hits.len(), 1, "bare builder flags once");
        assert!(
            hits.first().is_some_and(|hit| hit.contains("css_classes")
                && hit.contains("tooltip_text")
                && hit.contains("can_focus")),
            "hint names every missing property"
        );
    }

    #[test]
    fn partial_builders_flag_remainder() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .css_classes([\"circular\"])",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        let hits = chain_hits(&files);
        assert_eq!(hits.len(), 1, "partial builder flags once");
        assert!(
            hits.first()
                .is_some_and(|hit| !hit.contains("css_classes") && hit.contains("tooltip_text")),
            "present property stays out of the hint"
        );
    }

    #[test]
    fn complete_builders_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn build_button() -> Button {",
                "    let btn = Button::builder()",
                "        .css_classes([\"circular\"])",
                "        .tooltip_text(\"Select\")",
                "        .can_focus(true)",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        assert!(
            chain_hits(&files).is_empty(),
            "complete builder stays silent"
        );
    }

    #[test]
    fn non_widget_builders_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    Client::builder().build();",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    Object::builder().property(\"x\", 1).build();",
                    "}",
                ],
            ),
        ];
        assert!(
            chain_hits(&files).is_empty(),
            "non-widget and generic builders stay silent"
        );
    }

    #[test]
    fn lookalikes_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let s = \"Button::builder().build()\";",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "fn f() {",
                    "    let b: ButtonBuilder = gtk::builders::ButtonBuilder::new();",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let b = Button::builder();",
                    "}",
                ],
            ),
        ];
        assert!(
            chain_hits(&files).is_empty(),
            "strings, module paths, and unterminated builders stay silent"
        );
    }
}
