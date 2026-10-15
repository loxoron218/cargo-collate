//! Post-build setter matching for widget styling checks.
//!
//! Holds the setter-to-property table with call-boundary matching, so direct
//! construction scans read one spelling of setter calls. Literal
//! `set_can_focus(true)` stays silent since the redundant-focus check owns
//! restatements of the default.

use crate::rules_ui::{
    builder_pattern::{redundant::is_true_close, trigger::dotted_name_at},
    missing_labels::labels_trigger::cursor_after_gap,
};

/// Post-build setters with the builder property they belong to.
const SETTER_PROPS: [(&str, &str); 4] = [
    ("set_css_classes", "css_classes"),
    ("set_tooltip_text", "tooltip_text"),
    ("set_can_focus", "can_focus"),
    ("set_focusable", "can_focus"),
];

/// Setter name, its builder property, and the opener at `dot`, if any.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `dot` - Byte index of the candidate `.`.
///
/// # Returns
///
/// * `Option<(&str, &str, usize)>` - Setter name, builder property, and opener.
#[must_use]
pub fn setter_call_at(body: &str, dot: usize) -> Option<(&str, &str, usize)> {
    let (name, name_end) = dotted_name_at(body, dot)?;
    let prop = SETTER_PROPS
        .iter()
        .find(|(setter, _)| *setter == name)
        .map(|(_, prop)| *prop)?;
    let bytes = body.as_bytes();
    let open = cursor_after_gap(bytes, name_end);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    if name == "set_can_focus" && is_true_close(bytes, open) {
        return None;
    }
    Some((name, prop, open))
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::builder_pattern::setters::setter_call_at;

    #[test]
    fn setters_need_openers() {
        assert!(
            setter_call_at("btn.set_css_classes(&[])", 3)
                .is_some_and(|(name, prop, _)| name == "set_css_classes" && prop == "css_classes"),
            "setter with opener flags"
        );
        assert!(
            setter_call_at("btn.set_tooltip_text", 3).is_none(),
            "missing opener stays silent"
        );
        assert!(
            setter_call_at("btn.set_css_class(\"x\")", 3).is_none(),
            "near misses stay silent"
        );
    }

    #[test]
    fn redundant_true_stays_silent() {
        assert!(
            setter_call_at("btn.set_can_focus(true)", 3).is_none(),
            "redundant true belongs to the redundant-focus check"
        );
        assert!(
            setter_call_at("btn.set_can_focus(false)", 3).is_some(),
            "explicit opt-out still flags"
        );
    }
}
