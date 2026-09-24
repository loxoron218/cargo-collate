//! Doc and test-marker scans above `fn` items.

use crate::{
    lexer::{literal::strip_strings, marker::is_doc_line, word::has_word},
    rules_simple::is_real_cfg_line,
    rules_tests::{range::code_of, test_docs::attached_above},
};

/// Attachment scan above an `fn` line.
#[derive(Debug, Clone, Copy)]
pub struct AboveFn {
    /// True when a `///` or `#[doc]` line attaches to the item.
    pub documented: bool,
    /// True when a test marker attaches to the item.
    pub tested: bool,
}

/// True if stripped `code` is an attribute line.
fn is_attr_code(code: &str) -> bool {
    code.trim_start().starts_with('#')
}

/// True if stripped `code` is a `#[doc]` attribute counting as documentation.
fn is_doc_attr(code: &str) -> bool {
    code.trim_start().starts_with("#[doc")
}

/// True if the attribute at `line` marks test code.
///
/// Covers `#[test]`, runner variants such as `#[tokio::test]` (via `::test`),
/// `#[rstest]` / `#[case]` / `#[test_case]` / `#[wasm_bindgen_test]` /
/// `#[parametrized]`, `#[bench]`, and exact `#[cfg(test)]` markers;
/// lookalikes such as `not(test)` stay excluded.
#[must_use]
pub fn is_test_attr(line: &str, code: &str) -> bool {
    if !is_attr_code(code) {
        return false;
    }
    if code.contains("#[test")
        || code.contains("::test")
        || has_word(code, "bench")
        || has_word(code, "rstest")
        || has_word(code, "case")
        || has_word(code, "test_case")
        || has_word(code, "wasm_bindgen_test")
        || has_word(code, "parametrized")
    {
        return true;
    }
    is_real_cfg_line(line)
}

/// Fold one attached line into `out`; false ends the scan early.
///
/// Doc lines and `#[doc]` attributes mark documentation, other attributes
/// accumulate test markers, and plain comments stop the scan.
fn step_above(out: &mut AboveFn, line: &str) -> bool {
    if is_doc_line(&strip_strings(line)) {
        out.documented = true;
        return true;
    }
    let code = code_of(line);
    if is_doc_attr(&code) {
        out.documented = true;
        return true;
    }
    if !is_attr_code(&code) {
        return false;
    }
    out.tested = out.tested || is_test_attr(line, &code);
    true
}

/// Docs and test markers directly above the `fn` at `idx`.
///
/// A blank line, a plain comment, or any other code line ends the scan before
/// reaching the docs.
#[must_use]
pub fn scan_above(lines: &[String], idx: usize) -> AboveFn {
    let mut out = AboveFn {
        documented: false,
        tested: false,
    };
    for (_, line) in attached_above(lines, idx) {
        if !step_above(&mut out, line) {
            break;
        }
    }
    out
}
