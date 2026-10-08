//! Import-alias resolution for accessibility labels.
//!
//! Collects the in-scope names bound to `accessible::Property::Label`
//! through `use ... as ...` imports, so `PropertyLabel(...)` and
//! `A11yLabel(...)` satisfy the rule exactly like bare `Label(...)`.

use std::collections::BTreeSet;

use crate::{
    rules_alias::{
        collect::{collect_use_stmts, use_body},
        import_alias::{original_name, split_alias, strip_raw},
    },
    rules_self_import::expand::expand,
};

/// True if `path` ends in `accessible::Property::Label`.
///
/// # Arguments
///
/// * `path` - Expanded import path without its alias.
///
/// # Returns
///
/// * `bool` - True for the accessible label property path.
fn is_label_path(path: &str) -> bool {
    let segs: Vec<&str> = path
        .split("::")
        .map(|seg| strip_raw(seg.trim()))
        .filter(|seg| !seg.is_empty())
        .collect();
    let len = segs.len();
    len >= 3
        && segs
            .get(len.saturating_sub(3))
            .is_some_and(|seg| *seg == "accessible")
        && segs
            .get(len.saturating_sub(2))
            .is_some_and(|seg| *seg == "Property")
        && segs
            .get(len.saturating_sub(1))
            .is_some_and(|seg| *seg == "Label")
}

/// Binder introduced by one expanded import for the label property, if any.
///
/// # Arguments
///
/// * `expanded` - Expanded import with an optional `as` alias.
///
/// # Returns
///
/// * `Option<String>` - In-scope name when the path targets the label.
fn label_binder(expanded: &str) -> Option<String> {
    let (path_part, alias_opt) = split_alias(expanded);
    if !is_label_path(&path_part) {
        return None;
    }
    if let Some(alias_raw) = alias_opt {
        let alias = strip_raw(alias_raw.trim()).trim().to_owned();
        if !alias.is_empty() && !alias.contains('*') && !alias.contains(':') {
            return Some(alias);
        }
        return None;
    }
    original_name(&path_part)
}

/// Insert label binders from one `use` statement body.
///
/// # Arguments
///
/// * `names` - Accepted label names under construction.
/// * `buf` - Joined `use` statement code.
///
/// # Returns
///
/// * `()` - No result; `names` gains any label binders.
fn insert_label_binders(names: &mut BTreeSet<String>, buf: &str) {
    let Some(body) = use_body(buf) else {
        return;
    };
    for expanded in expand(&body) {
        if let Some(binder) = label_binder(&expanded) {
            names.extend([binder]);
        }
    }
}

/// In-scope label names for `lines`, including the bare fallback.
///
/// # Arguments
///
/// * `lines` - Original file lines with strings intact.
///
/// # Returns
///
/// * `BTreeSet<String>` - Accepted call names inside `update_property`.
#[must_use]
pub fn label_aliases(lines: &[String]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, _, buf) in collect_use_stmts(lines) {
        insert_label_binders(&mut names, &buf);
    }
    names.extend(["Label".to_owned()]);
    names
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_labels::labels_alias::label_aliases;

    #[test]
    fn plain_import_binds_label() {
        let lines = vec!["use gtk::accessible::Property::Label;".to_owned()];
        let names = label_aliases(&lines);
        assert!(names.contains("Label"), "bare name binds");
    }

    #[test]
    fn aliases_resolve_through_groups() {
        let lines =
            vec!["use gtk::{Button, accessible::Property::Label as PropertyLabel};".to_owned()];
        let names = label_aliases(&lines);
        assert!(names.contains("PropertyLabel"), "alias binds");
        assert!(names.contains("Label"), "fallback stays");
        let grouped = vec![
            "use gtk::{".to_owned(),
            "    accessible::Property::Label as A11yLabel,".to_owned(),
            "};".to_owned(),
        ];
        assert!(
            label_aliases(&grouped).contains("A11yLabel"),
            "multiline alias binds"
        );
    }

    #[test]
    fn unrelated_imports_stay_out() {
        let lines = vec!["use gtk::{Button, Label as GtkLabel};".to_owned()];
        let names = label_aliases(&lines);
        assert!(!names.contains("GtkLabel"), "non-label paths stay out");
        assert!(names.contains("Label"), "fallback stays");
    }
}
