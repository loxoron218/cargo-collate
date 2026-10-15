//! Receiver resolution for setter default-class checks.
//!
//! Maps post-build `set_css_classes` receivers onto the widget built for the
//! same variable in the same `fn`, so setter lists check against their own
//! type. Unresolvable receivers fall back to a single distinct widget type
//! while multi-widget bodies stay silent instead of guessing.

use crate::rules_ui::{
    builder_pattern::{chain::chain_build_at, trigger::ident_start_before},
    missing_labels::{
        labels_trigger::{builder_type_at, cursor_after_gap, is_builder_open_at},
        widget_builders::is_widget_builder,
    },
};

/// Maximum bytes from `let` to its builder marker.
const LET_SCAN_LIMIT: usize = 2000;

/// Variable name before the dot at `dot`, if any.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `dot` - Byte index of the candidate dot.
///
/// # Returns
///
/// * `Option<String>` - Receiver spelling such as `btn`.
#[must_use]
pub fn setter_var_at(body: &str, dot: usize) -> Option<String> {
    let start = ident_start_before(body.as_bytes(), dot)?;
    Some(body.get(start..dot)?.to_owned())
}

/// Widget and chain segment for the builder at `pos`, if inventoried.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `pos` - Byte index of the `::builder` marker.
///
/// # Returns
///
/// * `Option<(String, String)>` - Widget basename with its chain segment.
fn builder_at(body: &str, pos: usize) -> Option<(String, String)> {
    if !is_builder_open_at(body.as_bytes(), pos) {
        return None;
    }
    let widget = builder_type_at(body, pos)?;
    if !is_widget_builder(widget) || widget == "Object" {
        return None;
    }
    let end = chain_build_at(body, pos)?;
    let segment = body.get(pos..end).map(str::to_owned)?;
    Some((widget.to_owned(), segment))
}

/// True if bounded `let` opens at `idx`.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `idx` - Candidate marker index.
///
/// # Returns
///
/// * `bool` - True on bounded matches.
fn is_let_at(body: &str, idx: usize) -> bool {
    let bytes = body.as_bytes();
    if body.get(idx..idx.saturating_add("let".len())) != Some("let") {
        return false;
    }
    if idx > 0
        && bytes
            .get(idx.saturating_sub(1))
            .is_some_and(|prev| prev.is_ascii_alphanumeric() || *prev == b'_')
    {
        return false;
    }
    bytes
        .get(idx.saturating_add("let".len()))
        .is_none_or(|next| !next.is_ascii_alphanumeric() && *next != b'_')
}

/// Cursor at the bound name after `let` at `idx`.
///
/// Skips an optional `mut` between `let` and the name.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `idx` - Start index of the `let` marker.
///
/// # Returns
///
/// * `usize` - Index of the bound name.
fn name_cursor_after_let(body: &str, idx: usize) -> usize {
    let bytes = body.as_bytes();
    let mut cursor = cursor_after_gap(bytes, idx.saturating_add("let".len()));
    if body.get(cursor..cursor.saturating_add("mut".len())) == Some("mut")
        && bytes
            .get(cursor.saturating_add("mut".len()))
            .is_none_or(|next| !next.is_ascii_alphanumeric() && *next != b'_')
    {
        cursor = cursor_after_gap(bytes, cursor.saturating_add("mut".len()));
    }
    cursor
}

/// True if `var` opens at `cursor` with a trailing boundary.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `cursor` - Candidate name index.
///
/// * `var` - Receiver spelling such as `btn`.
///
/// # Returns
///
/// * `bool` - True on bounded matches.
fn is_var_at(body: &str, cursor: usize, var: &str) -> bool {
    if body.get(cursor..cursor.saturating_add(var.len())) != Some(var) {
        return false;
    }
    body.as_bytes()
        .get(cursor.saturating_add(var.len()))
        .is_none_or(|next| !next.is_ascii_alphanumeric() && *next != b'_')
}

/// True if `let` at `idx` binds `var`.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `idx` - Candidate `let` index.
///
/// * `var` - Receiver spelling such as `btn`.
///
/// # Returns
///
/// * `bool` - True when this binding names the receiver.
fn binds_var(body: &str, idx: usize, var: &str) -> bool {
    if !is_let_at(body, idx) {
        return false;
    }
    is_var_at(body, name_cursor_after_let(body, idx), var)
}

