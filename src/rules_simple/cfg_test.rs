//! Cfg-test marker detectors for single-suite files.

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    rules_tests::range::{stray_gap, test_ranges},
    scan::SourceFile,
};

/// Index after ASCII whitespace from `i`.
///
/// # Arguments
///
/// * `bytes` - Raw line bytes.
/// * `i` - Start index.
///
/// # Returns
///
/// * `usize` - First non-whitespace index at or after `i`.
pub fn skip_ws_at(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i = i.saturating_add(1);
    }
    i
}

/// Index after `token` at `i` with whitespace allowed before it, if any.
fn skip_token(bytes: &[u8], i: usize, token: &str) -> Option<usize> {
    let start = skip_ws_at(bytes, i);
    let end = start.saturating_add(token.len());
    (bytes.get(start..end) == Some(token.as_bytes())).then_some(end)
}

/// True if an exact cfg-test marker opens at byte `i`.
///
/// Only `cfg(test)` with optional whitespace counts; predicates like
/// `not(test)` stay excluded.
fn cfg_test_open(bytes: &[u8], i: usize) -> bool {
    let mut j = i;
    for token in ["#", "[", "cfg", "(", "test", ")", "]"] {
        let Some(next) = skip_token(bytes, j, token) else {
            return false;
        };
        j = next;
    }
    true
}

/// True if `line` holds a real cfg-test attribute outside strings and tails.
///
/// # Arguments
///
/// * `line` - Original source line.
///
/// # Returns
///
/// * `bool` - True when `#[cfg(test)]` appears.
#[must_use]
pub fn is_real_cfg_line(line: &str) -> bool {
    let stripped = strip_strings(line);
    let code = cut_line_comment(&stripped);
    if code.contains("/*") {
        return false;
    }
    let bytes = code.as_bytes();
    (0..bytes.len()).any(|i| cfg_test_open(bytes, i))
}

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
        rules_simple::cfg_test::{cfg_test_files, is_real_cfg_line, multi_cfg_files},
        scan::support::test_file,
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
