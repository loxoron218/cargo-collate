//! Builder-pattern detectors for widget styling properties.
//!
//! Parent index for the capability. The `box_shape` submodule counts call
//! arguments, the `chain` submodule flags inventoried widget builders
//! missing styling calls, `direct` flags direct construction and post-build
//! setters, `redundant` flags explicit focus restatements, `setters` holds
//! setter-call matching, `trigger` holds
//! call-boundary matching, `roots` holds
//! import-root resolution, and shared text scanning lives here so both
//! directions read identical analysis text. Only gtk-family files scan:
//! same-named foreign types stay silent. Nothing else stays exempt: test code
//! flags like library code.

pub mod box_shape;
pub mod chain;
pub mod direct;
pub mod redundant;
pub mod roots;
pub mod setters;
pub mod trigger;

use crate::{
    lexer::{marker::cut_line_comment, word::has_word},
    rules_ui::builder_pattern::{
        chain::chain_hits, direct::direct_hits, redundant::redundant_focus_hits,
    },
    scan::SourceFile,
};

/// Crates whose word marks a file as widget-building UI code.
const UI_CRATES: [&str; 4] = ["gtk", "gtk4", "libadwaita", "adw"];

/// Joined code text of `file` with tails cut.
///
/// String contents stay blanked from the precomputed lines while comment
/// tails drop out, so triggers never fire inside literals or comments. One
/// original line maps to one row plus its newline.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `String` - Analysis text for builder scans.
#[must_use]
pub fn code_text(file: &SourceFile) -> String {
    let mut text = String::new();
    for line in &file.stripped {
        text.push_str(cut_line_comment(line));
        text.push('\n');
    }
    text
}

/// True if `file` builds UI widgets.
///
/// Requires a gtk-family word anywhere in blanked code outside comments, so
/// same-named types from other ecosystems (`std::boxed::Box`, audio windows,
/// TUI text) stay silent while fully-qualified and imported widget paths
/// both count.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `bool` - True for gtk-family files.
fn is_ui_file(file: &SourceFile) -> bool {
    UI_CRATES.iter().any(|mark| {
        file.stripped
            .iter()
            .any(|line| has_word(cut_line_comment(line), mark))
    })
}

/// Findings of one file scan with one-based line tracking.
///
/// Runs `hit_at` at every byte offset of the joined code text, so multiline
/// calls attribute to their opening line. Files outside the gtk family stay
/// silent up front.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `hit_at` - Finding for the offset and line holding it, if any.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines in scan order.
#[must_use]
pub fn file_scan_hits(
    file: &SourceFile,
    hit_at: fn(&SourceFile, &str, usize, usize) -> Option<String>,
) -> Vec<String> {
    if !is_ui_file(file) {
        return Vec::new();
    }
    let text = code_text(file);
    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    let mut lineno: usize = 1;
    let mut idx = 0;
    while idx < bytes.len() {
        if let Some(hit) = hit_at(file, &text, idx, lineno) {
            hits.push(hit);
        }
        if bytes.get(idx) == Some(&b'\n') {
            lineno = lineno.saturating_add(1);
        }
        idx = idx.saturating_add(1);
    }
    hits
}

/// Builder-pattern findings across files.
///
/// Merges incomplete chains with redundant focus calls plus direct
/// construction and setter hits, sorted
/// and deduplicated so overlapping triggers report once.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
#[must_use]
pub fn builder_pattern_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = chain_hits(files);
    hits.extend(redundant_focus_hits(files));
    hits.extend(direct_hits(files));
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::builder_pattern::builder_pattern_hits, scan::support::test_file};

    #[test]
    fn standards_example_stays_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn build_action_button() -> Button {",
                "    let btn = Button::builder()",
                "        .css_classes([\"circular\"])",
                "        .tooltip_text(\"Select item\")",
                "        .build();",
                "    btn.update_property(&[Label(\"Select item\")]);",
                "    btn",
                "}",
            ],
        )];
        assert!(
            builder_pattern_hits(&files).is_empty(),
            "complete builder stays silent"
        );
    }

    #[test]
    fn violations_combine() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn build_button() -> Button {",
                "    let bare = Button::builder().build();",
                "    let direct = Button::new(\"label\");",
                "    bare.set_css_classes(&[]);",
                "    bare",
                "}",
            ],
        )];
        assert_eq!(
            builder_pattern_hits(&files).len(),
            3,
            "chain, direct, and setter each flag"
        );
    }

    #[test]
    fn gated_code_flags() {
        let inline = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
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
            builder_pattern_hits(&inline).len(),
            1,
            "inline test modules flag"
        );
        let integration = vec![test_file(
            "tests/flow.rs",
            &[
                "use gtk::Button;",
                "fn build_button() -> Button {",
                "    let btn = Button::new(\"label\");",
                "    btn",
                "}",
            ],
        )];
        assert_eq!(
            builder_pattern_hits(&integration).len(),
            1,
            "integration tests flag"
        );
    }

    #[test]
    fn foreign_types_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn f() {",
                "    let boxed = Box::new(value);",
                "    let pinned = Box::pin(work());",
                "}",
            ],
        )];
        assert!(
            builder_pattern_hits(&files).is_empty(),
            "files without gtk words stay silent"
        );
    }
}
