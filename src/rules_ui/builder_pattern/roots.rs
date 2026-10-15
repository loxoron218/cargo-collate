//! Import-root resolution for widget names.
//!
//! Maps in-scope basenames onto their crate roots plus import originals
//! through `use` statements, so direct calls resolve precisely: explicit
//! `std` imports silence smart-pointer lookalikes while gtk roots count and
//! aliases resolve through their originals. Glob imports bind nothing by
//! name; unknown and unmapped names fall back to file gating upstream.

use std::collections::BTreeMap;

use crate::{
    rules_alias::{
        collect::{collect_use_stmts, use_body},
        import_alias::{original_name, split_alias, strip_raw},
    },
    rules_self_import::expand::expand,
    rules_ui::missing_labels::widget_builders::{BUILDERLESS_WIDGETS, WIDGET_BUILDERS},
};

/// Crate roots whose types build UI widgets.
const UI_ROOTS: [&str; 4] = ["gtk", "gtk4", "libadwaita", "adw"];

/// Crate roots that never build UI widgets.
const STD_ROOTS: [&str; 3] = ["std", "core", "alloc"];

/// Binder with its crate root and import original, if usable.
///
/// Skips glob imports since they bind nothing by name.
///
/// # Arguments
///
/// * `expanded` - Expanded import with an optional `as` alias.
///
/// # Returns
///
/// * `Option<(String, String, String)>` - Bound name, crate root, original.
fn root_binder(expanded: &str) -> Option<(String, String, String)> {
    let (path_part, alias_opt) = split_alias(expanded);
    let mut segments = path_part
        .split("::")
        .map(|seg| strip_raw(seg.trim()))
        .filter(|seg| !seg.is_empty());
    let root = segments.next()?.to_owned();
    let original = original_name(&path_part)?;
    let binder = alias_opt.map_or_else(
        || original.clone(),
        |alias| strip_raw(alias.trim()).to_owned(),
    );
    if binder.contains('*') || root.contains('*') || binder.is_empty() || root.is_empty() {
        return None;
    }
    Some((binder, root, original))
}

/// In-scope crate roots keyed by bound name.
///
/// Later bindings overwrite earlier ones, mirroring shadowing.
///
/// # Arguments
///
/// * `lines` - Original file lines with strings intact.
///
/// # Returns
///
/// * `BTreeMap<String, (String, String)>` - Bound name to root and original.
#[must_use]
pub fn import_roots(lines: &[String]) -> BTreeMap<String, (String, String)> {
    collect_use_stmts(lines)
        .iter()
        .filter_map(|(_, _, buf)| use_body(buf))
        .flat_map(|body| expand(&body))
        .filter_map(|expanded| root_binder(&expanded))
        .map(|(binder, root, original)| (binder, (root, original)))
        .collect()
}

/// True if `root` ships UI widgets.
///
/// # Arguments
///
/// * `root` - First segment of an import path.
///
/// # Returns
///
/// * `bool` - True for gtk-family roots.
#[must_use]
pub fn is_ui_root(root: &str) -> bool {
    UI_ROOTS.contains(&root)
}

/// True if `root` never ships UI widgets.
///
/// # Arguments
///
/// * `root` - First segment of an import path.
///
/// # Returns
///
/// * `bool` - True for standard library roots.
#[must_use]
pub fn is_std_root(root: &str) -> bool {
    STD_ROOTS.contains(&root)
}

/// Inventory name for `base`, directly or through a gtk-rooted alias.
///
/// Plain inventory names always count; aliases only count on gtk roots, so
/// same-named foreign types stay silent while unknown roots fall back to
/// file gating upstream. Builderless widgets resolve like inventoried ones
/// so direct construction routes to the generic object builder.
///
/// # Arguments
///
/// * `base` - Basename before `::` at the call site.
/// * `roots` - Bound names with crate roots and import originals.
///
/// # Returns
///
/// * `Option<String>` - Inventory name when the basename names a widget.
#[must_use]
pub fn widget_name(base: &str, roots: &BTreeMap<String, (String, String)>) -> Option<String> {
    if WIDGET_BUILDERS.contains(&base) || BUILDERLESS_WIDGETS.contains(&base) {
        return Some(base.to_owned());
    }
    let (root, original) = roots.get(base)?;
    if !is_ui_root(root) {
        return None;
    }
    if WIDGET_BUILDERS.contains(&original.as_str())
        || BUILDERLESS_WIDGETS.contains(&original.as_str())
    {
        return Some(original.clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::builder_pattern::roots::{import_roots, is_std_root, is_ui_root};

    #[test]
    fn plain_imports_bind_roots() {
        let lines = vec!["use gtk::Button;".to_owned()];
        let roots = import_roots(&lines);
        assert_eq!(
            roots.get("Button"),
            Some(&("gtk".to_owned(), "Button".to_owned())),
            "plain import binds root and original"
        );
    }

    #[test]
    fn aliases_bind_alias_with_original() {
        let lines = vec!["use gtk::Button as GtkButton;".to_owned()];
        let roots = import_roots(&lines);
        assert_eq!(
            roots.get("GtkButton"),
            Some(&("gtk".to_owned(), "Button".to_owned())),
            "alias binds with original"
        );
        let grouped = vec!["use std::{boxed::Box, sync::Arc};".to_owned()];
        let grouped_roots = import_roots(&grouped);
        assert_eq!(
            grouped_roots.get("Box"),
            Some(&("std".to_owned(), "Box".to_owned())),
            "grouped std import binds"
        );
    }

    #[test]
    fn globs_stay_out() {
        let lines = vec!["use gtk::*;".to_owned()];
        assert!(import_roots(&lines).is_empty(), "glob imports bind nothing");
    }

    #[test]
    fn roots_classify() {
        for root in ["gtk", "gtk4", "libadwaita", "adw"] {
            assert!(is_ui_root(root), "{root} counts");
        }
        assert!(!is_ui_root("std"), "std stays out");
        for root in ["std", "core", "alloc"] {
            assert!(is_std_root(root), "{root} counts");
        }
        assert!(!is_std_root("gtk"), "gtk stays out");
    }
}
