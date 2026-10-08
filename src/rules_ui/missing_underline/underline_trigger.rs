//! Builder and underline matching for mnemonic checks.
//!
//! Holds mnemonic-capable `Type::builder` detection plus `.use_underline`
//! and `.set_use_underline` enablement matching, so bodies share one
//! spelling of the underline rule.

use crate::{
    lexer::word::boundary_after,
    rules_ui::{
        missing_labels::labels_trigger::cursor_after_gap,
        missing_underline::mnemonic_widgets::{is_mnemonic_widget, mnemonic_widgets},
        ui_ext::is_ident_byte,
    },
};

/// True if `body` builds a mnemonic-capable widget with a builder.
///
/// Scans widget-first: every mnemonic basename looks for a trailing
/// `::builder` with a trailing boundary plus a `.build(` call, so
/// non-mnemonic widgets never trigger while fully qualified paths still
/// count through their trailing basename.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// # Returns
///
/// * `bool` - True when a mnemonic widget builder marker appears.
#[must_use]
pub fn has_mnemonic_builder(body: &str) -> bool {
    if !body.contains(".build(") {
        return false;
    }
    for widget in mnemonic_widgets() {
        if widget_builder_present(body, widget) {
            return true;
        }
    }
    false
}

/// True if `widget::builder` opens in `body` with a trailing boundary.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `widget` - Mnemonic basename such as `Button`.
///
/// # Returns
///
/// * `bool` - True when the widget builder marker appears.
fn widget_builder_present(body: &str, widget: &str) -> bool {
    if !is_mnemonic_widget(widget) {
        return false;
    }
    let mut from = 0;
    while let Some(tail) = body.get(from..) {
        let Some(rel) = tail.find(widget) else {
            return false;
        };
        let pos = from.saturating_add(rel);
        if widget_has_builder(body, pos, widget.len()) {
            return true;
        }
        from = pos.saturating_add(1);
    }
    false
}

/// True if `::builder` follows the widget at `pos` with a boundary.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `pos` - Byte index of the widget basename.
/// * `len` - Byte length of the widget basename.
///
/// # Returns
///
/// * `bool` - True when the builder marker opens with a boundary.
fn widget_has_builder(body: &str, pos: usize, len: usize) -> bool {
    let after = pos.saturating_add(len);
    if body.get(after..after.saturating_add("::builder".len())) != Some("::builder") {
        return false;
    }
    boundary_after(body, after.saturating_add("::builder".len()))
}

/// True if `.name(true)` enables mnemonics at `pos`, if any.
///
/// Requires a leading dot, a bounded name, and a `(true)` argument list with
/// gaps allowed around the opener and closer.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `pos` - Byte index of the underline name.
/// * `len` - Byte length of the underline name.
///
/// # Returns
///
/// * `bool` - True when the call enables mnemonics here.
fn underline_on_at(body: &str, pos: usize, len: usize) -> bool {
    let bytes = body.as_bytes();
    if pos == 0 || bytes.get(pos.saturating_sub(1)) != Some(&b'.') {
        return false;
    }
    if bytes
        .get(pos.saturating_add(len))
        .is_some_and(|next| is_ident_byte(*next))
    {
        return false;
    }
    parens_true(body, pos.saturating_add(len))
}

/// True if `(true)` opens at `after` past gaps, if any.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `after` - Byte index past the method name.
///
/// # Returns
///
/// * `bool` - True when the argument list passes `true`.
fn parens_true(body: &str, after: usize) -> bool {
    let bytes = body.as_bytes();
    let open = cursor_after_gap(bytes, after);
    if bytes.get(open) != Some(&b'(') {
        return false;
    }
    let arg = cursor_after_gap(bytes, open.saturating_add(1));
    let end = arg.saturating_add("true".len());
    if bytes.get(arg..end) != Some(b"true") {
        return false;
    }
    if bytes.get(end).is_some_and(|next| is_ident_byte(*next)) {
        return false;
    }
    let close = cursor_after_gap(bytes, end);
    bytes.get(close).is_some_and(|next| *next == b')')
}

/// True if `.name(true)` appears in `body` for underline `name`.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
/// * `name` - Underline spelling such as `use_underline`.
///
/// # Returns
///
/// * `bool` - True when a bounded enabling call appears.
fn underline_named(body: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(tail) = body.get(from..) {
        let Some(rel) = tail.find(name) else {
            return false;
        };
        let pos = from.saturating_add(rel);
        if underline_on_at(body, pos, name.len()) {
            return true;
        }
        from = pos.saturating_add(1);
    }
    false
}

/// True if `body` enables mnemonics through either underline spelling.
///
/// Accepts `.use_underline(true)` in builder chains and
/// `.set_use_underline(true)` setter calls, so both programmatic spellings
/// satisfy the rule while `false` spellings stay flagged.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// # Returns
///
/// * `bool` - True when either underline spelling passes `true`.
#[must_use]
pub fn has_underline(body: &str) -> bool {
    underline_named(body, "use_underline") || underline_named(body, "set_use_underline")
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_underline::underline_trigger::{
        has_mnemonic_builder, has_underline,
    };

    #[test]
    fn mnemonic_builders_split_layout() {
        assert!(
            has_mnemonic_builder("Button::builder().build("),
            "mnemonic builder flags"
        );
        assert!(
            has_mnemonic_builder("let x = gtk::Button::builder().build(;"),
            "qualified builder flags"
        );
        assert!(
            !has_mnemonic_builder("Box::builder().build("),
            "layout builder stays silent"
        );
        assert!(
            !has_mnemonic_builder("Signal::builder().build("),
            "non-widget builder stays silent"
        );
        assert!(
            !has_mnemonic_builder("let builder = 1;"),
            "bare words stay silent"
        );
    }

    #[test]
    fn underline_needs_true() {
        assert!(
            has_underline(".use_underline(true)"),
            "builder spelling satisfies"
        );
        assert!(
            has_underline(".set_use_underline(true)"),
            "setter spelling satisfies"
        );
        assert!(has_underline(".use_underline ( true )"), "gaps satisfy");
        assert!(
            !has_underline(".use_underline(false)"),
            "false stays flagged"
        );
        assert!(
            !has_underline(".set_use_underline(false)"),
            "setter false stays flagged"
        );
        assert!(!has_underline(".build("), "bare chain stays flagged");
    }
}
