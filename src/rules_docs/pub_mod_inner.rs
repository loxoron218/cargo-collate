//! Public modules with leading inner `//!` docs.
//!
//! A `pub mod` item documents with outer `///` above its attributes; leading
//! `//!` inside reads as parent docs. Any bare-`pub` inline module whose first
//! substantive body line is `//!` flags its doc line. File modules, private
//! modules, and scoped `pub(...)` modules stay silent, as do bodies opening
//! with code.

use crate::{
    lexer::visibility::is_bare_pub_at,
    rules_tests::{
        order::{is_inline_mod, match_mod_head, strip_attrs},
        range::code_of,
    },
    scan::SourceFile,
};

/// True if stripped `line` is a full-line `//!` inner-doc line.
///
/// Leading whitespace stays allowed; string-blanked text keeps `//!` inside
/// literals silent.
fn is_inner_doc_line(stripped: &str) -> bool {
    stripped.trim_start().starts_with("//!")
}

/// True if `code` opens with bare `pub`.
///
/// Scoped `pub(...)` counts as private, matching `is_pub_sig` handling.
fn is_bare_pub_head(code: &str) -> bool {
    code.starts_with("pub") && is_bare_pub_at(code, 0)
}

/// Open index when `code` holds both `{` and `;`, if the opener wins.
///
/// Returns `Some` for inline bodies and `None` for file references, mirroring
/// `order_kind` handling without nesting the caller.
fn both_open_idx(code: &str, idx: usize, off: usize) -> Option<usize> {
    (code.find('{') < code.find(';')).then(|| idx.saturating_add(off).saturating_add(1))
}

/// Index of the body opener for the module at `idx`, if any.
///
/// Same-line `{` wins; otherwise up to three following lines resolve like
/// `kind_from_following`, skipping blanks and attribute-only lines.
fn body_open_idx(lines: &[String], idx: usize, rest: &str) -> Option<usize> {
    if rest.contains('{') {
        return Some(idx);
    }
    if rest.contains(';') {
        return None;
    }
    for (off, line) in lines.iter().skip(idx.saturating_add(1)).take(3).enumerate() {
        let code = code_of(line);
        if code.trim().is_empty() || code.trim_start().starts_with('#') {
            continue;
        }
        if code.contains('{') && !code.contains(';') {
            return Some(idx.saturating_add(off).saturating_add(1));
        }
        if code.contains('{') && code.contains(';') {
            return both_open_idx(&code, idx, off);
        }
        if code.contains(';') {
            return None;
        }
        break;
    }
    None
}

/// True if the opener line holds `//!` after `{`.
fn same_line_docs(file: &SourceFile, open: usize) -> bool {
    file.stripped.get(open).is_some_and(|line| {
        line.find('{').is_some_and(|brace| {
            line.get(brace.saturating_add(1)..)
                .unwrap_or("")
                .trim_start()
                .starts_with("//!")
        })
    })
}

/// True if the opener line holds non-blank text after `{` besides `//!`.
fn same_line_blocks(file: &SourceFile, open: usize) -> bool {
    file.stripped.get(open).is_some_and(|line| {
        line.find('{').is_some_and(|brace| {
            let after = line
                .get(brace.saturating_add(1)..)
                .unwrap_or("")
                .trim_start();
            !after.trim().is_empty() && !after.starts_with("//!")
        })
    })
}

/// Doc-line index of leading `//!` inside the module opening at `open`, if any.
///
/// Only blank lines sit between the opener and the doc; any code, attribute,
/// or other comment ends the search with no hit. Block-comment markers stay
/// silent like the missing-docs driver.
fn leading_doc_idx(file: &SourceFile, open: usize) -> Option<usize> {
    if same_line_docs(file, open) {
        return Some(open);
    }
    if same_line_blocks(file, open) {
        return None;
    }
    let mut next = open.saturating_add(1);
    while let Some(stripped) = file.stripped.get(next) {
        if stripped.trim().is_empty() {
            next = next.saturating_add(1);
            continue;
        }
        let code = code_of(stripped);
        if code.contains("/*") || code.contains("*/") {
            return None;
        }
        if is_inner_doc_line(stripped) {
            return Some(next);
        }
        return None;
    }
    None
}

/// Findings for one file with leading `//!` inside `pub mod` bodies.
fn check_file_pub_mod_inner(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    for (idx, stripped) in file.stripped.iter().enumerate() {
        let code = code_of(stripped);
        if code.contains("/*") || code.contains("*/") {
            continue;
        }
        let head = strip_attrs(&code);
        let Some((_, rest)) = match_mod_head(&head) else {
            continue;
        };
        if !is_bare_pub_head(&head) {
            continue;
        }
        if !is_inline_mod(&file.stripped, idx, rest) {
            continue;
        }
        let Some(open) = body_open_idx(&file.stripped, idx, rest) else {
            continue;
        };
        if let Some(doc) = leading_doc_idx(file, open)
            && let Some(line) = file.lines.get(doc)
        {
            out.push(format!("{}:{}:{line}", file.path, doc.saturating_add(1)));
        }
    }
    out
}

