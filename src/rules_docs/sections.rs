//! Unnecessary `# Panics` and `# Errors` doc sections.
//!
//! Private non-test items require these sections when the body can panic or
//! the signature returns `Result`, including aliases such as `StorageResult`;
//! test code stays exempt like `missing_fn_docs`. Missing sections live in
//! `missing_sections`; this file keeps the unnecessary side plus shared
//! helpers.

use std::{collections::HashSet, hash::BuildHasher};

use crate::{
    gating::gated_stems,
    lexer::{literal::strip_strings, word::has_word},
    rules_docs::{
        fn_body, fn_index, is_pub_sig, is_pub_trait_fn, joined_signature,
        missing_docs::{
            attachment::{is_test_attr, scan_above},
            is_test_file,
        },
        return_tail,
    },
    rules_tests::{
        range::{code_of, in_ranges, stray_gap, test_ranges},
        test_docs::attached_above,
    },
    scan::{SourceFile, indexed_lines},
};

/// Doc-section predicate across test-gated files.
type DocPred = fn(&SourceFile, &HashSet<String>, &[(usize, usize)], usize) -> bool;

/// True if `code` is a `///` section header for `marker`.
#[must_use]
pub fn is_section_header(code: &str, marker: &str) -> bool {
    let trimmed = code.trim_start();
    if !trimmed.starts_with("///") || trimmed.starts_with("////") {
        return false;
    }
    let after_slashes = trimmed.get(3..).unwrap_or("").trim_start();
    if !after_slashes.starts_with('#') {
        return false;
    }
    let body = after_slashes.trim_start_matches('#').trim_start();
    let want = marker.trim_start_matches('#').trim_start();
    body.starts_with(want)
        && body.get(want.len()..).is_none_or(|t| {
            t.is_empty() || t.starts_with(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        })
}

/// Attached signature for the doc line at `idx`.
fn attached_sig(lines: &[String], idx: usize) -> String {
    fn_index(lines, idx.saturating_add(1))
        .map_or(String::new(), |pos| joined_signature(lines, pos, 12))
}

/// True if `body` holds a panic-capable marker.
///
/// Complete panic-capable set from `doc.rust-lang.org/nightly/std/all.html`
/// `#macros` (nightly 1.100.0, 2026-09-20): `panic!`, `todo!`,
/// `unimplemented!`, `unreachable!`, `assert!`, `assert_eq!`, `assert_ne!`,
/// `assert_matches!`, `debug_assert!`, `debug_assert_eq!`,
/// `debug_assert_ne!`, `debug_assert_matches!`, plus infallible-unwrap
/// methods `unwrap()`, `expect(`, `unwrap_err`, `expect_err`.
#[must_use]
pub fn body_can_panic(body: &str) -> bool {
    const MARKERS: [&str; 16] = [
        "panic!",
        "todo!",
        "unimplemented!",
        "unreachable!",
        "assert!",
        "assert_eq!",
        "assert_ne!",
        "assert_matches!",
        "debug_assert!",
        "debug_assert_eq!",
        "debug_assert_ne!",
        "debug_assert_matches!",
        "unwrap()",
        "expect(",
        "unwrap_err",
        "expect_err",
    ];
    MARKERS.iter().any(|m| body.contains(m))
}

/// True if the return type of `sig` mentions `Result`.
///
/// Bare `Result` matches by word; generic aliases such as `StorageResult<T>`
/// match via `Result<`, which keeps clippy parity without firing on struct
/// names like `UnsafeScanResult` that merely end in `Result`.
#[must_use]
pub fn returns_result(sig: &str) -> bool {
    return_tail(sig).is_some_and(|tail| has_word(&tail, "Result") || tail.contains("Result<"))
}

/// `fn` signature and body start for the doc at `idx`, if any.
fn fn_at(lines: &[String], idx: usize) -> Option<(String, usize)> {
    let sig = attached_sig(lines, idx);
    if !has_word(&sig, "fn") {
        return None;
    }
    let pos = fn_index(lines, idx.saturating_add(1))?;
    Some((sig, pos))
}

/// True if the documented `fn` is private, inheriting trait visibility.
fn is_private_doc(lines: &[String], idx: usize, sig: &str) -> bool {
    !is_pub_sig(sig) && !is_pub_trait_fn(lines, idx)
}

/// True if the `fn` at `pos` sits in test-exempt code.
#[must_use]
pub fn is_test_fn<S>(
    file: &SourceFile,
    gated: &HashSet<String, S>,
    exempt: &[(usize, usize)],
    pos: usize,
) -> bool
where
    S: BuildHasher,
{
    if is_test_file(file, gated) {
        return true;
    }
    if in_ranges(exempt, pos) {
        return true;
    }
    let Some(line) = file.lines.get(pos) else {
        return false;
    };
    let Some(stripped) = file.stripped.get(pos) else {
        return false;
    };
    if is_test_attr(line, &code_of(stripped)) {
        return true;
    }
    scan_above(&file.stripped, pos).tested
}

/// True if a `marker` section attaches directly above the `fn` at `pos`.
#[must_use]
pub fn has_section(lines: &[String], pos: usize, marker: &str) -> bool {
    attached_above(lines, pos)
        .iter()
        .any(|(_, line)| is_section_header(&strip_strings(line), marker))
}

/// True if `# Panics` or `# Errors` at `idx` is removable warning-free.
///
/// `wants_errors` selects the need check: `Result` return versus
/// panic-capable body; private test items stay unnecessary either way.
fn unnecessary_for(
    file: &SourceFile,
    gated: &HashSet<String>,
    exempt: &[(usize, usize)],
    idx: usize,
    wants_errors: bool,
) -> bool {
    let Some((sig, pos)) = fn_at(&file.stripped, idx) else {
        return true;
    };
    let needed = if wants_errors {
        returns_result(&sig)
    } else {
        body_can_panic(&fn_body(&file.stripped, pos))
    };
    if !needed {
        return true;
    }
    if is_private_doc(&file.stripped, idx, &sig) && is_test_fn(file, gated, exempt, pos) {
        return true;
    }
    false
}

/// True if `# Panics` at `idx` is removable warning-free.
fn panics_at(
    file: &SourceFile,
    gated: &HashSet<String>,
    exempt: &[(usize, usize)],
    idx: usize,
) -> bool {
    unnecessary_for(file, gated, exempt, idx, false)
}

/// True if `# Errors` at `idx` is removable warning-free.
fn errors_at(
    file: &SourceFile,
    gated: &HashSet<String>,
    exempt: &[(usize, usize)],
    idx: usize,
) -> bool {
    unnecessary_for(file, gated, exempt, idx, true)
}

/// Doc-section hits in one file.
fn file_doc_hits(
    file: &SourceFile,
    gated: &HashSet<String>,
    marker: &str,
    pred: DocPred,
) -> Vec<String> {
    let exempt = test_ranges(&file.lines, stray_gap);
    let mut out = Vec::new();
    for (idx, (line, code)) in indexed_lines(file) {
        if !is_section_header(&strip_strings(code), marker) {
            continue;
        }
        if !pred(file, gated, &exempt, idx) {
            continue;
        }
        out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
    }
    out
}

/// Generic doc-section hits for `marker` with `pred`.
fn doc_hits(files: &[SourceFile], marker: &str, pred: DocPred) -> Vec<String> {
    let gated = gated_stems(files);
    files
        .iter()
        .flat_map(|file| file_doc_hits(file, &gated, marker, pred))
        .collect()
}

/// Unnecessary `# Panics` sections that clippy would not require.
pub fn unnecessary_panics(files: &[SourceFile]) -> Vec<String> {
    doc_hits(files, "# Panics", panics_at)
}

/// Unnecessary `# Errors` sections that clippy would not require.
pub fn unnecessary_errors(files: &[SourceFile]) -> Vec<String> {
    doc_hits(files, "# Errors", errors_at)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_docs::sections::{body_can_panic, unnecessary_errors, unnecessary_panics},
        scan::support::test_file,
    };

    #[test]
    fn errors_alias_silent() {
        let alias = [
            "/// D.",
            "/// # Errors",
            "pub fn f() -> StorageResult<i64> { Ok(1) }",
        ];
        let files = vec![test_file("src/a.rs", &alias)];
        assert!(unnecessary_errors(&files).is_empty(), "alias stays silent");
    }

    #[test]
    fn panic_markers_cover_nightly() {
        for marker in [
            "assert_matches!",
            "debug_assert_matches!",
            "debug_assert_eq!",
            "debug_assert_ne!",
        ] {
            assert!(
                body_can_panic(&format!("fn f() {{ {marker} }}")),
                "MARKERS missing nightly panic marker `{marker}`"
            );
        }
        let files = vec![test_file(
            "src/a.rs",
            &[
                "/// D.",
                "/// # Panics",
                "pub fn f(x: Option<i32>) -> i32 { x.unwrap() }",
            ],
        )];
        assert!(
            unnecessary_panics(&files).is_empty(),
            "panicking body stays silent"
        );
    }

    #[test]
    fn private_needed_silent() {
        let errors = vec![test_file(
            "src/a.rs",
            &[
                "/// D.",
                "/// # Errors",
                "fn f() -> Result<i32, String> { Ok(1) }",
            ],
        )];
        assert!(
            unnecessary_errors(&errors).is_empty(),
            "private needing stays silent"
        );
        let panics = vec![test_file(
            "src/b.rs",
            &["/// D.", "/// # Panics", "fn f() { panic!(\"x\"); }"],
        )];
        assert!(
            unnecessary_panics(&panics).is_empty(),
            "private panicking stays silent"
        );
    }
}
