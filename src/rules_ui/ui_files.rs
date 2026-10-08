//! Declarative UI and CSS file detectors for programmatic widgets.
//!
//! Flags stray `*.ui`, `*.blp`, `*.xml`, and `*.css` files on disk alongside
//! `*.rs` code references so widgets stay built programmatically with
//! `css_classes` instead.

use crate::{
    rules_tests::range::{in_ranges, stray_gap, test_ranges},
    rules_ui::ui_scan::has_ui_reference,
    scan::SourceFile,
};

/// Declarative file presence findings, one hit per file, sorted.
///
/// # Arguments
///
/// * `ui_paths` - Declarative `*.ui`, `*.blp`, `*.xml`, and `*.css` paths.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
fn presence_hits(ui_paths: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = ui_paths
        .iter()
        .map(|path| {
            format!("{path}: declarative UI/CSS file; build widgets programmatically instead")
        })
        .collect();
    hits.sort();
    hits.dedup();
    hits
}

/// True if `path` lives under an integration-test tree.
///
/// # Arguments
///
/// * `path` - Scan-relative file path.
///
/// # Returns
///
/// * `bool` - True inside `tests/` trees.
fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/") || path.contains("/tests/")
}

/// Code-reference findings for one file outside test code.
///
/// Inline `#[cfg(test)]` modules and integration-test trees stay exempt so
/// fixture literals in tests never flag; stray files on disk still report
/// through presence hits.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
fn file_reference_hits(file: &SourceFile) -> Vec<String> {
    if is_test_path(&file.path) {
        return Vec::new();
    }
    let exempt = test_ranges(&file.lines, stray_gap);
    file.lines
        .iter()
        .enumerate()
        .filter(|pair| !in_ranges(&exempt, pair.0) && has_ui_reference(pair.1))
        .map(|(idx, line)| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        .collect()
}

/// Code-reference findings across files.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
fn reference_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(file_reference_hits).collect()
}

/// Declarative UI/CSS files and string-literal references.
///
/// Combines stray files on disk with string-literal extension references in
/// `*.rs` source so both creation and use stay flagged; output is sorted and
/// deduplicated.
///
/// # Arguments
///
/// * `files` - Scanned source files.
/// * `ui_paths` - Declarative `*.ui`, `*.blp`, `*.xml`, and `*.css` paths.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
#[must_use]
pub fn ui_file_hits(files: &[SourceFile], ui_paths: &[String]) -> Vec<String> {
    let mut hits = presence_hits(ui_paths);
    hits.extend(reference_hits(files));
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::{rules_ui::ui_files::ui_file_hits, scan::support::test_file};

    #[test]
    fn presence_and_refs_combine() {
        let files = vec![
            test_file("src/a.rs", &["let s = \"window.ui\";", "let x = 1;"]),
            test_file("src/b.rs", &["fn f() {}"]),
        ];
        let paths = vec!["src/view.ui".to_owned(), "assets/style.css".to_owned()];
        let hits = ui_file_hits(&files, &paths);
        assert_eq!(hits.len(), 3, "presence plus refs combine");
        let dupes = vec!["src/view.ui".to_owned(), "src/view.ui".to_owned()];
        assert_eq!(ui_file_hits(&[], &dupes).len(), 1, "duplicates dedupe");
    }

    #[test]
    fn clean_files_stay_silent() {
        let files = vec![test_file("src/a.rs", &["fn f() {}", "let x = 1;"])];
        assert!(
            ui_file_hits(&files, &[]).is_empty(),
            "clean files stay silent"
        );
    }

    #[test]
    fn gated_modules_stay_silent() {
        let inline = vec![test_file(
            "src/a.rs",
            &["#[cfg(test)]", "mod tests {", "let s = \"window.ui\";", "}"],
        )];
        assert!(
            ui_file_hits(&inline, &[]).is_empty(),
            "inline test modules stay silent"
        );
        let integration = vec![test_file("tests/flow.rs", &["let s = \"window.ui\";"])];
        assert!(
            ui_file_hits(&integration, &[]).is_empty(),
            "integration tests stay silent"
        );
    }
}
