//! Cfg-test marker detectors for single-suite files.

use crate::{
    lexer::cfg_marker::is_real_cfg_line,
    rules_harness::range::{stray_gap, test_ranges},
    scan::SourceFile,
};

/// Files declaring more than one inline cfg-test module.
///
/// Gated file declarations (`#[cfg(test)] mod foo;`) stay unlimited; only
/// bodied suites count, so delegated test files never trip the single-suite
/// rule.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
#[must_use]
pub fn multi_cfg_files(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| {
            let total = test_ranges(&file.lines, stray_gap).len();
            (total > 1).then(|| format!("{}: {total}", file.path))
        })
        .collect()
}

/// Paths of files holding at least one cfg-test attribute.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Paths with cfg-test markers.
#[must_use]
pub fn cfg_test_files(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .filter(|f| f.lines.iter().any(|l| is_real_cfg_line(l)))
        .map(|f| f.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::cfg_marker::is_real_cfg_line,
        rules_simple::cfg_harness::{cfg_test_files, multi_cfg_files},
        scan::builders::test_file,
    };

    #[test]
    fn multi_cfg_counts_files() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["#[cfg(test)]", "mod a {", "}", "#[cfg(test)]", "mod b {"],
            ),
            test_file("src/b.rs", &["#[cfg(test)]", "mod tests {", "}"]),
        ];
        assert_eq!(multi_cfg_files(&files), vec!["src/a.rs: 2".to_owned()]);
    }

    #[test]
    fn multi_cfg_allows_file_gates() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "pub mod auth;",
                    "",
                    "#[cfg(test)]",
                    "mod cases;",
                    "#[cfg(test)]",
                    "mod regression;",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "}",
                    "#[cfg(test)]",
                    "mod helper;",
                ],
            ),
        ];
        assert!(
            multi_cfg_files(&files).is_empty(),
            "file gates stay unlimited with or without one inline suite"
        );
    }

    #[test]
    fn cfg_markers_detected() {
        assert!(is_real_cfg_line("#[cfg(test)]"));
        assert!(is_real_cfg_line("#[ cfg ( test ) ]"));
        assert!(!is_real_cfg_line("#[cfg(not(test))]"));
        assert!(!is_real_cfg_line("#[cfg(all(test))]"));
        let files = vec![
            test_file("src/a.rs", &["#[cfg(test)]", "mod tests {", "}"]),
            test_file("src/b.rs", &["fn f() {}"]),
        ];
        assert_eq!(cfg_test_files(&files), vec!["src/a.rs".to_owned()]);
        assert!(
            multi_cfg_files(&files).is_empty(),
            "single blocks stay silent"
        );
    }
}
