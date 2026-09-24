//! Missing `///` documentation on functions including private items.
//!
//! Clippy's `missing_docs_in_private_items` cannot exempt test code without
//! `allow` attributes, which this project forbids, so this syntactic detector
//! requires an attached `///` line on every `fn` outside exempt code instead.
//!
//! Parent index for the detector. The attachment submodule holds the doc
//! scan, the doc-header submodule holds the `//!` file check, the std-methods
//! submodule holds the exempt method list, while this file keeps the driver
//! plus its tests.

pub mod attachment;
pub mod doc_header;
pub mod std_methods;

use std::collections::HashSet;

use crate::{
    gating::{gated_stems, is_stem_gated},
    lexer::word::declared_fn_name,
    rules_docs::missing_docs::{
        attachment::{is_test_attr, scan_above},
        std_methods::is_std_method,
    },
    rules_tests::range::{code_of, in_ranges, stray_gap, test_ranges},
    scan::{SourceFile, indexed_lines},
};

/// True if `path` lives under an integration-test tree.
fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/") || path.contains("/tests/")
}

/// True if `file` holds only test code.
///
/// Integration-test trees and parent-gated file modules stay wholly exempt;
/// inline test modules filter per item instead.
fn is_test_file(file: &SourceFile, gated: &HashSet<String>) -> bool {
    is_test_path(&file.path) || is_stem_gated(file, gated)
}

/// True if the `fn` at `idx` needs docs but has none attached.
///
/// Test modules, test and bench attributes, the `main` entry, standard-trait
/// methods, and lines holding block-comment markers stay exempt. Trait-impl
/// methods and macro code need docs like any other item.
fn is_undocumented_fn(lines: &[String], exempt: &[(usize, usize)], idx: usize, line: &str) -> bool {
    let code = code_of(line);
    if code.contains("/*") || code.contains("*/") {
        return false;
    }
    let Some(name) = declared_fn_name(&code) else {
        return false;
    };
    if name == "main" || is_std_method(&name) {
        return false;
    }
    if in_ranges(exempt, idx) {
        return false;
    }
    if is_test_attr(line, &code) {
        return false;
    }
    let above = scan_above(lines, idx);
    if above.tested {
        return false;
    }
    !above.documented
}

