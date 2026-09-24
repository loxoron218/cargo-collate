//! Group ranks for single `use` statements.
//!
//! Maps each import head to its `std`, external, self-package, or crate rank.

use std::collections::BTreeSet;

use crate::{
    lexer::{
        visibility::{strip_pub_block, strip_pub_word},
        word::boundary_after,
    },
    rules_tests::order::strip_attrs,
};

/// Rank of the `std` group (`std`, `core`, `alloc`).
const STD: u8 = 0;

/// Rank of third-party external crates.
const EXTERNAL: u8 = 1;

/// Rank of the own package name in integration targets.
const SELF_PACKAGE: u8 = 2;

/// Rank of crate-relative imports.
const INTERNAL: u8 = 3;

/// True if string-stripped `code` opens a `use` statement.
#[must_use]
pub fn is_use_start(code: &str) -> bool {
    let flat = strip_attrs(code);
    let trimmed = flat.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    let rest = strip_pub_word(rest.trim_start());
    rest.starts_with("use") && boundary_after(rest, 3)
}

/// Text after the leading `use` keyword of a joined statement.
fn use_body(joined: &str) -> String {
    let flat = strip_attrs(joined);
    let trimmed = flat.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    let rest = strip_pub_word(rest.trim_start());
    let Some(tail) = rest.strip_prefix("use") else {
        return String::new();
    };
    if !boundary_after(rest, 3) {
        return String::new();
    }
    tail.trim_start().to_owned()
}

/// Group rank and mixed flag for one joined statement.
#[must_use]
pub fn classify(joined: &str, pkg_name: Option<&str>, locals: &BTreeSet<String>) -> (u8, bool) {
    let body = use_body(joined);
    if body.trim_start().starts_with('{') {
        return classify_brace(&body, pkg_name, locals);
    }
    (rank_of(&head_segment(&body), pkg_name, locals), false)
}

/// First path segment of a use body with markers stripped.
fn head_segment(body: &str) -> String {
    let mut rest = body.trim_start();
    while let Some(tail) = rest.strip_prefix("::") {
        rest = tail.trim_start();
    }
    if let Some(tail) = rest.strip_prefix("r#") {
        rest = tail;
    }
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i.saturating_add(c.len_utf8());
        } else {
            break;
        }
    }
    rest.get(..end).map_or_default(str::to_owned)
}

/// Group rank for one head segment.
fn rank_of(head: &str, pkg_name: Option<&str>, locals: &BTreeSet<String>) -> u8 {
    if head == "std" || head == "core" || head == "alloc" {
        STD
    } else if head == "crate" || head == "self" || head == "super" {
        INTERNAL
    } else if pkg_name.is_some_and(|name| name == head) {
        SELF_PACKAGE
    } else if locals.contains(head) {
        INTERNAL
    } else {
        EXTERNAL
    }
}

/// Group rank for a consolidated `use { ... }` block plus mixed flag.
///
/// Only the first segment of each top-level entry counts, so nested paths
/// inside grouped entries never leak inner segments into the verdict.
fn classify_brace(body: &str, pkg_name: Option<&str>, locals: &BTreeSet<String>) -> (u8, bool) {
    let mut first: Option<u8> = None;
    let mut mixed = false;
    for entry in brace_entries(body) {
        fold_rank(
            &mut first,
            &mut mixed,
            rank_of(&head_segment(&entry), pkg_name, locals),
        );
    }
    (first.unwrap_or(EXTERNAL), mixed)
}

/// Top-level entries of a consolidated block split at depth-one commas.
fn brace_entries(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut depth: i32 = 0;
    let mut start = 0;
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes.get(i).copied() {
            Some(b'{') => {
                start = brace_opened(depth, start, i);
                depth = depth.saturating_add(1);
                i = i.saturating_add(1);
            }
            Some(b'}') => {
                depth = depth.saturating_sub(1);
                start = brace_closed(&mut out, body, depth, start, i);
                i = i.saturating_add(1);
            }
            Some(b',') if depth == 1 => {
                push_entry(&mut out, body, start, i);
                start = i.saturating_add(1);
                i = i.saturating_add(1);
            }
            _ => {
                i = i.saturating_add(1);
            }
        }
    }
    out
}

