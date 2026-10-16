//! Redundant focus-enablement detectors for widget builders.
//!
//! Flags explicit `.can_focus(true)` and `.set_can_focus(true)` calls, which
//! restate the base default on every widget type, plus `.can_focus(false)`
//! inside builder chains of widgets whose init already disables focus (see
//! `focus_defaults`). `.can_focus(false)` elsewhere stays silent as a
//! deliberate opt-out, as does `.set_can_focus(false)` post-build whose
//! widget type never resolves. `.set_focusable(true)` stays silent since
//! `focusable` defaults to `FALSE` and really enables something.

use crate::{
    rules_ui::{
        builder_pattern::{
            file_scan_hits,
            redundant_false::redundant_false_at,
            trigger::{dotted_name_at, is_literal_close},
        },
        missing_labels::labels_trigger::cursor_after_gap,
    },
    scan::SourceFile,
};

/// Focus-enabling spellings that restate the default.
const REDUNDANT_SETTERS: [&str; 2] = ["can_focus", "set_can_focus"];

/// True if `name` restates the default when called with literal `true`.
///
/// # Arguments
///
/// * `name` - Method name after the dot.
///
/// # Returns
///
/// * `bool` - True for `can_focus` and `set_can_focus` only.
fn is_redundant_name(name: &str) -> bool {
    REDUNDANT_SETTERS.contains(&name)
}

/// True if the call at `open` passes only literal `true`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `open` - Byte index of the `(` opener.
///
/// # Returns
///
/// * `bool` - True for `(true)` with gaps allowed.
#[must_use]
pub fn is_true_close(bytes: &[u8], open: usize) -> bool {
    is_literal_close(bytes, open, b"true")
}

/// Method name at `dot`, if it restates the default with literal `true`.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `dot` - Byte index of the candidate `.`.
///
/// # Returns
///
/// * `Option<&str>` - Redundant setter name on matches.
fn redundant_call_at(body: &str, dot: usize) -> Option<&str> {
    let bytes = body.as_bytes();
    let (name, name_end) = dotted_name_at(body, dot)?;
    if !is_redundant_name(name) {
        return None;
    }
    let open = cursor_after_gap(bytes, name_end);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    is_true_close(bytes, open).then_some(name)
}

/// Finding for the redundant call opening at `idx`, if any.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `text` - Joined code text with strings blanked and tails cut.
/// * `idx` - Byte index under scan.
/// * `lineno` - One-based line number holding `idx`.
///
/// # Returns
///
/// * `Option<String>` - Hit line with a fix hint on redundant calls.
fn redundant_hit_at(file: &SourceFile, text: &str, idx: usize, lineno: usize) -> Option<String> {
    if let Some(name) = redundant_call_at(text, idx) {
        let line = file
            .lines
            .get(lineno.saturating_sub(1))
            .map_or("", String::as_str);
        return Some(format!(
            "{}:{lineno}:{line} help: remove redundant .{name}(true); it restates the default",
            file.path
        ));
    }
    redundant_false_at(file, text, idx, lineno)
}

/// Redundant focus-enablement findings for one file.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
fn file_redundant_hits(file: &SourceFile) -> Vec<String> {
    file_scan_hits(file, redundant_hit_at)
}

/// Redundant focus restatements across files.
///
/// Flags builder and post-build `true` spellings alike with no exemptions
/// beyond gtk-family gating, plus `.can_focus(false)` inside builder chains
/// of init-unfocusable widgets. Deliberate `.can_focus(false)` opt-outs,
/// variables, and `set_focusable` stay silent.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
pub fn redundant_focus_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(file_redundant_hits).collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::builder_pattern::redundant::redundant_focus_hits, scan::support::test_file,
    };

    #[test]
    fn builder_true_flags() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn f() {",
                "    let btn = Button::builder()",
                "        .css_classes([\"x\"])",
                "        .can_focus(true)",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        let hits = redundant_focus_hits(&files);
        assert_eq!(hits.len(), 1, "builder true flags once");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.contains("remove redundant")),
            "hint tells removal"
        );
    }

    #[test]
    fn setter_true_flags_gaps_count() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn f(btn: &Button) {",
                "    btn.set_can_focus( true );",
                "}",
            ],
        )];
        assert_eq!(
            redundant_focus_hits(&files).len(),
            1,
            "setter true with gaps flags"
        );
    }

    #[test]
    fn meaningful_spellings_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let btn = Button::builder()",
                    "        .can_focus(false)",
                    "        .build();",
                    "    btn",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Button;",
                    "fn f(btn: &Button, focus: bool) {",
                    "    btn.set_can_focus(focus);",
                    "    btn.set_focusable(true);",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let s = \".can_focus(true)\";",
                    "}",
                ],
            ),
        ];
        assert!(
            redundant_focus_hits(&files).is_empty(),
            "false, variables, focusable, and strings stay silent"
        );
    }

    #[test]
    fn false_on_unfocusable_init_flags() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::WindowControls;",
                "fn f() {",
                "    let controls = WindowControls::builder()",
                "        .side(Start)",
                "        .can_focus( false )",
                "        .build();",
                "    controls",
                "}",
            ],
        )];
        let hits = redundant_focus_hits(&files);
        assert_eq!(hits.len(), 1, "init-restated false flags once");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.contains("WindowControls")),
            "hint names the init-unfocusable widget"
        );
    }

    #[test]
    fn false_opt_out_stays_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f(btn: &Button) {",
                    "    btn.set_can_focus(false);",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Box;",
                    "fn f() {",
                    "    let box_ = Box::builder()",
                    "        .can_focus(false)",
                    "        .build();",
                    "    box_",
                    "}",
                ],
            ),
        ];
        assert!(
            redundant_focus_hits(&files).is_empty(),
            "unresolvable setters and deliberate opt-outs stay silent"
        );
    }

    #[test]
    fn foreign_files_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn f(widget: &Widget) {",
                "    widget.can_focus(true);",
                "}",
            ],
        )];
        assert!(
            redundant_focus_hits(&files).is_empty(),
            "files without gtk words stay silent"
        );
    }
}
