//! Builder and label matching for accessibility checks.
//!
//! Holds inventoried `Type::builder`, `.build(`, `update_property`, and
//! alias-resolved label matching so bodies share one spelling of the widget
//! rule.

use std::collections::BTreeSet;

use crate::{lexer::word::has_word, rules_ui::missing_labels::widget_builders::is_widget_builder};

/// True if `byte` continues an identifier.
///
/// # Arguments
///
/// * `byte` - Byte to classify.
///
/// # Returns
///
/// * `bool` - True for ASCII alphanumeric or underscore.
#[must_use]
pub const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True if `::builder` opens at `pos` without continuing an identifier.
///
/// Rejects `::builders` module paths alongside longer spellings.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `pos` - Candidate marker index.
///
/// # Returns
///
/// * `bool` - True on a bounded marker match.
#[must_use]
pub fn is_builder_open_at(bytes: &[u8], pos: usize) -> bool {
    bytes
        .get(pos..pos.saturating_add("::builder".len()))
        .is_some_and(|window| window == b"::builder")
        && bytes
            .get(pos.saturating_add("::builder".len()))
            .is_none_or(|next| !is_ident_byte(*next))
}

/// Type basename immediately before `pos`, if any.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `pos` - Byte index of the `::builder` marker.
///
/// # Returns
///
/// * `Option<&str>` - Basename such as `Button` in `gtk::Button::builder`.
#[must_use]
pub fn builder_type_at(body: &str, pos: usize) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut start = pos;
    while start > 0
        && bytes
            .get(start.saturating_sub(1))
            .is_some_and(|prev| is_ident_byte(*prev))
    {
        start = start.saturating_sub(1);
    }
    if start == pos {
        return None;
    }
    body.get(start..pos)
}

/// True if `body` builds an inventoried widget with a builder.
///
/// Requires a `Type::builder` marker whose basename sits in the widget
/// inventory (or the generic `Object` fallback) plus a `.build(` call, so
/// `Signal`, `Settings`, HTTP clients, and bare words never trigger.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// # Returns
///
/// * `bool` - True when a widget builder marker appears.
#[must_use]
pub fn has_builder_trigger(body: &str) -> bool {
    if !body.contains(".build(") {
        return false;
    }
    let bytes = body.as_bytes();
    body.match_indices("::builder").any(|(pos, _)| {
        is_builder_open_at(bytes, pos) && builder_type_at(body, pos).is_some_and(is_widget_builder)
    })
}

/// True if `name` opens at `idx` with a boundary before it.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `idx` - Candidate name index.
/// * `name` - Accepted label spelling.
///
/// # Returns
///
/// * `bool` - True on a bounded name match.
fn is_label_name_at(bytes: &[u8], idx: usize, name: &[u8]) -> bool {
    bytes
        .get(idx..idx.saturating_add(name.len()))
        .is_some_and(|window| window == name)
        && (idx == 0
            || bytes
                .get(idx.saturating_sub(1))
                .is_none_or(|prev| !is_ident_byte(*prev)))
}

/// Cursor past ASCII gaps starting at `start`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `start` - Index past the label spelling.
///
/// # Returns
///
/// * `usize` - Resume index at the first non-gap byte.
#[must_use]
pub fn cursor_after_gap(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start;
    while bytes
        .get(cursor)
        .is_some_and(|next| *next == b' ' || *next == b'\t' || *next == b'\n' || *next == b'\r')
    {
        cursor = cursor.saturating_add(1);
    }
    cursor
}

/// True if an opener sits at `cursor`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `cursor` - Index past optional gaps.
///
/// # Returns
///
/// * `bool` - True when `(` opens there.
fn is_opener_at(bytes: &[u8], cursor: usize) -> bool {
    bytes.get(cursor).is_some_and(|next| *next == b'(')
}