/// Entry start after an opening brace at the outer level.
const fn brace_opened(depth: i32, start: usize, pos: usize) -> usize {
    if depth == 0 {
        pos.saturating_add(1)
    } else {
        start
    }
}

/// Entry start after a closing brace, sealing outer entries.
fn brace_closed(out: &mut Vec<String>, body: &str, depth: i32, start: usize, pos: usize) -> usize {
    if depth == 0 {
        push_entry(out, body, start, pos);
        pos.saturating_add(1)
    } else {
        start
    }
}

/// Push the trimmed slice as one entry, if non-empty.
fn push_entry(out: &mut Vec<String>, body: &str, start: usize, end: usize) {
    if let Some(entry) = body.get(start..end)
        && !entry.trim().is_empty()
    {
        out.push(entry.trim().to_owned());
    }
}

/// Fold one rank into the running first-seen rank and mixed flag.
const fn fold_rank(first: &mut Option<u8>, mixed: &mut bool, rank: u8) {
    if let Some(seen) = *first {
        *mixed |= seen != rank;
    } else {
        *first = Some(rank);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::rules_imports::rank::{brace_entries, classify, head_segment, rank_of};

    #[test]
    fn heads_ranked() {
        let locals = BTreeSet::from(["gating".to_owned()]);
        assert_eq!(rank_of("std", None, &locals), 0, "std ranks first");
        assert_eq!(rank_of("core", None, &locals), 0, "core joins std");
        assert_eq!(rank_of("alloc", None, &locals), 0, "alloc joins std");
        assert_eq!(rank_of("anyhow", None, &locals), 1, "unknown is external");
        assert_eq!(
            rank_of("oxhidifi", Some("oxhidifi"), &locals),
            2,
            "own package ranks third"
        );
        assert_eq!(rank_of("crate", None, &locals), 3, "crate ranks last");
        assert_eq!(
            rank_of("gating", None, &locals),
            3,
            "bare locals join crate"
        );
    }

    #[test]
    fn segments_read() {
        assert_eq!(head_segment("std::{a};"), "std", "brace head reads");
        assert_eq!(head_segment("::foo::bar;"), "foo", "leading colons skip");
        assert_eq!(head_segment("foo as bar;"), "foo", "alias head reads");
        assert_eq!(head_segment(""), "", "empty stays empty");
    }

    #[test]
    fn nested_paths_stay_whole() {
        let entries = brace_entries(
            "{\n    symphonia::core::{\n        codecs::Audio,\n    },\n    tracing::warn,\n}",
        );
        assert_eq!(
            entries,
            vec![
                "symphonia::core::{\n        codecs::Audio,\n    }".to_owned(),
                "tracing::warn".to_owned(),
            ],
            "entries split whole"
        );
        let locals = BTreeSet::new();
        let nested =
            "use {\n    symphonia::core::{\n        codecs::Audio,\n    },\n    tracing::warn,\n};";
        let (group, mixed) = classify(nested, None, &locals);
        assert_eq!(group, 1, "nested paths group by head");
        assert!(!mixed, "inner segments stay silent");
    }

    #[test]
    fn consolidated_shapes_classified() {
        let locals = BTreeSet::new();
        let clean = "use {\n    anyhow::A,\n    serde::B,\n};";
        let (group, mixed) = classify(clean, None, &locals);
        assert_eq!(group, 1, "externals group together");
        assert!(!mixed, "uniform block stays silent");
        let dirty = "use {\n    anyhow::A,\n    crate::b,\n};";
        let (_, mixed) = classify(dirty, None, &locals);
        assert!(mixed, "mixed block flags");
    }
}