/// Byte index of `let <var>` before `limit`, if any.
///
/// Keeps the last match so shadowing resolves to the nearest binding.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `var` - Receiver spelling such as `btn`.
///
/// * `limit` - Exclusive end index at the setter dot.
///
/// # Returns
///
/// * `Option<usize>` - Start index of the matching `let`.
fn let_pos_for(body: &str, var: &str, limit: usize) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut out = None;
    let mut idx: usize = 0;
    while idx.saturating_add("let".len()) <= bytes.len() && idx < limit {
        if binds_var(body, idx, var) {
            out = Some(idx);
        }
        idx = idx.saturating_add(1);
    }
    out
}

/// Widget backing `var` from its `let` binding, if any.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `var` - Receiver spelling such as `btn`.
///
/// * `setter` - Byte index of the setter dot.
///
/// # Returns
///
/// * `Option<(String, String)>` - Widget basename with its trigger segment.
fn let_widget(body: &str, var: &str, setter: usize) -> Option<(String, String)> {
    let let_pos = let_pos_for(body, var, setter)?;
    let bound = let_pos.saturating_add(LET_SCAN_LIMIT).min(setter);
    let mut idx = let_pos;
    while idx < bound {
        if let Some(found) = builder_at(body, idx) {
            return Some(found);
        }
        idx = idx.saturating_add(1);
    }
    None
}

/// Widget name for the builder at `idx`, if inventoried.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `idx` - Candidate `::builder` index.
///
/// # Returns
///
/// * `Option<String>` - Widget basename on inventoried matches.
fn widget_name_at(body: &str, idx: usize) -> Option<String> {
    if !is_builder_open_at(body.as_bytes(), idx) {
        return None;
    }
    let name = builder_type_at(body, idx)?;
    if !is_widget_builder(name) || name == "Object" {
        return None;
    }
    Some(name.to_owned())
}

/// Single distinct widget type in `body` with the whole body as segment.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// # Returns
///
/// * `Option<(String, String)>` - Widget basename with body triggers.
fn single_widget(body: &str) -> Option<(String, String)> {
    let bytes = body.as_bytes();
    let mut names = Vec::new();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        if let Some(name) = widget_name_at(body, idx) {
            names.push(name);
        }
        idx = idx.saturating_add(1);
    }
    let first = names.first()?;
    if names.iter().any(|name| name != first) {
        return None;
    }
    Some((first.clone(), body.to_owned()))
}

/// Widget and chain segment backing `var` before `setter`, if any.
///
/// Prefers the nearest `let <var>` binding whose builder closes before the
/// setter; falls back to a single distinct widget type in the body.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `var` - Receiver spelling such as `btn`.
///
/// * `setter` - Byte index of the setter dot.
///
/// # Returns
///
/// * `Option<(String, String)>` - Widget basename with its trigger segment.
#[must_use]
pub fn resolve_widget(body: &str, var: &str, setter: usize) -> Option<(String, String)> {
    if let Some(found) = let_widget(body, var, setter) {
        return Some(found);
    }
    single_widget(body)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::missing_css_classes::setter_resolve::resolve_widget, scan::support::test_file,
    };

    #[test]
    fn bindings_resolve() {
        let file = test_file(
            "src/a.rs",
            &[
                "fn f() {",
                "    let t = ToggleButton::builder().build();",
                "    t.set_css_classes(&[]);",
                "}",
            ],
        );
        let body = file.stripped.join("\n");
        let dot = body
            .find("t.set_css_classes")
            .unwrap_or(0)
            .saturating_add(1);
        assert!(
            resolve_widget(&body, "t", dot).is_some_and(|(widget, _)| widget == "ToggleButton"),
            "binding resolves"
        );
    }

    #[test]
    fn mixed_bodies_stay_silent() {
        let file = test_file(
            "src/a.rs",
            &[
                "fn f() {",
                "    let a = ToggleButton::builder().build();",
                "    let b = Button::builder().build();",
                "    x.set_css_classes(&[]);",
                "}",
            ],
        );
        let body = file.stripped.join("\n");
        assert!(
            resolve_widget(&body, "x", body.len()).is_none(),
            "mixed bodies stay silent"
        );
    }
}
