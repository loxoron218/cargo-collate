//! Mnemonic-capable widget inventory for underline checks.
//!
//! Lists the `gtk4` widget basenames that expose `use_underline` on their
//! builder plus `set_use_underline` on the instance, so layout containers
//! never flag while buttons and labels always do.

/// Mnemonic-capable widget basenames with `use-underline` support.
///
/// Subset of the widget inventory covering buttons, labels, and expanders.
/// Names appear once even when shared across `gtk` crates.
const MNEMONIC_WIDGETS: [&str; 7] = [
    "Button",
    "CheckButton",
    "Expander",
    "Label",
    "LinkButton",
    "MenuButton",
    "ToggleButton",
];

/// True if `name` builds mnemonic-capable widgets.
///
/// # Arguments
///
/// * `name` - Type basename immediately before `::builder`.
///
/// # Returns
///
/// * `bool` - True for mnemonic-capable widget basenames.
#[must_use]
pub fn is_mnemonic_widget(name: &str) -> bool {
    MNEMONIC_WIDGETS.contains(&name)
}

/// Mnemonic-capable widget basenames, in inventory order.
///
/// # Returns
///
/// * `&[&str]` - Widget names with `use-underline` support.
#[must_use]
pub const fn mnemonic_widgets() -> &'static [&'static str] {
    &MNEMONIC_WIDGETS
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_underline::mnemonic_widgets::{
        is_mnemonic_widget, mnemonic_widgets,
    };

    #[test]
    fn mnemonic_set_splits_layout() {
        assert!(is_mnemonic_widget("Button"), "buttons count");
        assert!(is_mnemonic_widget("Label"), "labels count");
        assert!(!is_mnemonic_widget("Box"), "layout stays silent");
        assert!(!is_mnemonic_widget("Signal"), "non-widgets stay silent");
        assert_eq!(mnemonic_widgets().len(), 7, "inventory stays fixed");
    }
}
