//! Text-level rules without cross-line state.

pub mod expect;
pub mod hygiene;
pub mod layout;
pub mod manifest;
pub mod naming;
pub mod underscore;

use crate::{
    lexer::{
        literal::strip_strings,
        marker::{cut_line_comment, has_plain_line_comment},
    },
    rules_tests::range::{stray_gap, test_ranges},
    scan::{SourceFile, indexed_lines},
};

/// Plain line markers with doc markers exempt.
#[must_use]
pub fn line_comments(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file)
                .filter(|(_, (_, code))| has_plain_line_comment(&strip_strings(code)))
                .map(|(idx, (line, _))| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        })
        .collect()
}

/// Index after ASCII whitespace from `i`.
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
#[must_use]
pub fn cfg_test_files(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .filter(|f| f.lines.iter().any(|l| is_real_cfg_line(l)))
        .map(|f| f.path.clone())
        .collect()
}

/// Source files over the line limit.
#[must_use]
pub fn long_files(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .filter(|f| f.lines.len() > 400)
        .map(|f| format!("{}: {} lines", f.path, f.lines.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::{
            cfg_test_files,
            hygiene::{
                doc_blank_hits, dot_ok_hits, has_dot_ok, has_include_macro, has_low_log,
                has_parent_path, has_path_attr, has_scoped_pub,
            },
            is_real_cfg_line, line_comments,
            manifest::{manifest_paths_for, package_dir_for, prefixed_manifest_roots},
            multi_cfg_files,
        },
        scan::{discover_roots, support::test_file},
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
    fn native_detectors_work() {
        assert!(has_dot_ok("a.ok()"));
        assert!(!has_dot_ok("let x = 1;"));
        assert!(has_parent_path("use super::foo;"));
        assert!(has_scoped_pub("pub(crate) fn f() {}"));
        assert!(has_scoped_pub("let x = foo!(bar => pub(crate));"));
        assert!(!has_scoped_pub("pub fn f() {}"));
        assert!(has_path_attr("#[path = \"a.rs\"]"));
        assert!(!has_path_attr("#[template(path = \"a.html\")]"));
        assert!(has_include_macro("include!(\"a\")"));
        assert!(has_low_log("trace!(\"x\")"));
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
        assert!(dot_ok_hits(&files).is_empty(), "clean files stay silent");
    }

    #[test]
    fn root_selection_covers_everything() {
        assert_eq!(discover_roots(), vec![".".to_owned()]);
    }

    #[test]
    fn package_dirs_cover_tool_trees() {
        assert_eq!(package_dir_for("src/a.rs"), "");
        assert_eq!(package_dir_for("collate/src/a.rs"), "collate");
        assert_eq!(
            package_dir_for("fuzz/fuzz_targets/parse.rs"),
            "",
            "manifest-less arbitrary roots stay repo-rooted"
        );
        let files = vec!["collate/src/a.rs".to_owned()];
        assert_eq!(
            manifest_paths_for(&files),
            vec!["Cargo.toml".to_owned(), "collate/Cargo.toml".to_owned()]
        );
        let roots = prefixed_manifest_roots("collate/Cargo.toml", "[bin]\npath = \"src/main.rs\"");
        assert_eq!(roots, vec!["collate/src/main.rs".to_owned()]);
    }

    #[test]
    fn raw_string_comment_lines_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "let q = r#\"",
                "// look, a comment",
                "https://example.com/x",
                "\"#;",
                "fn f() {}",
            ],
        )];
        assert!(
            line_comments(&files).is_empty(),
            "raw interiors never count as comments"
        );
    }

    #[test]
    fn doc_blank_flags_opener_after_code() {
        let files = vec![test_file(
            "src/a.rs",
            &["fn helper() {}", "/// Loads an item."],
        )];
        assert_eq!(doc_blank_hits(&files).len(), 1);
        let clean = vec![test_file(
            "src/a.rs",
            &["fn helper() {}", "", "/// Loads an item."],
        )];
        assert!(
            doc_blank_hits(&clean).is_empty(),
            "opener after blank stays silent"
        );
    }

    #[test]
    fn doc_blank_edge_shapes() {
        let bof = vec![test_file("src/a.rs", &["/// First line."])];
        assert!(doc_blank_hits(&bof).is_empty(), "BOF stays silent");
        let cont = vec![test_file(
            "src/a.rs",
            &["fn f() {}", "", "/// First.", "/// Second."],
        )];
        assert!(
            doc_blank_hits(&cont).is_empty(),
            "continuations stay silent"
        );
        let quad = vec![test_file("src/a.rs", &["fn f() {}", "//// Rule"])];
        assert!(doc_blank_hits(&quad).is_empty(), "quadruple stays silent");
        let trailing = vec![test_file("src/a.rs", &["let x = 1; /// trailing"])];
        assert!(
            doc_blank_hits(&trailing).is_empty(),
            "trailing stays silent"
        );
        let in_string = vec![test_file("src/a.rs", &[r#"let s = "/// not doc";"#])];
        assert!(
            doc_blank_hits(&in_string).is_empty(),
            "string contents stay silent"
        );
        let ws_blank = vec![test_file("src/a.rs", &["fn f() {}", "   ", "/// Doc."])];
        assert!(
            doc_blank_hits(&ws_blank).is_empty(),
            "whitespace counts as blank"
        );
    }

    #[test]
    fn doc_blank_paren_opener_exempt() {
        let wrapped = vec![test_file(
            "src/a.rs",
            &[
                "signed_request!(",
                "    /// Sends a signed GET request.",
                "    pub fn f() {}",
                ");",
            ],
        )];
        assert!(
            doc_blank_hits(&wrapped).is_empty(),
            "paren openers stay silent"
        );
        let after_close = vec![test_file(
            "src/a.rs",
            &["let x = foo(", "    bar,", ");", "/// Docs without blank."],
        )];
        assert_eq!(
            doc_blank_hits(&after_close).len(),
            1,
            "openers after close still flag"
        );
        let paren_string = vec![test_file("src/a.rs", &[r#"let s = "(";"#, "/// Docs."])];
        assert_eq!(
            doc_blank_hits(&paren_string).len(),
            1,
            "parens in strings stay inert"
        );
    }

    #[test]
    fn doc_blank_struct_enum_exempt() {
        let fields = vec![test_file(
            "src/a.rs",
            &[
                "struct OrderState {",
                "    /// Current depth.",
                "    depth: i32,",
                "    /// Seen flag.",
                "    seen_code: bool,",
                "}",
            ],
        )];
        assert!(doc_blank_hits(&fields).is_empty(), "struct fields exempt");
        let variants = vec![test_file(
            "src/a.rs",
            &[
                "enum Kind {",
                "    /// First variant.",
                "    First,",
                "    /// Second variant.",
                "    Second,",
                "}",
            ],
        )];
        assert!(doc_blank_hits(&variants).is_empty(), "enum variants exempt");
        let block_open = vec![test_file(
            "src/a.rs",
            &["impl Foo {", "    /// Method docs.", "    fn f() {}", "}"],
        )];
        assert!(
            doc_blank_hits(&block_open).is_empty(),
            "block openers stay silent"
        );
        let nested_variant = vec![test_file(
            "src/a.rs",
            &[
                "pub enum Event {",
                "    Start {",
                "        /// Album to analyze.",
                "        album_id: AlbumId,",
                "        /// Track file paths.",
                "        tracks: Vec<PathBuf>,",
                "    },",
                "}",
            ],
        )];
        assert!(
            doc_blank_hits(&nested_variant).is_empty(),
            "struct-variant fields exempt"
        );
        let split_header = vec![test_file(
            "src/a.rs",
            &[
                "pub struct SortListBuilder<T: SortListBounds, F, U>",
                "where",
                "    T::Criteria: 'static,",
                "{",
                "    /// The `ListBox` being built.",
                "    list_box: ListBox,",
                "    /// Tracks the current order.",
                "    order_map: u32,",
                "}",
            ],
        )];
        assert!(
            doc_blank_hits(&split_header).is_empty(),
            "where-clause lifetimes keep the header pending"
        );
        let method = vec![test_file(
            "src/a.rs",
            &[
                "impl Foo {",
                "    fn g() {",
                "    }",
                "    /// Method docs.",
                "    fn f() {}",
                "}",
            ],
        )];
        assert_eq!(doc_blank_hits(&method).len(), 1, "impl docs still flag");
    }
}
