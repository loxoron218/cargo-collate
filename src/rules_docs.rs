//! Unnecessary doc and attribute detection.
//!
//! Parent index for the capability. Submodules hold the detectors while this
//! file keeps the shared signature helpers plus the detector tests.

pub mod item_args;
pub mod keywords;
pub mod missing_docs;
pub mod must_use;
pub mod sections;

use std::collections::BTreeSet;

use crate::{
    lexer::{
        literal::strip_strings,
        marker::cut_line_comment,
        visibility::is_bare_pub_at,
        word::{declared_fn_name, has_word},
    },
    rules_alias::collect::{collect_imports, collect_use_stmts},
    rules_tests::range::brace_delta,
};

/// Code of `line` with strings blanked and the line tail cut.
fn code_of(line: &str) -> String {
    cut_line_comment(&strip_strings(line)).to_owned()
}

/// True if `trimmed` is a doc, attribute, or blank filler line.
fn is_filler(trimmed: &str) -> bool {
    trimmed.is_empty()
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with('#')
}

/// Joined signature starting after `idx`, up to `limit` lines.
fn joined_signature(lines: &[String], idx: usize, limit: usize) -> String {
    let mut out = String::new();
    for line in lines.iter().skip(idx).take(limit) {
        let code = code_of(line);
        if is_filler(code.trim()) {
            continue;
        }
        out.push_str(&code);
        out.push(' ');
        if code.contains('{') || code.contains(';') {
            break;
        }
    }
    out
}

/// True if `sig` declares a public item.
///
/// Scoped visibility counts as private since `is_exported()` is false for it.
fn is_pub_sig(sig: &str) -> bool {
    sig.match_indices("pub")
        .map(|(i, _)| i)
        .any(|i| is_bare_pub_at(sig, i))
}

/// Return-type fragment of `sig` after `->`, if any.
///
/// Cuts the body opener and trailing semicolons so only the type remains.
fn return_tail(sig: &str) -> Option<String> {
    let arrow = sig.find("->")?;
    let mut tail = sig
        .get(arrow.saturating_add(2)..)
        .unwrap_or("")
        .trim_start()
        .to_owned();
    if let Some(end) = tail.find(['{', ';']) {
        tail.truncate(end);
    }
    Some(tail)
}

/// Top-level `fn` names declared in `lines`.
fn file_fn_names(lines: &[String]) -> BTreeSet<String> {
    lines
        .iter()
        .scan(0_i32, |depth, line| {
            let name = (*depth <= 0)
                .then_some(declared_fn_name(&code_of(line)))
                .flatten();
            *depth = depth.saturating_add(brace_delta(line));
            Some(name)
        })
        .flatten()
        .collect()
}

/// Usable imported names across `lines`.
fn imported_binders(lines: &[String]) -> BTreeSet<String> {
    let (binders, _) = collect_imports(&collect_use_stmts(lines));
    binders
}

/// True if `sig` returns unit or never, or has no return arrow.
fn sig_returns_unit(sig: &str) -> bool {
    let Some(tail) = return_tail(sig) else {
        return true;
    };
    let tail = tail.trim_start();
    tail.is_empty() || tail.starts_with("()") || tail.starts_with('!')
}

/// Updated depth and started flag after one brace character.
const fn step_brace(depth: i32, started: bool, c: char) -> (i32, bool) {
    if c == '{' {
        (depth.saturating_add(1), true)
    } else if c == '}' {
        (depth.saturating_sub(1), started)
    } else {
        (depth, started)
    }
}

/// Body text of the `fn` starting at `start`, brace-balanced.
fn fn_body(lines: &[String], start: usize) -> String {
    let mut body = String::new();
    let mut depth: i32 = 0;
    let mut started = false;
    for line in lines.iter().skip(start) {
        let code = code_of(line);
        for c in code.chars() {
            (depth, started) = step_brace(depth, started, c);
        }
        if started {
            body.push_str(&code);
            body.push('\n');
        }
        if started && depth <= 0 {
            break;
        }
    }
    body
}

/// Index of the `fn` line at or after `idx`, if any.
fn fn_index(lines: &[String], idx: usize) -> Option<usize> {
    for (off, line) in lines.iter().skip(idx).enumerate().take(20) {
        let code = code_of(line);
        if is_filler(code.trim()) {
            continue;
        }
        if has_word(&code, "fn") {
            return Some(idx.saturating_add(off));
        }
    }
    None
}

/// True if stripped `code` declares a trait rather than an impl or bound.
fn is_trait_decl(code: &str) -> bool {
    has_word(code, "trait") && !has_word(code, "impl")
}

/// End line (exclusive) of the brace block starting at `start`, if any.
///
/// Multiline headers accumulate until `{`; a `;` first (as in trait aliases)
/// means no block.
fn block_end(lines: &[String], start: usize) -> Option<usize> {
    if !joined_signature(lines, start, 12).contains('{') {
        return None;
    }
    let mut depth: i32 = 0;
    let mut opened = false;
    let mut k = start;
    while let Some(line) = lines.get(k) {
        let delta = brace_delta(line);
        opened = opened || delta > 0;
        depth = depth.saturating_add(delta);
        k = k.saturating_add(1);
        if opened && depth <= 0 {
            break;
        }
    }
    Some(k)
}

