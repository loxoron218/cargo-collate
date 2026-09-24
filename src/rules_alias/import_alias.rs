//! Alias parsing and original-name resolution.
//!
//! Splits expanded `use` paths on standalone `as` and maps `self`
//! imports onto their parent name.

use std::collections::BTreeSet;

/// Standard library prelude names that collide without an explicit import.
///
/// Aliasing away from these stays allowed since removing the alias would
/// shadow the prelude or trigger `redundant_imports`.
///
/// Reference: `doc.rust-lang.org/nightly/std/all.html` (nightly
/// 1.100.0, 2026-09-20) plus the prelude union `std/prelude::{v1,rust_2021,
/// rust_2024}` and `core/prelude::{v1,rust_2021,rust_2024}`. Union across all
/// editions: `v1` (incl. deprecated `try`, experimental `concat_bytes`,
/// `const_format_args`, `log_syntax`, `pattern_type`, `trace_macros`, `deref`,
/// `type_ascribe` and attribute macros `bench`, `alloc_error_handler`,
/// `cfg_accessible`, `cfg_eval`, `define_opaque`, `derive_const`, `eii`,
/// `eii_declaration`, `test_case`, `unsafe_eii`), 2021 (`TryFrom`, `TryInto`,
/// `FromIterator`), 2024 (`Future`, `IntoFuture`, `AsyncFn*`). `Debug`/`Hash`
/// are derive-only re-exports but still collide by name, so they stay listed.
const PRELUDE: &[&str] = &[
    "AsyncFn",
    "AsyncFnMut",
    "AsyncFnOnce",
    "AsMut",
    "AsRef",
    "Box",
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "Err",
    "ExactSizeIterator",
    "Extend",
    "Fn",
    "FnMut",
    "FnOnce",
    "From",
    "FromIterator",
    "Future",
    "Hash",
    "Into",
    "IntoFuture",
    "IntoIterator",
    "Iterator",
    "None",
    "Ok",
    "Option",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Result",
    "Send",
    "Sized",
    "Some",
    "String",
    "Sync",
    "ToOwned",
    "ToString",
    "TryFrom",
    "TryInto",
    "Unpin",
    "Vec",
    "align_of",
    "align_of_val",
    "alloc_error_handler",
    "assert",
    "assert_eq",
    "assert_ne",
    "bench",
    "cfg",
    "cfg_accessible",
    "cfg_eval",
    "cfg_select",
    "column",
    "compile_error",
    "concat",
    "concat_bytes",
    "const_format_args",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "define_opaque",
    "deref",
    "derive",
    "derive_const",
    "drop",
    "eii",
    "eii_declaration",
    "env",
    "eprint",
    "eprintln",
    "file",
    "format",
    "format_args",
    "global_allocator",
    "include",
    "include_bytes",
    "include_str",
    "is_x86_feature_detected",
    "line",
    "log_syntax",
    "matches",
    "module_path",
    "option_env",
    "panic",
    "pattern_type",
    "print",
    "println",
    "size_of",
    "size_of_val",
    "stringify",
    "test",
    "test_case",
    "thread_local",
    "todo",
    "trace_macros",
    "try",
    "type_ascribe",
    "unimplemented",
    "unreachable",
    "unsafe_eii",
    "vec",
    "write",
    "writeln",
];

/// Split an expanded path into its path part and optional alias.
///
/// Whitespace inside the path (newlines from multiline `use` blocks) is
/// removed so grouped imports still resolve.
#[must_use]
pub fn split_alias(expanded: &str) -> (String, Option<String>) {
    let Some(pos) = alias_pos(expanded) else {
        return (expanded.trim().to_owned(), None);
    };
    let path_raw = expanded.get(..pos).unwrap_or_default();
    let alias_raw = expanded.get(pos.saturating_add(2)..).unwrap_or_default();
    let path: String = path_raw.split_whitespace().collect();
    let alias = alias_raw
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    if path.is_empty() || alias.is_empty() {
        return (expanded.trim().to_owned(), None);
    }
    (path, Some(alias))
}

