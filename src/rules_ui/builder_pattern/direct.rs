//! Direct-construction and post-build setter detectors.
//!
//! Flags inventoried widget `Type::new(`-style calls and
//! `.set_css_classes(`-style post-build setters, so widgets route through
//! builders carrying `css_classes`, `tooltip_text`, and `can_focus` instead.
//! `set_focusable` counts as the `can_focus` setter spelling.

use std::collections::BTreeMap;

use crate::{
    rules_ui::{
        builder_pattern::{
            box_shape::is_gtk_box_new,
            file_scan_hits,
            roots::{is_std_root, widget_name},
            trigger::{ident_end_after, ident_start_before, is_ctor_name},
        },
        missing_labels::labels_trigger::cursor_after_gap,
    },
    scan::SourceFile,
};

/// Post-build setters with the builder property they belong to.
const SETTER_PROPS: [(&str, &str); 4] = [
    ("set_css_classes", "css_classes"),
    ("set_tooltip_text", "tooltip_text"),
    ("set_can_focus", "can_focus"),
    ("set_focusable", "can_focus"),
];

/// Widget basename and call name at `sep`, if directly constructed.
///
/// Matches `Widget::assoc(` where the basename sits in the widget inventory
/// directly or through a gtk-rooted import alias, and the associated name
/// belongs to a constructor family, so types, constants, and class-level
/// setters stay silent while constructor spellings flag. Explicit `std`
/// imports always mean the smart pointer; other mapped roots count as
/// widgets while unknown and unmapped names fall back to file gating
/// upstream. Gtk boxes only count in their two-argument shape since
/// `std::boxed::Box` shares the basename.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `sep` - Byte index of the first `:` in `::`.
/// * `roots` - Bound names with crate roots and import originals.
///
/// # Returns
///
/// * `Option<(String, String)>` - In-scope name and inventory name on calls.
fn direct_call_at(
    body: &str,
    sep: usize,
    roots: &BTreeMap<String, (String, String)>,
) -> Option<(String, String)> {
    let bytes = body.as_bytes();
    if bytes.get(sep) != Some(&b':') || bytes.get(sep.saturating_add(1)) != Some(&b':') {
        return None;
    }
    let base_start = ident_start_before(bytes, sep)?;
    let base = body.get(base_start..sep)?;
    let widget = widget_name(base, roots)?;
    if roots.get(base).is_some_and(|(root, _)| is_std_root(root)) {
        return None;
    }
    let assoc_start = sep.saturating_add(2);
    let assoc_end = ident_end_after(bytes, assoc_start)?;
    let assoc = body.get(assoc_start..assoc_end)?;
    if !is_ctor_name(assoc) {
        return None;
    }
    let open = cursor_after_gap(bytes, assoc_end);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    if widget == "Box" && (assoc != "new" || !is_gtk_box_new(body, open)) {
        return None;
    }
    Some((base.to_owned(), widget))
}

/// Setter name and its builder property at `dot`, if any.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `dot` - Byte index of the candidate `.`.
///
/// # Returns
///
/// * `Option<(&str, &str)>` - Setter name and builder property on matches.
fn setter_call_at(body: &str, dot: usize) -> Option<(&str, &str)> {
    let bytes = body.as_bytes();
    if bytes.get(dot) != Some(&b'.') {
        return None;
    }
    let name_start = dot.saturating_add(1);
    let name_end = ident_end_after(bytes, name_start)?;
    let name = body.get(name_start..name_end)?;
    let prop = SETTER_PROPS
        .iter()
        .find(|(setter, _)| *setter == name)
        .map(|(_, prop)| *prop)?;
    let open = cursor_after_gap(bytes, name_end);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    Some((name, prop))
}

/// Finding for direct or setter calls opening at `idx`, if any.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `text` - Joined code text with strings blanked and tails cut.
/// * `idx` - Byte index under scan.
/// * `lineno` - One-based line number holding `idx`.
///
/// # Returns
///
/// * `Option<String>` - Hit line with a fix hint on direct calls.
fn direct_hit_at(file: &SourceFile, text: &str, idx: usize, lineno: usize) -> Option<String> {
    if let Some((base, _)) = direct_call_at(text, idx, &file.roots) {
        let line = file
            .lines
            .get(lineno.saturating_sub(1))
            .map_or("", String::as_str);
        return Some(format!(
            "{}:{lineno}:{line} help: use {base}::builder() with .css_classes(...), \
             .tooltip_text(...), and .can_focus(...) instead",
            file.path
        ));
    }
    if let Some((_, prop)) = setter_call_at(text, idx) {
        let line = file
            .lines
            .get(lineno.saturating_sub(1))
            .map_or("", String::as_str);
        return Some(format!(
            "{}:{lineno}:{line} help: move this into the builder as .{prop}(...) instead",
            file.path
        ));
    }
    None
}

