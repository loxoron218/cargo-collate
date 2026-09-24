//! File-scope name collection for conflict checks.
//!
//! Gathers item names introduced by declarations so aliases that would
//! collide without renaming stay allowed.

use std::collections::BTreeSet;

use crate::lexer::{literal::strip_strings, marker::cut_line_comment};

/// Item keywords introducing a file-scope name.
///
/// Complete item list from the Rust Reference
/// (`doc.rust-lang.org/reference/items.html`): `fn`, `struct`, `enum`,
/// `union`, `trait`, `type`, `const`, `static`, `mod`, `macro` (macro 2.0),
/// `extern crate`, and `use` (redundant with import binders but harmless — any
/// collision still keeps the alias). `impl` stays out since it implements an
/// existing item rather than introducing a shadowing name; `macro_rules!` is
/// collected separately below.
const ITEM_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "union", "trait", "type", "const", "static", "mod", "macro", "extern",
    "use",
];

/// File-scope item names from string-stripped lines.
#[must_use]
pub fn collect_locals(lines: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let stripped = strip_strings(line);
        let code = cut_line_comment(&stripped);
        collect_line_locals(code, &mut out);
        collect_macro_rules(code, &mut out);
    }
    out
}

/// Push item names for one stripped line for plain keywords.
fn collect_line_locals(code: &str, out: &mut BTreeSet<String>) {
    for keyword in ITEM_KEYWORDS {
        push_names_for(code, keyword, false, out);
    }
}

/// Push `macro_rules!` names for one stripped line.
fn collect_macro_rules(code: &str, out: &mut BTreeSet<String>) {
    push_names_for(code, "macro_rules", true, out);
}

/// Push names following `keyword` in `code`.
fn push_names_for(code: &str, keyword: &str, needs_bang: bool, out: &mut BTreeSet<String>) {
    for (pos, _) in code.match_indices(keyword) {
        if let Some(name) = name_at_keyword(code, pos, keyword.len(), needs_bang) {
            out.extend([name]);
        }
    }
}

/// Name after a keyword at `pos`, if well-formed.
fn name_at_keyword(code: &str, pos: usize, len: usize, needs_bang: bool) -> Option<String> {
    if !prev_is_gap(code, pos) {
        return None;
    }
    let mut next = pos.saturating_add(len);
    if code
        .as_bytes()
        .get(next)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return None;
    }
    next = skip_whitespace(code, next);
    if needs_bang {
        next = bang_end(code, next)?;
    }
    ident_at(code, next)
}

/// Index after `!` plus whitespace, if present.
fn bang_end(code: &str, next: usize) -> Option<usize> {
    if code.as_bytes().get(next) != Some(&b'!') {
        return None;
    }
    Some(skip_whitespace(code, next.saturating_add(1)))
}

/// True if the byte before `pos` ends any identifier run.
#[must_use]
pub fn prev_is_gap(code: &str, pos: usize) -> bool {
    pos == 0
        || code
            .as_bytes()
            .get(pos.saturating_sub(1))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// Byte index after ASCII whitespace from `next`.
pub fn skip_whitespace(code: &str, mut next: usize) -> usize {
    while code
        .as_bytes()
        .get(next)
        .is_some_and(u8::is_ascii_whitespace)
    {
        next = next.saturating_add(1);
    }
    next
}

/// Identifier at byte `next`, with raw prefix stripped, if any.
pub fn ident_at(code: &str, mut next: usize) -> Option<String> {
    if code.get(next..next.saturating_add(2)) == Some("r#") {
        next = next.saturating_add(2);
    }
    if code
        .as_bytes()
        .get(next)
        .is_none_or(|b| !(b.is_ascii_alphabetic() || *b == b'_'))
    {
        return None;
    }
    let start = next;
    next = next.saturating_add(1);
    while code
        .as_bytes()
        .get(next)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        next = next.saturating_add(1);
    }
    code.get(start..next).map(str::to_owned)
}
