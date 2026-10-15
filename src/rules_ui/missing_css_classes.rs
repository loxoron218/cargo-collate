//! Missing default-class detectors for widget builders.
//!
//! Parent index for the capability. The `css_inventory` submodule holds the
//! upstream widget inventory, `css_parse` holds class-list parsing,
//! `class_literal` holds literal extraction, `class_require` merges always
//! with content requirements, `css_chain` flags builder lists dropping
//! defaults, `setter_hits` flags post-build setter lists dropping defaults
//! with `setter_resolve` mapping receivers onto builders and `setter_scan`
//! holding call-shape scans, so every programmatic widget keeps its
//! auto-added classes. Nothing stays exempt: test code flags like library
//! code.

pub mod class_literal;
pub mod class_require;
pub mod css_chain;
pub mod css_inventory;
pub mod css_parse;
pub mod setter_hits;
pub mod setter_resolve;
pub mod setter_scan;

use crate::{
    rules_ui::missing_css_classes::{css_chain::css_chain_hits, setter_hits::setter_hits},
    scan::SourceFile,
};

/// Widget builders dropping required default CSS classes.
///
/// Flags inventoried builder `css_classes` lists and post-build
/// `set_css_classes` lists omitting always-required or triggered content
/// classes. Dynamic variables stay silent while empty literals flag.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with fix hints appended.
#[must_use]
pub fn missing_css_classes_hits(files: &[SourceFile]) -> Vec<String> {
    let mut hits = css_chain_hits(files);
    hits.extend(setter_hits(files));
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::missing_css_classes::missing_css_classes_hits, scan::support::test_file,
    };

    #[test]
    fn builder_plus_setter_flag() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::LinkButton;",
                "fn contact() {",
                "    let a = LinkButton::builder().css_classes([\"flat\"]).build();",
                "    let b = LinkButton::builder().build();",
                "    b.set_css_classes(&[\"flat\"]);",
                "}",
            ],
        )];
        assert_eq!(
            missing_css_classes_hits(&files).len(),
            2,
            "chain and setter each flag"
        );
    }

    #[test]
    fn complete_lists_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::LinkButton;",
                "fn contact() {",
                "    let a = LinkButton::builder().css_classes([\"link\"]).build();",
                "    a.set_css_classes(&[\"link\", \"flat\"]);",
                "}",
            ],
        )];
        assert!(
            missing_css_classes_hits(&files).is_empty(),
            "complete lists stay silent"
        );
    }

    #[test]
    fn foreign_builders_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn sync() {",
                "    let c = Client::builder().css_classes([\"flat\"]).build();",
                "    c.set_css_classes(&[\"flat\"]);",
                "}",
            ],
        )];
        assert!(
            missing_css_classes_hits(&files).is_empty(),
            "non-widget builders stay silent"
        );
    }
}
