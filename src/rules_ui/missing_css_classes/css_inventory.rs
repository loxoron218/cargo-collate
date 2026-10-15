//! Default CSS-class inventory for widget builders.
//!
//! Lists every `gtk4` and `libadwaita` widget basename whose instance carries
//! an auto-added style class on its own node, so builder `css_classes` lists
//! keep required defaults instead of wiping them. Content-driven entries only
//! apply when the chain sets their trigger property while state transients
//! and opt-in theme classes stay out by construction.
//!
//! Sources: `gtk/gtk*.c` `gtk_widget_add_css_class` self calls on `main`,
//! `libadwaita/src/adw-*.c` self calls on `main`,
//! `docs.gtk.org/gtk4/class.*.html` CSS nodes, `gtk-rs/gtk4-rs#1110`.

use crate::rules_ui::{missing_css_classes::css_parse::method_open_at, ui_ext::is_ident_byte};

/// Widget with its always-present self class.
const ALWAYS_SINGLE: [(&str, &str); 13] = [
    ("ToggleButton", "toggle"),
    ("LinkButton", "link"),
    ("MenuButton", "popup"),
    ("GridView", "view"),
    ("ListView", "view"),
    ("TextView", "view"),
    ("Window", "background"),
    ("PopoverMenu", "menu"),
    ("StackSwitcher", "linked"),
    ("Spinner", "spinner"),
    ("ShortcutRow", "shortcut-row"),
    ("PasswordEntryRow", "password"),
    ("LinkRow", "link"),
];

/// True if `classes` holds `want` exactly.
///
/// Shared with requirement merging so both layers read one spelling of class presence.
///
/// # Arguments
///
/// * `classes` - Parsed class names from the builder or setter list.
///
/// * `want` - Required class spelling.
///
/// # Returns
///
/// * `bool` - True when the required spelling is present.
#[must_use]
pub fn contains_class(classes: &[String], want: &str) -> bool {
    classes.iter().any(|have| have == want)
}

/// True if `word` opens at `pos` with identifier boundaries.
///
/// # Arguments
///
/// * `segment` - Builder chain text with strings blanked and tails cut.
///
/// * `pos` - Candidate marker index.
///
/// * `word` - Expected spelling such as `false`.
///
/// # Returns
///
/// * `bool` - True on a bounded `word` match.
fn is_word_at(segment: &str, pos: usize, word: &str) -> bool {
    let bytes = segment.as_bytes();
    if segment.get(pos..pos.saturating_add(word.len())) != Some(word) {
        return false;
    }
    if bytes
        .get(pos.saturating_sub(1))
        .is_some_and(|prev| is_ident_byte(*prev))
    {
        return false;
    }
    bytes
        .get(pos.saturating_add(word.len()))
        .is_none_or(|next| !is_ident_byte(*next))
}

/// Advance paren `depth` past the byte at `cursor`.
///
/// Shared with setter scans so both directions step parens on one spelling.
///
/// # Arguments
///
/// * `byte` - Byte under scan, if any.
///
/// * `depth` - Mutable paren depth.
pub const fn step_paren(byte: Option<u8>, depth: &mut i32) {
    match byte {
        Some(b'(') => *depth = depth.saturating_add(1),
        Some(b')') => *depth = depth.saturating_sub(1),
        Some(_) | None => {}
    }
}

/// True if the call at `open` holds a bounded `word` before it closes.
///
/// # Arguments
///
/// * `segment` - Builder chain text with strings blanked and tails cut.
///
/// * `open` - Byte index of the argument opener.
///
/// * `word` - Expected spelling such as `false`.
///
/// # Returns
///
/// * `bool` - True when `word` appears before the matching closer.
fn has_word_between(segment: &str, open: usize, word: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut depth: i32 = 1;
    let mut cursor = open.saturating_add(1);
    let mut seen = false;
    while cursor < bytes.len() && depth > 0 {
        if is_word_at(segment, cursor, word) {
            seen = true;
        }
        step_paren(bytes.get(cursor).copied(), &mut depth);
        cursor = cursor.saturating_add(1);
    }
    seen && depth <= 0
}