/// Direct-construction and setter findings for one file.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
fn file_direct_hits(file: &SourceFile) -> Vec<String> {
    file_scan_hits(file, direct_hit_at)
}

/// Direct-construction and setter findings across files.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
pub fn direct_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(file_direct_hits).collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::builder_pattern::direct::direct_hits, scan::support::test_file};

    #[test]
    fn direct_construction_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::{Button, Label};",
                "fn f() {",
                "    let a = Button::new(\"label\");",
                "    let b = Label::default();",
                "    let c = gtk::DropDown::from_icon_name(\"x\");",
                "}",
            ],
        )];
        let hits = direct_hits(&files);
        assert_eq!(hits.len(), 3, "every constructor spelling flags");
        assert!(
            hits.iter().all(|hit| hit.contains("builder()")),
            "every hint routes to the builder"
        );
    }

    #[test]
    fn boxed_shapes_split() {
        let gtk_box = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::{Box, Orientation::Vertical};",
                "fn f() {",
                "    let b = Box::new(Vertical, 6);",
                "    let c = Box::new(gtk::Orientation::Horizontal, 12);",
                "}",
            ],
        )];
        assert_eq!(direct_hits(&gtk_box).len(), 2, "orientation shapes flag");
        let std_box = vec![
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Box;",
                    "fn f() {",
                    "    let b = Box::new(value);",
                    "    let p = Box::pin(work());",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "use gtk::Box;",
                    "fn f() {",
                    "    let b = Box::new(move |_| handle());",
                    "    let d: Box<dyn Ink> = Box::default();",
                    "}",
                ],
            ),
        ];
        assert!(
            direct_hits(&std_box).is_empty(),
            "single-value shapes stay silent"
        );
    }

    #[test]
    fn builder_heads_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn f() {",
                "    let btn = Button::builder()",
                "        .css_classes([\"x\"])",
                "        .tooltip_text(\"y\")",
                "        .can_focus(true)",
                "        .build();",
                "    btn",
                "}",
            ],
        )];
        assert!(direct_hits(&files).is_empty(), "builder chains stay silent");
    }

    #[test]
    fn setters_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "fn f() {",
                "    btn.set_css_classes(&[]);",
                "    btn.set_tooltip_text(\"x\");",
                "    btn.set_can_focus(true);",
                "    btn.set_focusable(true);",
                "}",
            ],
        )];
        let hits = direct_hits(&files);
        assert_eq!(hits.len(), 4, "every setter spelling flags");
        assert!(
            hits.iter().all(|hit| hit.contains("into the builder")),
            "every hint routes into the builder"
        );
    }

    #[test]
    fn non_widget_calls_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    Client::new();",
                    "    Window::set_default_icon_name(\"app\");",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    Signal::emit(\"x\");",
                    "}",
                ],
            ),
            test_file("src/c.rs", &["use gtk::Button;", "fn f() {}"]),
        ];
        assert!(
            direct_hits(&files).is_empty(),
            "non-widget calls and class setters stay silent"
        );
    }

    #[test]
    fn lookalikes_stay_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let s = \"Button::new()\";",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    let role = Button::Role;",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "use gtk::Button;",
                    "fn f() {",
                    "    btn.set_css_class(\"x\");",
                    "}",
                ],
            ),
        ];
        assert!(
            direct_hits(&files).is_empty(),
            "strings, constants, and near misses stay silent"
        );
    }

    #[test]
    fn std_mapped_names_stay_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button;",
                "use std::boxed::Box;",
                "fn f() {",
                "    let b = Box::new(value);",
                "    let p = Box::pin(work());",
                "}",
            ],
        )];
        assert!(
            direct_hits(&files).is_empty(),
            "explicit std imports stay silent"
        );
    }

    #[test]
    fn aliased_widgets_flag() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::Button as GtkButton;",
                "fn f() {",
                "    let b = GtkButton::new(\"label\");",
                "}",
            ],
        )];
        let hits = direct_hits(&files);
        assert_eq!(hits.len(), 1, "aliased construction flags");
        assert!(
            hits.first()
                .is_some_and(|hit| hit.contains("GtkButton::builder()")),
            "hint keeps the in-scope spelling"
        );
        let foreign = vec![test_file(
            "src/b.rs",
            &[
                "use gtk::Button;",
                "use html2gtk::Label as LabelElement;",
                "fn f() {",
                "    let element = LabelElement::from(paragraph);",
                "}",
            ],
        )];
        assert!(
            direct_hits(&foreign).is_empty(),
            "foreign aliases stay silent"
        );
    }
}