/// Public inline modules with leading inner `//!` docs.
///
/// Every bare-`pub` inline `pub mod` whose first substantive body line is
/// `//!` flags that doc line, even when outer `///` docs also exist.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn pub_mod_inner_docs(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_pub_mod_inner).collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_docs::pub_mod_inner::pub_mod_inner_docs, scan::support::test_file};

    #[test]
    fn inline_pub_inner_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["pub mod tests {", "//! Unit tests.", "}"],
        )];
        let hits = pub_mod_inner_docs(&files);
        assert_eq!(hits.len(), 1, "inner doc flags");
        assert!(
            hits.first().is_some_and(|h| h.contains(":2:")),
            "doc line flagged"
        );
    }

    #[test]
    fn outer_only_silent_mixed_flags() {
        let clean = vec![test_file(
            "src/a.rs",
            &["/// Unit tests.", "#[cfg(test)]", "pub mod tests {", "}"],
        )];
        assert!(
            pub_mod_inner_docs(&clean).is_empty(),
            "outer docs stay silent"
        );
        let mixed = vec![test_file(
            "src/b.rs",
            &["/// Unit tests.", "pub mod tests {", "//! Inner copy.", "}"],
        )];
        assert_eq!(
            pub_mod_inner_docs(&mixed).len(),
            1,
            "mixed styles still flag"
        );
    }

    #[test]
    fn visibility_shapes() {
        let private = vec![test_file("src/a.rs", &["mod tests {", "//! Inner.", "}"])];
        assert!(
            pub_mod_inner_docs(&private).is_empty(),
            "private stays silent"
        );
        let scoped = vec![test_file(
            "src/b.rs",
            &["pub(crate) mod tests {", "//! Inner.", "}"],
        )];
        assert!(
            pub_mod_inner_docs(&scoped).is_empty(),
            "scoped stays silent"
        );
        let file_mod = vec![test_file("src/c.rs", &["pub mod tests;", "//! Orphan."])];
        assert!(
            pub_mod_inner_docs(&file_mod).is_empty(),
            "file modules stay silent"
        );
    }

    #[test]
    fn body_shapes() {
        let code_first = vec![test_file(
            "src/a.rs",
            &["pub mod foo {", "fn helper() {}", "//! Late.", "}"],
        )];
        assert!(
            pub_mod_inner_docs(&code_first).is_empty(),
            "code first stays silent"
        );
        let blank_gap = vec![test_file(
            "src/b.rs",
            &["pub mod foo {", "", "//! After blank.", "}"],
        )];
        assert_eq!(
            pub_mod_inner_docs(&blank_gap).len(),
            1,
            "blank gap still flags"
        );
        let outer_first = vec![test_file(
            "src/c.rs",
            &["pub mod foo {", "/// Item docs.", "fn f() {}", "}"],
        )];
        assert!(
            pub_mod_inner_docs(&outer_first).is_empty(),
            "inner /// stays silent"
        );
        let plain_first = vec![test_file(
            "src/d.rs",
            &["pub mod foo {", "// Plain.", "//! Late.", "}"],
        )];
        assert!(
            pub_mod_inner_docs(&plain_first).is_empty(),
            "plain comments end the search"
        );
    }

    #[test]
    fn header_and_literal_shapes() {
        let multiline = vec![test_file(
            "src/a.rs",
            &["pub mod foo", "{", "//! Inner.", "}"],
        )];
        assert_eq!(
            pub_mod_inner_docs(&multiline).len(),
            1,
            "multiline opener flags"
        );
        let gated = vec![test_file(
            "src/b.rs",
            &[
                "/// Unit tests.",
                "#[cfg(test)]",
                "pub mod tests {",
                "//! Inner.",
                "}",
            ],
        )];
        assert_eq!(pub_mod_inner_docs(&gated).len(), 1, "gated flags");
        let same_line = vec![test_file("src/c.rs", &["pub mod foo { //! Inner.", "}"])];
        assert_eq!(
            pub_mod_inner_docs(&same_line).len(),
            1,
            "same-line docs flag"
        );
        let same_code = vec![test_file("src/d.rs", &["pub mod foo { fn f() {} }"])];
        assert!(
            pub_mod_inner_docs(&same_code).is_empty(),
            "same-line code stays silent"
        );
        let literal = vec![test_file(
            "src/e.rs",
            &[
                "let s = \"pub mod foo {\";",
                "pub mod bar {",
                "fn f() {}",
                "}",
            ],
        )];
        assert!(
            pub_mod_inner_docs(&literal).is_empty(),
            "string contents stay silent"
        );
        let blocked = vec![test_file(
            "src/f.rs",
            &["/* pub mod foo {", "//! Inner.", "} */"],
        )];
        assert!(
            pub_mod_inner_docs(&blocked).is_empty(),
            "block markers stay silent"
        );
    }
}