/// Byte position of a standalone `as` surrounded by whitespace, if any.
fn alias_pos(expanded: &str) -> Option<usize> {
    let bytes = expanded.as_bytes();
    let mut found = None;
    let mut idx: usize = 0;
    while idx.saturating_add(2) <= bytes.len() {
        let is_as =
            bytes.get(idx) == Some(&b'a') && bytes.get(idx.saturating_add(1)) == Some(&b's');
        if is_as && prev_is_ws(bytes, idx) && next_is_ws(bytes, idx.saturating_add(2)) {
            found = Some(idx);
        }
        idx = idx.saturating_add(1);
    }
    found
}

/// True if the byte before `pos` is whitespace or the start.
fn prev_is_ws(bytes: &[u8], pos: usize) -> bool {
    pos == 0
        || bytes
            .get(pos.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
}

/// True if the byte at `pos` is whitespace or the end.
fn next_is_ws(bytes: &[u8], pos: usize) -> bool {
    pos >= bytes.len() || bytes.get(pos).is_some_and(u8::is_ascii_whitespace)
}

/// Strip a raw-identifier prefix, if any.
#[must_use]
pub fn strip_raw(ident: &str) -> &str {
    ident.strip_prefix("r#").unwrap_or(ident)
}

/// Original imported name for a path, with `self` resolving to its parent.
pub fn original_name(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.contains('*') {
        return None;
    }
    let segs: Vec<&str> = trimmed
        .split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let last_raw = segs.last().copied().unwrap_or_default();
    let last = strip_raw(last_raw);
    if last.is_empty() || last == "_" || last == "*" {
        return None;
    }
    if last != "self" {
        return Some(last.to_owned());
    }
    parent_original(&segs)
}

/// Parent name for a `self` import, if usable.
fn parent_original(segs: &[&str]) -> Option<String> {
    if segs.len() < 2 {
        return Some("self".to_owned());
    }
    let parent_raw = segs
        .get(segs.len().saturating_sub(2))
        .copied()
        .unwrap_or_default();
    let parent = strip_raw(parent_raw.trim());
    if parent.is_empty() || parent == "*" || parent == "_" {
        return None;
    }
    Some(parent.to_owned())
}

/// True if an alias can be removed without colliding in the same file.
#[must_use]
pub fn is_unnecessary_alias(
    orig: &str,
    alias: &str,
    binders: &BTreeSet<String>,
    locals: &BTreeSet<String>,
) -> bool {
    if orig == alias || alias == "_" {
        return true;
    }
    if PRELUDE.contains(&orig) {
        return false;
    }
    !binders.contains(orig) && !locals.contains(orig)
}

#[cfg(test)]
mod tests {
    use crate::rules_alias::import_alias::PRELUDE;

    #[test]
    fn prelude_covers_nightly_union() {
        assert!(
            PRELUDE.len() == 108,
            "PRELUDE length drifted; re-scrape the nightly std prelude pages"
        );
        for name in [
            "try",
            "pattern_type",
            "deref",
            "type_ascribe",
            "concat_bytes",
            "const_format_args",
            "log_syntax",
            "trace_macros",
            "cfg_select",
            "define_opaque",
            "derive_const",
            "eii",
            "eii_declaration",
            "test_case",
            "unsafe_eii",
            "alloc_error_handler",
            "cfg_accessible",
            "cfg_eval",
            "Future",
            "IntoFuture",
            "TryFrom",
            "TryInto",
            "FromIterator",
            "AsyncFn",
            "AsyncFnMut",
            "AsyncFnOnce",
            "Debug",
            "Hash",
        ] {
            assert!(
                PRELUDE.contains(&name),
                "PRELUDE missing nightly prelude item `{name}`"
            );
        }
    }
}