/// True if `segment` calls `.method` with a bounded `word` argument.
///
/// Shared with requirement merging so frameless and separator triggers read
/// one boolean-argument spelling.
///
/// # Arguments
///
/// * `segment` - Builder chain text with strings blanked and tails cut.
///
/// * `method` - Builder method such as `has_frame`.
///
/// * `word` - Expected argument spelling such as `false`.
///
/// # Returns
///
/// * `bool` - True for an explicit boolean opt-in.
#[must_use]
pub fn method_bool_arg(segment: &str, method: &str, word: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        if let Some(open) = method_open_at(segment, idx, method)
            && has_word_between(segment, open, word)
        {
            return true;
        }
        idx = idx.saturating_add(1);
    }
    false
}

/// True if `segment` calls `.has_frame` with a `false` argument.
///
/// # Arguments
///
/// * `segment` - Builder chain text with strings blanked and tails cut.
///
/// # Returns
///
/// * `bool` - True for an explicit frameless opt-out.
fn has_frame_false(segment: &str) -> bool {
    method_bool_arg(segment, "has_frame", "false")
}

/// Always-required classes missing from `classes` for `widget`.
///
/// `ViewSwitcher` needs `narrow` or `wide` while `ScrolledWindow` needs
/// `frame` unless the chain opts out with `has_frame(false)`.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Builder chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the `css_classes` list.
///
/// # Returns
///
/// * `Vec<String>` - Missing required spellings in inventory order.
#[must_use]
pub fn missing_always(widget: &str, segment: &str, classes: &[String]) -> Vec<String> {
    if widget == "ViewSwitcher" {
        if contains_class(classes, "narrow") || contains_class(classes, "wide") {
            return Vec::new();
        }
        return vec!["narrow or wide".to_owned()];
    }
    if widget == "ScrolledWindow" {
        if has_frame_false(segment) || contains_class(classes, "frame") {
            return Vec::new();
        }
        return vec!["frame".to_owned()];
    }
    for (name, required) in ALWAYS_SINGLE {
        if widget == name && !contains_class(classes, required) {
            return vec![required.to_owned()];
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_css_classes::{
        class_require::collect_missing, css_inventory::missing_always,
    };

    #[test]
    fn always_splits_present() {
        let have = vec!["toggle".to_owned()];
        assert!(
            missing_always("ToggleButton", "", &have).is_empty(),
            "present stays silent"
        );
        assert_eq!(
            missing_always("ToggleButton", "", &[]).len(),
            1,
            "absent flags once"
        );
        assert!(
            missing_always("Box", "", &[]).is_empty(),
            "layout stays silent"
        );
    }

    #[test]
    fn switcher_needs_either() {
        for have in [vec!["narrow".to_owned()], vec!["wide".to_owned()]] {
            assert!(
                missing_always("ViewSwitcher", "", &have).is_empty(),
                "either satisfies"
            );
        }
        assert_eq!(
            missing_always("ViewSwitcher", "", &[]).len(),
            1,
            "neither flags once"
        );
    }

    #[test]
    fn scrolled_frame_opts_out() {
        assert!(
            missing_always("ScrolledWindow", ".has_frame(false)", &[]).is_empty(),
            "opt-out stays silent"
        );
        assert_eq!(
            missing_always("ScrolledWindow", "", &[]).len(),
            1,
            "default flags"
        );
    }

    #[test]
    fn conditional_needs_trigger() {
        let have = vec!["text-button".to_owned()];
        assert!(
            collect_missing("Button", ".label(\"\")", &have).is_empty(),
            "present stays silent"
        );
        assert_eq!(
            collect_missing("Button", ".label(\"\")", &[]).len(),
            1,
            "triggered absent flags"
        );
        assert!(
            collect_missing("Button", "", &[]).is_empty(),
            "untriggered stays silent"
        );
    }
}