/// True if stripped `code` opens a public trait block.
fn is_pub_trait_head(code: &str) -> bool {
    is_trait_decl(code) && has_word(code, "pub")
}

/// Top-level block ranges whose header line satisfies `is_head`.
///
/// Multiline headers resolve through `block_end`; nested items stay excluded
/// since they are never exported.
fn header_ranges(lines: &[String], is_head: fn(&str) -> bool) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut depth: i32 = 0;
    let mut idx = 0;
    while idx < lines.len() {
        let Some(line) = lines.get(idx) else {
            break;
        };
        let code = code_of(line);
        if depth > 0 || !is_head(&code) {
            depth = depth.saturating_add(brace_delta(line));
            idx = idx.saturating_add(1);
            continue;
        }
        let Some(end) = block_end(lines, idx) else {
            depth = depth.saturating_add(brace_delta(line));
            idx = idx.saturating_add(1);
            continue;
        };
        ranges.push((idx, end));
        idx = end;
    }
    ranges
}

/// Public top-level trait ranges as zero-based start/end spans.
///
/// Nested items stay excluded since they are never exported.
fn pub_trait_ranges(lines: &[String]) -> Vec<(usize, usize)> {
    header_ranges(lines, is_pub_trait_head)
}

/// True if the `fn` after `idx` sits inside a public trait block.
///
/// Trait methods carry no `pub` keyword; visibility is inherited.
fn is_pub_trait_fn(lines: &[String], idx: usize) -> bool {
    fn_index(lines, idx.saturating_add(1)).is_some_and(|pos| {
        pub_trait_ranges(lines)
            .iter()
            .any(|(a, b)| *a <= pos && pos < *b)
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_docs::{
            must_use::unnecessary_must_use,
            sections::{unnecessary_errors, unnecessary_panics},
        },
        scan::support::test_file,
    };

    #[test]
    fn must_use_private_flagged() {
        let files = vec![
            test_file("src/a.rs", &["#[must_use]", "fn helper() -> bool { true }"]),
            test_file("src/b.rs", &["#[must_use]", "pub fn ok() -> bool { true }"]),
        ];
        let hits = unnecessary_must_use(&files);
        assert_eq!(hits.len(), 1, "only private stays flagged");
        assert!(
            hits.first().is_some_and(|h| h.starts_with("src/a.rs")),
            "private file flagged"
        );
    }

    #[test]
    fn must_use_unit_flagged() {
        let files = vec![
            test_file("src/a.rs", &["#[must_use]", "pub fn noop() {}"]),
            test_file("src/b.rs", &[r#"#[must_use = "x"]"#, "fn h() -> i32 { 1 }"]),
            test_file(
                "src/c.rs",
                &["let s = \"#[must_use]\";", "fn f() -> bool { true }"],
            ),
        ];
        assert_eq!(
            unnecessary_must_use(&files).len(),
            2,
            "unit and private flag"
        );
    }

    #[test]
    fn must_use_multiline_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "#[must_use]",
                "pub fn f(",
                "x: i32,",
                ") -> bool {",
                "true",
                "}",
            ],
        )];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "multiline stays silent"
        );
    }

    #[test]
    fn panics_test_flagged() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["/// Docs.", "/// # Panics", "#[test]", "fn t() {}"],
            ),
            test_file(
                "src/b.rs",
                &["/// Docs.", "/// # Panics", "pub fn f() { panic!(\"x\"); }"],
            ),
        ];
        let hits = unnecessary_panics(&files);
        assert_eq!(hits.len(), 1, "only test stays flagged");
        assert!(
            hits.first().is_some_and(|h| h.starts_with("src/a.rs")),
            "test file flagged"
        );
    }

    #[test]
    fn panics_prose_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["/// Mentions `# Panics` inline.", "pub fn f() -> i32 { 1 }"],
            ),
            test_file(
                "src/b.rs",
                &["/// D.", "/// # Panics", "pub fn pure() -> i32 { 1 }"],
            ),
        ];
        assert_eq!(unnecessary_panics(&files).len(), 1, "only header flags");
    }

    #[test]
    fn errors_result_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["/// D.", "/// # Errors", "fn f() -> i32 { 1 }"],
            ),
            test_file(
                "src/b.rs",
                &[
                    "/// D.",
                    "/// # Errors",
                    "pub fn f() -> Result<i32> { Ok(1) }",
                ],
            ),
        ];
        assert_eq!(unnecessary_errors(&files).len(), 1, "non-Result flags");
    }

    #[test]
    fn trait_errors_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "pub trait Api {",
                "/// D.",
                "/// # Errors",
                "fn f() -> Result<i32, String> { Ok(1) }",
                "}",
            ],
        )];
        assert!(
            unnecessary_errors(&files).is_empty(),
            "public trait methods stay silent"
        );
        let private = vec![test_file(
            "src/b.rs",
            &[
                "trait Api {",
                "/// D.",
                "/// # Errors",
                "fn f() -> i32 { 1 }",
                "}",
            ],
        )];
        assert_eq!(
            unnecessary_errors(&private).len(),
            1,
            "private trait methods still flag"
        );
    }

    #[test]
    fn trait_panics_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "pub trait Api {",
                "/// D.",
                "/// # Panics",
                "fn f() { panic!(\"x\"); }",
                "}",
            ],
        )];
        assert!(
            unnecessary_panics(&files).is_empty(),
            "panicking trait methods stay silent"
        );
    }
}