/// Findings for one file outside integration-test trees.
fn file_missing_docs(file: &SourceFile) -> Vec<String> {
    let exempt = test_ranges(&file.lines, stray_gap);
    indexed_lines(file)
        .filter(|(idx, (_, code))| is_undocumented_fn(&file.stripped, &exempt, *idx, code))
        .map(|(idx, (line, _))| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        .collect()
}

/// Functions without attached `///` docs outside exempt code.
///
/// Every free function, associated function, and method needs documentation,
/// whether public or private, including trait-impl methods and macro code.
/// Inline test modules, integration tests, test and bench attributes, the
/// `main` entry, and standard-trait methods stay exempt.
pub fn missing_fn_docs(files: &[SourceFile]) -> Vec<String> {
    let gated = gated_stems(files);
    files
        .iter()
        .filter(|file| !is_test_file(file, &gated))
        .flat_map(file_missing_docs)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_docs::missing_docs::missing_fn_docs, scan::support::test_file};

    #[test]
    fn private_fn_flagged() {
        let files = vec![
            test_file("src/a.rs", &["fn helper() {}"]),
            test_file("src/b.rs", &["/// Helper.", "fn helper() {}"]),
        ];
        let hits = missing_fn_docs(&files);
        assert_eq!(hits.len(), 1, "only undocumented stays flagged");
        assert!(
            hits.first().is_some_and(|h| h.starts_with("src/a.rs")),
            "bare file flagged"
        );
    }

    #[test]
    fn pub_and_method_flagged() {
        let files = vec![
            test_file("src/a.rs", &["pub fn api() {}"]),
            test_file(
                "src/b.rs",
                &["struct S;", "impl S {", "fn build() -> S { S }", "}"],
            ),
            test_file(
                "src/c.rs",
                &["/// Makes beans.", "pub fn beans() -> i32 { 1 }"],
            ),
        ];
        let hits = missing_fn_docs(&files);
        assert_eq!(hits.len(), 2, "public and inherent methods flag");
    }

    #[test]
    fn trait_impl_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "trait Api {",
                "fn name(&self) -> i32;",
                "}",
                "struct S;",
                "impl Api for S {",
                "fn name(&self) -> i32 { 1 }",
                "}",
            ],
        )];
        let hits = missing_fn_docs(&files);
        assert_eq!(hits.len(), 2, "declaration and impl method flag");
        assert!(hits.iter().any(|h| h.contains(":2:")), "trait line flagged");
        assert!(hits.iter().any(|h| h.contains(":6:")), "impl line flagged");
    }

    #[test]
    fn std_methods_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "impl Display for S {",
                "fn fmt(&self, f: &mut Formatter<'_>) -> Result { Ok(()) }",
                "}",
                "impl Default for S {",
                "fn default() -> Self { S }",
                "}",
                "impl Clone for S {",
                "fn clone(&self) -> Self { S }",
                "fn clone_from(&mut self, source: &Self) {}",
                "}",
            ],
        )];
        assert!(
            missing_fn_docs(&files).is_empty(),
            "std methods stay silent"
        );
        let lookalikes = vec![test_file(
            "src/b.rs",
            &["fn formatter() {}", "fn defaulter() {}"],
        )];
        assert_eq!(
            missing_fn_docs(&lookalikes).len(),
            2,
            "nearby names still flag"
        );
    }

    #[test]
    fn tested_code_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["#[cfg(test)]", "mod tests {", "fn helper() {}", "}"],
            ),
            test_file("src/b.rs", &["#[test]", "fn t() {}"]),
            test_file("src/c.rs", &["fn main() {}"]),
            test_file("tests/x.rs", &["fn helper() {}"]),
            test_file("src/y.rs", &["#[cfg(test)]", "mod cases;"]),
            test_file("src/y/cases.rs", &["fn helper() {}"]),
        ];
        assert!(missing_fn_docs(&files).is_empty(), "test code stays silent");
        let gated = vec![
            test_file("src/d.rs", &["#[cfg(test)]", "mod helper;"]),
            test_file("src/helper.rs", &["fn helper() {}"]),
        ];
        assert!(
            missing_fn_docs(&gated).is_empty(),
            "parent-gated files stay silent"
        );
    }

    #[test]
    fn attachment_shapes() {
        let documented = vec![test_file(
            "src/a.rs",
            &["/// Builds.", "#[inline]", "fn build() -> i32 { 1 }"],
        )];
        assert!(
            missing_fn_docs(&documented).is_empty(),
            "attrs between docs and fn stay silent"
        );
        let detached = vec![test_file(
            "src/b.rs",
            &["/// Builds.", "", "fn build() -> i32 { 1 }"],
        )];
        assert_eq!(
            missing_fn_docs(&detached).len(),
            1,
            "blank line detaches docs"
        );
        let literal = vec![test_file(
            "src/c.rs",
            &["let s = \"fn fake() {}\";", "fn real() {}"],
        )];
        let hits = missing_fn_docs(&literal);
        assert_eq!(hits.len(), 1, "string contents stay silent");
        assert!(
            hits.first().is_some_and(|h| h.contains(":2:")),
            "real fn flagged"
        );
    }

    #[test]
    fn fn_call_names_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "/// Runs the scan.",
                "fn run(lines: &[String]) -> bool {",
                "    let body = fn_body(lines, 0);",
                "    let pos = fn_index(lines, 1);",
                "    body.is_empty() && pos.is_none()",
                "}",
            ],
        )];
        assert!(
            missing_fn_docs(&files).is_empty(),
            "fn-prefixed calls stay silent"
        );
    }

    #[test]
    fn nested_impl_shapes() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "mod imp {",
                "struct S;",
                "impl Api for S {",
                "fn m(&self) {}",
                "}",
                "impl S {",
                "fn helper() {}",
                "}",
                "}",
            ],
        )];
        let hits = missing_fn_docs(&files);
        assert_eq!(hits.len(), 2, "nested trait and inherent methods flag");
        assert!(
            hits.iter().any(|h| h.contains(":4:")),
            "trait method flagged"
        );
        assert!(
            hits.iter().any(|h| h.contains(":7:")),
            "helper line flagged"
        );
    }

    #[test]
    fn macro_shapes_flagged() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "macro_rules! gen {",
                    "() => {",
                    "impl Api for S {",
                    "fn m(&self) {}",
                    "}",
                    "fn helper() {}",
                    "}",
                    "}",
                ],
            ),
            test_file("src/b.rs", &["delegate!(pub fn generated() -> i32);"]),
        ];
        let hits = missing_fn_docs(&files);
        assert_eq!(hits.len(), 3, "macro definition and call fns flag");
        assert!(
            hits.iter().any(|h| h.starts_with("src/b.rs")),
            "delegate line flagged"
        );
    }

    #[test]
    fn bench_and_inline_test_silent() {
        let files = vec![
            test_file("src/a.rs", &["#[bench]", "fn b(b: &mut Bencher) {}"]),
            test_file("src/b.rs", &["#[test] fn t() {}"]),
            test_file(
                "src/c.rs",
                &["#[doc = \"Builds.\"]", "fn build() -> i32 { 1 }"],
            ),
        ];
        assert!(
            missing_fn_docs(&files).is_empty(),
            "bench, inline test, and doc attrs stay silent"
        );
    }

    #[test]
    fn std_method_family_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "impl ToString for S {",
                "fn to_string(&self) -> String { String::new() }",
                "}",
                "impl IntoIterator for S {",
                "fn into_iter(self) -> impl Iterator { std::iter::empty() }",
                "}",
                "impl PartialOrd for S {",
                "fn partial_cmp(&self, o: &Self) -> Option<Ord> { None }",
                "fn lt(&self, o: &Self) -> bool { false }",
                "fn le(&self, o: &Self) -> bool { false }",
                "fn gt(&self, o: &Self) -> bool { false }",
                "fn ge(&self, o: &Self) -> bool { false }",
                "}",
                "impl BorrowMut for S {",
                "fn borrow_mut(&mut self) -> &mut T { panic!() }",
                "}",
            ],
        )];
        assert!(
            missing_fn_docs(&files).is_empty(),
            "std method family stays silent"
        );
    }
}
