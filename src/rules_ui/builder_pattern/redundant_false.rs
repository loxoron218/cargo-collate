//! Restated init-default detectors for focus opt-outs.
//!
//! Flags `.can_focus(false)` inside builder chains of widgets whose init
//! already disables focus (see `focus_defaults`), so restated opt-outs go
//! while deliberate ones elsewhere stay silent. Post-build
//! `.set_can_focus(false)` never resolves a widget type and stays out of
//! scope.

use crate::{
    rules_ui::{
        builder_pattern::{
            focus_defaults::is_unfocusable_init,
            trigger::{builder_chain_at, dotted_name_at, is_literal_close},
        },
        missing_labels::labels_trigger::cursor_after_gap,
    },
    scan::SourceFile,
};

/// True if `.can_focus(false)` opens at `idx`.
///
/// # Arguments
///
/// * `segment` - Builder chain text between `::builder` and `.build(`.
/// * `bytes` - Segment bytes under scan.
/// * `idx` - Candidate dot index.
///
/// # Returns
///
/// * `bool` - True on a bounded opt-out call.
fn is_can_focus_false_at(segment: &str, bytes: &[u8], idx: usize) -> bool {
    let Some((name, name_end)) = dotted_name_at(segment, idx) else {
        return false;
    };
    if name != "can_focus" {
        return false;
    }
    let open = cursor_after_gap(bytes, name_end);
    bytes.get(open) == Some(&b'(') && is_literal_close(bytes, open, b"false")
}

/// True if `segment` disables focus through the builder spelling.
///
/// Only the `.can_focus(` builder method counts; `.set_can_focus(` never
/// appears in builder chains.
///
/// # Arguments
///
/// * `segment` - Builder chain text between `::builder` and `.build(`.
///
/// # Returns
///
/// * `bool` - True for a bounded `.can_focus(false)` call.
fn has_can_focus_false(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if is_can_focus_false_at(segment, bytes, idx) {
            return true;
        }
        idx = idx.saturating_add(1);
    }
    false
}

/// Finding for a restated init default in the chain at `idx`, if any.
///
/// Flags `.can_focus(false)` only when the chained widget type already
/// disables focus in its init; deliberate opt-outs elsewhere stay silent.
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
/// * `Option<String>` - Hit line with a fix hint on restated defaults.
pub fn redundant_false_at(
    file: &SourceFile,
    text: &str,
    idx: usize,
    lineno: usize,
) -> Option<String> {
    let (widget, end) = builder_chain_at(text, idx)?;
    if !is_unfocusable_init(widget) {
        return None;
    }
    let segment = text.get(idx..end)?;
    if !has_can_focus_false(segment) {
        return None;
    }
    let line = file
        .lines
        .get(lineno.saturating_sub(1))
        .map_or("", String::as_str);
    Some(format!(
        "{}:{lineno}:{line} help: remove redundant .can_focus(false); {widget} init already \
         disables focus",
        file.path
    ))
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::builder_pattern::redundant_false::redundant_false_at,
        scan::{SourceFile, builders::test_file},
    };

    fn hit_at(file: &SourceFile, idx: usize) -> Option<String> {
        redundant_false_at(file, &file.lines.join("\n"), idx, 1)
    }

    #[test]
    fn init_restatement_flags() {
        let file = test_file(
            "src/a.rs",
            &[
                "use gtk::WindowControls;",
                "fn f() {",
                "    let c = WindowControls::builder().can_focus(false).build();",
                "    c",
                "}",
            ],
        );
        let text = file.lines.join("\n");
        let idx = text.find("::builder").unwrap_or(0);
        assert!(
            hit_at(&file, idx).is_some_and(|hit| hit.contains("WindowControls")),
            "init-restated false flags"
        );
    }

    #[test]
    fn deliberate_opt_out_silent() {
        let file = test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn f() {",
                "    let btn = Button::builder().can_focus(false).build();",
                "    btn",
                "}",
            ],
        );
        let text = file.lines.join("\n");
        let idx = text.find("::builder").unwrap_or(0);
        assert!(
            hit_at(&file, idx).is_none(),
            "deliberate opt-out stays silent"
        );
        assert!(
            hit_at(&file, usize::MAX).is_none(),
            "out-of-range stays silent"
        );
    }
}
