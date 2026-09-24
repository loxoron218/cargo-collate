//! Comments attached to functions inside test modules.
//!
//! Private tests stay undocumented; any comment directly above a private
//! function inside a `#[cfg(test)]` module is flagged, whether or not it
//! carries `#[test]`. Explicitly `pub` helpers are shared across test
//! modules, so their docs serve an inter-module contract and stay allowed.

use crate::{
    lexer::{
        literal::strip_strings,
        word::{declared_fn_name, has_word},
    },
    rules_tests::range::{code_of, in_ranges, stray_gap, test_ranges},
    scan::{SourceFile, indexed_lines},
};

/// True if `stripped` is a comment line of any flavor.
fn is_any_comment(stripped: &str) -> bool {
    stripped.trim_start().starts_with("//")
}

/// True if `code` is another attribute line to skip over.
fn is_attr_line(code: &str) -> bool {
    code.trim_start().starts_with('#')
}

/// Lines attached above `idx`: adjacent comment and attribute lines.
///
/// A blank line or any other code line ends the attachment. Lines yield
/// nearest-first so single-pass folds keep issue order.
#[must_use]
pub fn attached_above(lines: &[String], idx: usize) -> Vec<(usize, &String)> {
    let mut out = Vec::new();
    let mut j = idx;
    while j > 0 {
        j = j.saturating_sub(1);
        let Some(line) = lines.get(j) else {
            break;
        };
        if line.trim().is_empty() {
            break;
        }
        let stripped = strip_strings(line);
        if !(is_any_comment(&stripped) || is_attr_line(&code_of(line))) {
            break;
        }
        out.push((j, line));
    }
    out
}

/// Comment line numbers directly attached above the `fn` at `idx`.
fn comments_above(lines: &[String], idx: usize) -> Vec<usize> {
    attached_above(lines, idx)
        .into_iter()
        .filter(|(_, line)| is_any_comment(&strip_strings(line)))
        .map(|(j, _)| j)
        .collect()
}

/// Display hit for the attached comment line `j`, if present.
fn comment_hit(file: &SourceFile, j: usize) -> Option<String> {
    file.lines
        .get(j)
        .map(|line| format!("{}:{}:{line}", file.path, j.saturating_add(1)))
}

/// Comments attached to private functions inside test modules.
///
/// Private tests stay undocumented; explicitly `pub` helpers are shared
/// across test modules and keep their docs. Classification runs on blanked
/// lines while display keeps original lines.
#[must_use]
pub fn unnecessary_test_docs(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            let ranges = test_ranges(&file.lines, stray_gap);
            indexed_lines(file)
                .filter(|(idx, (_, code))| {
                    in_ranges(&ranges, *idx)
                        && declared_fn_name(&code_of(code)).is_some()
                        && !has_word(&code_of(code), "pub")
                })
                .flat_map(|(idx, _)| {
                    comments_above(&file.stripped, idx)
                        .into_iter()
                        .filter_map(|j| comment_hit(file, j))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_tests::test_docs::unnecessary_test_docs, scan::support::test_file};

    #[test]
    fn docs_attached_flagged() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "/// Summary.",
                    "#[test]",
                    "fn t() {}",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "#[test]",
                    "fn plain() {}",
                    "}",
                ],
            ),
        ];
        let hits = unnecessary_test_docs(&files);
        assert_eq!(hits.len(), 1, "attached doc flags");
        assert!(
            hits.first().is_some_and(|h| h.starts_with("src/a.rs")),
            "doc file flagged"
        );
    }

    #[test]
    fn helper_docs_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "#[cfg(test)]",
                "mod tests {",
                "/// Makes a track.",
                "fn make_track() {}",
                "}",
            ],
        )];
        assert_eq!(unnecessary_test_docs(&files).len(), 1, "helper doc flags");
    }

    #[test]
    fn pub_helper_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "#[cfg(test)]",
                    "pub mod tests {",
                    "/// Makes a track.",
                    "pub fn make_track() {}",
                    "}",
                ],
            ),
            test_file(
                "src/b.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "/// Connects.",
                    "pub async fn storage_in() {}",
                    "}",
                ],
            ),
        ];
        assert!(
            unnecessary_test_docs(&files).is_empty(),
            "shared pub helpers stay silent"
        );
    }

    #[test]
    fn outside_range_silent() {
        let files = vec![
            test_file("src/a.rs", &["/// Docs.", "fn f() {}"]),
            test_file(
                "src/b.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "let s = \"/// Summary.\";",
                    "fn f() {}",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "#[cfg(test)]",
                    "mod tests {",
                    "/// Orphan.",
                    "",
                    "fn t() {}",
                    "}",
                ],
            ),
        ];
        assert!(
            unnecessary_test_docs(&files).is_empty(),
            "outside code and non-attached stays silent"
        );
    }
}
