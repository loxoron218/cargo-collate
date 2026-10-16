//! Builder-call matching for widget styling checks.
//!
//! Holds the `.build(` terminator, inventoried widget basenames, `.name(`
//! property matching, identifier runs, constructor families, and the gtk box
//! shape shared by chain and direct scans, so both directions of the builder
//! rule read one spelling of call boundaries. Byte markers stay generic
//! while widget knowledge resolves through the inventory.

use crate::rules_ui::{
    missing_labels::{
        labels_trigger::{builder_type_at, cursor_after_gap, is_builder_open_at},
        widget_builders::WIDGET_BUILDERS,
    },
    ui_ext::is_ident_byte,
};

/// Maximum bytes from `::builder` to its `.build(` terminator.
///
/// Chains separate on `;` or a new `::builder` well inside this window, so a
/// longer gap means the builder never terminates rather than a missed chain.
pub const CHAIN_SCAN_LIMIT: usize = 4000;

/// True if `marker` matches at `pos`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `pos` - Candidate marker index.
/// * `marker` - Expected bytes such as `b".build"`.
///
/// # Returns
///
/// * `bool` - True on a full marker match.
fn marker_at(bytes: &[u8], pos: usize, marker: &[u8]) -> bool {
    let mut offset = 0;
    while offset < marker.len() {
        if bytes.get(pos.saturating_add(offset)) != marker.get(offset) {
            return false;
        }
        offset = offset.saturating_add(1);
    }
    true
}

/// True if `.build(` opens at `pos` with gaps allowed before the opener.
///
/// Rejects `.builders` and longer spellings through the trailing boundary.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `pos` - Candidate dot index.
///
/// # Returns
///
/// * `bool` - True on a bounded terminator match.
#[must_use]
pub fn is_build_open_at(bytes: &[u8], pos: usize) -> bool {
    if !marker_at(bytes, pos, b".build") {
        return false;
    }
    if bytes
        .get(pos.saturating_add(".build".len()))
        .is_some_and(|next| is_ident_byte(*next))
    {
        return false;
    }
    let open = cursor_after_gap(bytes, pos.saturating_add(".build".len()));
    bytes.get(open).is_some_and(|next| *next == b'(')
}

/// Widget basename immediately before `pos`, if inventoried.
///
/// The generic `Object` fallback stays out since its typestate API sets
/// properties through `.property(` instead of direct builder methods.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `pos` - Byte index of the `::builder` marker.
///
/// # Returns
///
/// * `Option<&str>` - Basename such as `Button`, when it names a widget.
#[must_use]
pub fn builder_widget_at(body: &str, pos: usize) -> Option<&str> {
    let name = builder_type_at(body, pos)?;
    WIDGET_BUILDERS.contains(&name).then_some(name)
}

/// True if `.name(` calls with gaps allowed before the opener.
///
/// Requires a trailing identifier boundary, so longer spellings and bare
/// mentions stay silent.
///
/// # Arguments
///
/// * `segment` - Builder chain text between `::builder` and `.build(`.
/// * `name` - Builder property such as `css_classes`.
///
/// # Returns
///
/// * `bool` - True when a bounded `name(` call appears.
#[must_use]
pub fn has_method_call(segment: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = segment.as_bytes();
    let needle = name.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes.get(idx) == Some(&b'.')
            && marker_at(bytes, idx.saturating_add(1), needle)
            && bytes
                .get(idx.saturating_add(1).saturating_add(needle.len()))
                .is_none_or(|next| !is_ident_byte(*next))
            && bytes
                .get(cursor_after_gap(
                    bytes,
                    idx.saturating_add(1).saturating_add(needle.len()),
                ))
                .is_some_and(|next| *next == b'(')
        {
            return true;
        }
        idx = idx.saturating_add(1);
    }
    false
}

/// True if the call at `open` passes only the literal `word`.
///
/// Requires the literal with identifier boundaries on both sides followed by
/// the closer, so variables, longer identifiers, and multi-argument calls
/// stay silent.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `open` - Byte index of the `(` opener.
/// * `word` - Expected literal such as `b"true"`.
///
/// # Returns
///
/// * `bool` - True for `(word)` with gaps allowed.
#[must_use]
pub fn is_literal_close(bytes: &[u8], open: usize, word: &[u8]) -> bool {
    let start = cursor_after_gap(bytes, open.saturating_add(1));
    if bytes
        .get(start..start.saturating_add(word.len()))
        .is_none_or(|window| window != word)
    {
        return false;
    }
    let after = start.saturating_add(word.len());
    if bytes.get(after).is_some_and(|next| is_ident_byte(*next)) {
        return false;
    }
    let close = cursor_after_gap(bytes, after);
    bytes.get(close).is_some_and(|next| *next == b')')
}

