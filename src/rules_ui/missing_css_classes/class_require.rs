//! Requirement merging for default-class checks.
//!
//! Holds content-driven pushers plus the always-plus-content merger, so
//! builder and setter scans report one sorted hint. Triggers resolve from
//! the backing chain while class presence resolves through the shared
//! inventory.

use crate::rules_ui::{
    builder_pattern::trigger::has_method_call,
    missing_css_classes::css_inventory::{contains_class, method_bool_arg, missing_always},
};

/// Push `text-button` for label-bearing buttons, if missing.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// * `missing` - Mutable missing spellings.
fn push_text_button(widget: &str, segment: &str, classes: &[String], missing: &mut Vec<String>) {
    if (widget == "Button" || widget == "SplitButton")
        && has_method_call(segment, "label")
        && !contains_class(classes, "text-button")
    {
        missing.push("text-button".to_owned());
    }
}

/// Push `image-button` for icon-bearing buttons, if missing.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// * `missing` - Mutable missing spellings.
fn push_image_button(widget: &str, segment: &str, classes: &[String], missing: &mut Vec<String>) {
    if (widget == "Button" || widget == "SplitButton")
        && has_method_call(segment, "icon_name")
        && !contains_class(classes, "image-button")
    {
        missing.push("image-button".to_owned());
    }
}

/// Push `text-button` for labeled check buttons, if missing.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// * `missing` - Mutable missing spellings.
fn push_check_text(widget: &str, segment: &str, classes: &[String], missing: &mut Vec<String>) {
    if widget == "CheckButton"
        && has_method_call(segment, "label")
        && !contains_class(classes, "text-button")
    {
        missing.push("text-button".to_owned());
    }
}

/// Push `flat` for frameless entries and buttons, if missing.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// * `missing` - Mutable missing spellings.
fn push_flat(widget: &str, segment: &str, classes: &[String], missing: &mut Vec<String>) {
    if (widget == "Button" || widget == "Entry")
        && method_bool_arg(segment, "has_frame", "false")
        && !contains_class(classes, "flat")
    {
        missing.push("flat".to_owned());
    }
}

/// Push `separators` for separator-showing lists, if missing.
///
/// Both `ListView` and `ListBox` add `separators` to their own node when
/// `show-separators` passes `true`, so explicit opt-ins keep the class.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// * `missing` - Mutable missing spellings.
fn push_separators(widget: &str, segment: &str, classes: &[String], missing: &mut Vec<String>) {
    if (widget == "ListView" || widget == "ListBox")
        && method_bool_arg(segment, "show_separators", "true")
        && !contains_class(classes, "separators")
    {
        missing.push("separators".to_owned());
    }
}

/// Required classes missing from a `css_classes` list for `widget`.
///
/// Merges always-required with triggered content classes, sorted and
/// deduplicated so builder and setter scans report one hint.
///
/// # Arguments
///
/// * `widget` - Inventoried basename before `::builder`.
///
/// * `segment` - Chain text with strings blanked and tails cut.
///
/// * `classes` - Parsed class names from the list.
///
/// # Returns
///
/// * `Vec<String>` - Missing spellings in sorted order.
#[must_use]
pub fn collect_missing(widget: &str, segment: &str, classes: &[String]) -> Vec<String> {
    let mut missing = missing_always(widget, segment, classes);
    push_text_button(widget, segment, classes, &mut missing);
    push_image_button(widget, segment, classes, &mut missing);
    push_check_text(widget, segment, classes, &mut missing);
    push_flat(widget, segment, classes, &mut missing);
    push_separators(widget, segment, classes, &mut missing);
    missing.sort();
    missing.dedup();
    missing
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_css_classes::class_require::collect_missing;

    #[test]
    fn triggers_merge() {
        assert_eq!(
            collect_missing("Button", ".label(\"\")", &[]).len(),
            1,
            "label trigger flags"
        );
        assert!(
            collect_missing("Button", "", &[]).is_empty(),
            "plain stays silent"
        );
    }

    #[test]
    fn separators_need_opt_in() {
        assert_eq!(
            collect_missing("ListView", ".show_separators(true)", &["view".to_owned()]).len(),
            1,
            "opt-in flags separators"
        );
        assert!(
            collect_missing("ListBox", "", &[]).is_empty(),
            "default stays silent"
        );
    }
}
