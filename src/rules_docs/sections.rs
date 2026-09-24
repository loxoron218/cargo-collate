//! Unnecessary `# Panics` and `# Errors` doc sections.
//!
//! Private items never require these sections with `check-private-items`
//! unchecked; public items only require them when the body can panic or the
//! signature returns `Result`, including aliases such as `StorageResult`.

use crate::{
    lexer::{literal::strip_strings, word::has_word},
    rules_docs::{fn_body, fn_index, is_pub_sig, is_pub_trait_fn, joined_signature, return_tail},
    scan::{SourceFile, indexed_lines},
};

/// True if `code` is a `///` section header for `marker`.
fn is_section_header(code: &str, marker: &str) -> bool {
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
    joined_signature(lines, idx.saturating_add(1), 20)
}

/// True if `body` holds a panic-capable marker.
///
/// Complete panic-capable set from `doc.rust-lang.org/nightly/std/all.html`
/// `#macros` (nightly 1.100.0, 2026-09-20): `panic!`, `todo!`,
/// `unimplemented!`, `unreachable!`, `assert!`, `assert_eq!`, `assert_ne!`,
/// `assert_matches!`, `debug_assert!`, `debug_assert_eq!`,
/// `debug_assert_ne!`, `debug_assert_matches!`, plus infallible-unwrap
/// methods `unwrap()`, `expect(`, `unwrap_err`, `expect_err`.
fn body_can_panic(body: &str) -> bool {
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

/// Public `fn` signature and body start for the doc at `idx`, if any.
///
/// Methods of public traits count as public: their visibility is inherited.
fn pub_fn_at(lines: &[String], idx: usize) -> Option<(String, usize)> {
    let sig = attached_sig(lines, idx);
    if !has_word(&sig, "fn") {
        return None;
    }
    if !is_pub_sig(&sig) && !is_pub_trait_fn(lines, idx) {
        return None;
    }
    let pos = fn_index(lines, idx.saturating_add(1))?;
    Some((sig, pos))
}

/// True if `# Panics` at `idx` is removable warning-free.
fn panics_at(lines: &[String], idx: usize) -> bool {
    let Some((_, pos)) = pub_fn_at(lines, idx) else {
        return true;
    };
    !body_can_panic(&fn_body(lines, pos))
}

/// True if `# Errors` at `idx` is removable warning-free.
fn errors_at(lines: &[String], idx: usize) -> bool {
    let Some((sig, _)) = pub_fn_at(lines, idx) else {
        return true;
    };
    !returns_result(&sig)
}

/// True if the return type of `sig` mentions `Result`.
///
/// Type aliases such as `StorageResult` resolve to `Result`, which a
/// word-boundary match misses; checking the return tail keeps parity with
/// clippy's `missing_errors_doc`.
fn returns_result(sig: &str) -> bool {
    return_tail(sig).is_some_and(|tail| tail.contains("Result"))
}

/// Generic doc-section hits for `marker` with `pred`.
fn doc_hits(files: &[SourceFile], marker: &str, pred: fn(&[String], usize) -> bool) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file)
                .filter(|(_, (_, code))| is_section_header(&strip_strings(code), marker))
                .filter(|(idx, _)| pred(&file.stripped, *idx))
                .map(|(idx, (line, _))| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        })
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
}