/// Method name after the dot at `dot`, if any.
///
/// Shares one spelling of dotted-call parsing between setter scans, so
/// direct and redundant detectors never clone each other.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `dot` - Byte index of the candidate `.`.
///
/// # Returns
///
/// * `Option<(&str, usize)>` - Method name and exclusive end index.
#[must_use]
pub fn dotted_name_at(body: &str, dot: usize) -> Option<(&str, usize)> {
    let bytes = body.as_bytes();
    if bytes.get(dot) != Some(&b'.') {
        return None;
    }
    let name_start = dot.saturating_add(1);
    let name_end = ident_end_after(bytes, name_start)?;
    let name = body.get(name_start..name_end)?;
    Some((name, name_end))
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
#[must_use]
pub fn chain_build_at(body: &str, from: usize) -> Option<usize> {
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

/// Widget basename and terminator end for the builder chain at `from`, if any.
///
/// Shares one spelling of chain segmentation between chain and redundant
/// scans, so stored intermediates never pair across statements twice.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `from` - Byte index of the opening `::builder` marker.
///
/// # Returns
///
/// * `Option<(&str, usize)>` - Widget basename and terminator end on chains.
#[must_use]
pub fn builder_chain_at(body: &str, from: usize) -> Option<(&str, usize)> {
    if !is_builder_open_at(body.as_bytes(), from) {
        return None;
    }
    let widget = builder_widget_at(body, from)?;
    let end = chain_build_at(body, from)?;
    Some((widget, end))
}

/// Start index of the identifier ending at `end`, if any.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `end` - Exclusive end index past the identifier.
///
/// # Returns
///
/// * `Option<usize>` - Start index of the identifier run.
#[must_use]
pub fn ident_start_before(bytes: &[u8], end: usize) -> Option<usize> {
    if end == 0 {
        return None;
    }
    if bytes
        .get(end.saturating_sub(1))
        .is_none_or(|prev| !is_ident_byte(*prev))
    {
        return None;
    }
    let mut start = end.saturating_sub(1);
    while start > 0
        && bytes
            .get(start.saturating_sub(1))
            .is_some_and(|prev| is_ident_byte(*prev))
    {
        start = start.saturating_sub(1);
    }
    Some(start)
}

/// End index past the identifier starting at `start`, if any.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `start` - Index of the first identifier byte.
///
/// # Returns
///
/// * `Option<usize>` - Exclusive end index past the identifier run.
#[must_use]
pub fn ident_end_after(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start).is_none_or(|first| !is_ident_byte(*first)) {
        return None;
    }
    let mut end = start.saturating_add(1);
    while bytes.get(end).is_some_and(|next| is_ident_byte(*next)) {
        end = end.saturating_add(1);
    }
    Some(end)
}

/// Constructor families flagging direct widget construction.
///
/// Covers every gtk constructor spelling while class-level setters such as
/// `set_default_icon_name` stay silent.
///
/// # Arguments
///
/// * `assoc` - Associated name after `Widget::`.
///
/// # Returns
///
/// * `bool` - True for constructor-family names.
#[must_use]
pub fn is_ctor_name(assoc: &str) -> bool {
    assoc == "new"
        || assoc.starts_with("new_")
        || assoc == "default"
        || assoc.starts_with("with_")
        || assoc == "from"
        || assoc.starts_with("from_")
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::builder_pattern::trigger::{
        has_method_call, is_build_open_at, is_ctor_name,
    };

    #[test]
    fn terminators_need_boundaries() {
        assert!(is_build_open_at(b".build(", 0), "plain terminator flags");
        assert!(
            is_build_open_at(b".build (", 0),
            "gaps before the opener flag"
        );
        assert!(
            !is_build_open_at(b".builders(", 0),
            "module paths stay silent"
        );
        assert!(
            !is_build_open_at(b".rebuild(", 1),
            "longer verbs stay silent"
        );
    }

    #[test]
    fn methods_need_calls() {
        assert!(
            has_method_call(".css_classes([\"x\"])", "css_classes"),
            "call flags"
        );
        assert!(
            !has_method_call(".css_classes2([\"x\"])", "css_classes"),
            "longer names stay silent"
        );
        assert!(
            !has_method_call("css_classes", "css_classes"),
            "bare mentions stay silent"
        );
        assert!(!has_method_call(".build(", ""), "empty names stay silent");
    }

    #[test]
    fn ctor_families_split_setters() {
        for name in [
            "new",
            "new_with_mnemonic",
            "default",
            "with_label",
            "with_range",
            "from",
            "from_icon_name",
            "from_appdata",
        ] {
            assert!(is_ctor_name(name), "{name} counts");
        }
        for name in [
            "builder",
            "build",
            "set_default_icon_name",
            "emit",
            "Role",
            "",
        ] {
            assert!(!is_ctor_name(name), "{name} stays silent");
        }
    }
}