/// True if `name` calls with identifier boundaries in `body`.
///
/// Skips whitespace between the name and the opener so `Label (` counts
/// while `Labels(` and `Labelled` stay silent.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `name` - Accepted label spelling.
///
/// # Returns
///
/// * `bool` - True when a bounded `name(` call appears.
fn has_name_call(body: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = body.as_bytes();
    let needle = name.as_bytes();
    let mut idx: usize = 0;
    while idx.saturating_add(needle.len()) <= bytes.len() {
        let cursor = cursor_after_gap(bytes, idx.saturating_add(needle.len()));
        if is_label_name_at(bytes, idx, needle) && is_opener_at(bytes, cursor) {
            return true;
        }
        idx = idx.saturating_add(1);
    }
    false
}

/// True if `body` calls any accepted label name.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `aliases` - In-scope label names from imports plus the fallback.
///
/// # Returns
///
/// * `bool` - True when a bounded alias call appears.
#[must_use]
pub fn has_label_call(body: &str, aliases: &BTreeSet<String>) -> bool {
    aliases.iter().any(|name| has_name_call(body, name))
}

/// True if `body` sets an accessibility label.
///
/// Requires `update_property` as a whole word plus a bounded alias call in
/// the same `fn` body.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `aliases` - In-scope label names from imports plus the fallback.
///
/// # Returns
///
/// * `bool` - True when both label markers appear.
#[must_use]
pub fn has_accessible_label(body: &str, aliases: &BTreeSet<String>) -> bool {
    has_word(body, "update_property") && has_label_call(body, aliases)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::rules_ui::missing_labels::labels_trigger::{
        has_accessible_label, has_builder_trigger, has_label_call,
    };

    fn names(of: &[&str]) -> BTreeSet<String> {
        of.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn triggers_need_punctuation() {
        assert!(
            has_builder_trigger("Button::builder().build("),
            "full marks flag"
        );
        assert!(
            has_builder_trigger("let x = Object::builder().build(;"),
            "generic fallback flags"
        );
        assert!(
            !has_builder_trigger("let builder = 1;"),
            "bare words stay silent"
        );
        assert!(
            !has_builder_trigger("x.build_something("),
            "prefix stays silent"
        );
    }

    #[test]
    fn non_widget_builders_stay_silent() {
        for body in [
            "Signal::builder(\"x\").build(",
            "Client::builder().build(",
            "Settings::builder().build(",
            "glib::Object::builder_property().build(",
        ] {
            assert!(!has_builder_trigger(body), "{body} stays silent");
        }
        assert!(
            !has_builder_trigger("let x = gtk::builders::ButtonBuilder::new(); x.build()"),
            "module paths stay silent"
        );
    }

    #[test]
    fn labels_need_bounded_openers() {
        let aliases = names(&["Label"]);
        assert!(has_label_call("Label(\"\")", &aliases), "call flags");
        assert!(has_label_call("Label (\"\")", &aliases), "gap counts");
        assert!(
            !has_label_call("Labels(\"\")", &aliases),
            "plural stays silent"
        );
        assert!(!has_label_call("Labelled", &aliases), "word stays silent");
    }

    #[test]
    fn aliases_satisfy_like_bare_names() {
        let aliases = names(&["Label", "PropertyLabel", "A11yLabel"]);
        for body in [
            "update_property(&[Label(\"\")])",
            "update_property(&[PropertyLabel(\"\")])",
            "update_property(&[A11yLabel(\"\")])",
        ] {
            assert!(has_accessible_label(body, &aliases), "{body} satisfies");
        }
        assert!(
            !has_accessible_label("update_property(&[Labels(\"\")])", &aliases),
            "near miss stays silent"
        );
    }

    #[test]
    fn accessible_needs_both_markers() {
        let aliases = names(&["Label"]);
        assert!(
            has_accessible_label("update_property(&[Label(\"\")])", &aliases),
            "pair flags"
        );
        assert!(
            !has_accessible_label("update_property(&[x])", &aliases),
            "missing label stays silent"
        );
        assert!(
            !has_accessible_label("Label(\"\")", &aliases),
            "missing call stays silent"
        );
        assert!(
            !has_accessible_label("update_property(&[A11yLabel(\"\")])", &aliases),
            "unknown alias stays silent"
        );
    }
}
