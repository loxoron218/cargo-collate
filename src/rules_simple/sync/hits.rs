//! File aggregation for std lock findings.

use crate::{
    lexer::marker::cut_line_comment,
    rules_simple::sync::{
        detect::{has_std_sync_lock, is_ident_byte, is_word_start, skip_ws},
        locks::{UseBlock, use_blocks_with_locks},
    },
    scan::SourceFile,
};

/// True if `keyword` opens at `i` with `name` following it.
fn keyword_name_at(bytes: &[u8], i: usize, keyword: &str, name: &str) -> bool {
    if !keyword_at(bytes, i, keyword) {
        return false;
    }
    let start = skip_ws(bytes, i.saturating_add(keyword.len()));
    let end = start.saturating_add(name.len());
    bytes.get(start..end) == Some(name.as_bytes())
        && bytes.get(end).is_none_or(|b| !is_ident_byte(*b))
}

/// True if `keyword` matches at `i` with boundaries on both sides.
fn keyword_at(bytes: &[u8], i: usize, keyword: &str) -> bool {
    let after = i.saturating_add(keyword.len());
    is_word_start(bytes, i)
        && bytes.get(after).is_none_or(|b| !is_ident_byte(*b))
        && bytes.get(i..after) == Some(keyword.as_bytes())
}

/// True if `code` defines `name` as a local item shadowing an import.
fn defines_name(code: &str, name: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if defines_at(bytes, i, name) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True if an item keyword at `i` defines `name`.
fn defines_at(bytes: &[u8], i: usize, name: &str) -> bool {
    for keyword in ["struct", "enum", "type", "union", "mod"] {
        if keyword_name_at(bytes, i, keyword, name) {
            return true;
        }
    }
    false
}

/// True if any line of `file` defines `name` as a local item.
#[must_use]
pub fn file_defines(file: &SourceFile, name: &str) -> bool {
    file.stripped
        .iter()
        .any(|code| defines_name(cut_line_comment(code), name))
}

/// True if `idx` sits inside any joined `use` statement range.
#[must_use]
pub fn line_in_blocks(idx: usize, blocks: &[UseBlock]) -> bool {
    blocks
        .iter()
        .any(|block| idx >= block.start && idx <= block.end)
}

/// Push one `path:line:content` finding for `idx`.
pub fn push_hit_at(out: &mut Vec<String>, file: &SourceFile, idx: usize) {
    if let Some(line) = file.lines.get(idx) {
        out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
    }
}

/// True if `name` matches at `i` with identifier boundaries on both sides.
fn whole_word_at(bytes: &[u8], i: usize, name: &str) -> bool {
    let end = i.saturating_add(name.len());
    is_word_start(bytes, i)
        && bytes.get(end).is_none_or(|b| !is_ident_byte(*b))
        && bytes.get(i..end) == Some(name.as_bytes())
}

/// True if the non-whitespace code before `i` ends in `::`.
fn path_qualified_before(bytes: &[u8], i: usize) -> bool {
    let mut j = i;
    while j > 0
        && bytes
            .get(j.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        j = j.saturating_sub(1);
    }
    if j < 2 {
        return false;
    }
    let prev = j.saturating_sub(1);
    bytes.get(prev) == Some(&b':') && bytes.get(prev.saturating_sub(1)) == Some(&b':')
}

/// True if `code` uses `name` bare instead of inside a `::` path.
#[must_use]
pub fn has_bare_name(code: &str, name: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if whole_word_at(bytes, i, name) && !path_qualified_before(bytes, i) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True if `code` uses any imported std lock name bare.
fn bare_std_use(code: &str, std_names: &[String]) -> bool {
    std_names.iter().any(|name| has_bare_name(code, name))
}

/// True if `tail` violates the std lock rule under imported `names`.
fn line_violates(tail: &str, names: &[String]) -> bool {
    has_std_sync_lock(tail) || (!names.is_empty() && bare_std_use(tail, names))
}

/// Std `Mutex`, `RwLock`, and `Condvar` uses; import `parking_lot` equivalents instead.
#[must_use]
pub fn std_sync_hits(files: &[SourceFile]) -> Vec<String> {
    let mut out = Vec::new();
    for file in files {
        check_file_std_sync(file, &mut out);
    }
    out
}

/// Append `std` lock findings of one file to `out`.
fn check_file_std_sync(file: &SourceFile, out: &mut Vec<String>) {
    let (blocks, per_block) = use_blocks_with_locks(file);
    let names = collect_names(file, &blocks, per_block, out);
    check_body_with(file, &blocks, &names, out, line_violates);
}

/// Collect banned names from use blocks, pushing import hits to `out`.
pub fn collect_names(
    file: &SourceFile,
    blocks: &[UseBlock],
    per_block: Vec<Vec<String>>,
    out: &mut Vec<String>,
) -> Vec<String> {
    let mut names = Vec::new();
    for (block, local) in blocks.iter().zip(per_block) {
        if !local.is_empty() {
            push_hit_at(out, file, block.start);
            names.extend(local);
        }
    }
    names.sort();
    names.dedup();
    names.retain(|name| !file_defines(file, name));
    names
}

/// Append explicit and resolved bare-name findings of one file to `out`.
pub fn check_body_with(
    file: &SourceFile,
    blocks: &[UseBlock],
    names: &[String],
    out: &mut Vec<String>,
    violates: fn(&str, &[String]) -> bool,
) {
    for (idx, code) in file.stripped.iter().enumerate() {
        if line_in_blocks(idx, blocks) {
            continue;
        }
        let tail = cut_line_comment(code);
        if tail.contains("/*") {
            continue;
        }
        if violates(tail, names) {
            push_hit_at(out, file, idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::sync::hits::std_sync_hits, scan::builders::test_file};

    #[test]
    fn std_imports_flagged() {
        for line in [
            "use std::sync::Mutex;",
            "use std::sync::RwLock;",
            "use std::sync::Condvar;",
            "use std::sync::{Mutex, RwLock};",
            "use std::sync::{Mutex, Condvar};",
            "use std::{sync::Mutex};",
            "use ::std::sync::RwLock;",
            "use std::sync::Mutex as StdMutex;",
            "use std::sync::*;",
        ] {
            let files = vec![test_file("src/a.rs", &[line])];
            assert_eq!(std_sync_hits(&files).len(), 1, "{line} flags");
        }
    }

    #[test]
    fn allowed_locks_silent() {
        let parking = vec![test_file(
            "src/a.rs",
            &["use parking_lot::Mutex;", "let m = Mutex::new(1);"],
        )];
        assert!(
            std_sync_hits(&parking).is_empty(),
            "parking_lot stays silent"
        );
        let parking_condvar = vec![test_file(
            "src/a.rs",
            &["use parking_lot::Condvar;", "let c = Condvar::new();"],
        )];
        assert!(
            std_sync_hits(&parking_condvar).is_empty(),
            "parking_lot condvar stays silent"
        );
        let tokio = vec![test_file(
            "src/a.rs",
            &["use tokio::sync::Mutex;", "let m = Mutex::new(1);"],
        )];
        assert!(std_sync_hits(&tokio).is_empty(), "tokio stays silent");
        let once = vec![test_file(
            "src/a.rs",
            &["use std::sync::OnceLock;", "let o = OnceLock::new();"],
        )];
        assert!(std_sync_hits(&once).is_empty(), "oncelock stays silent");
        let bare = vec![test_file("src/a.rs", &["let m: Mutex = Mutex::new(1);"])];
        assert!(
            std_sync_hits(&bare).is_empty(),
            "bare without import stays silent"
        );
    }

    #[test]
    fn bare_names_follow_imports() {
        let files = vec![test_file(
            "src/a.rs",
            &["use std::sync::Mutex;", "let m: Mutex = Mutex::new(1);"],
        )];
        assert_eq!(std_sync_hits(&files).len(), 2, "import and use flag");
        let condvar = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::Condvar;",
                "let c: Condvar = Condvar::new();",
            ],
        )];
        assert_eq!(std_sync_hits(&condvar).len(), 2, "condvar flags");
        let aliased = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::Mutex as StdMutex;",
                "let m: StdMutex = StdMutex::new(1);",
            ],
        )];
        assert_eq!(std_sync_hits(&aliased).len(), 2, "alias and use flag");
        let shadow = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::Mutex as StdMutex;",
                "let m: Mutex = Mutex::new(1);",
            ],
        )];
        assert_eq!(std_sync_hits(&shadow).len(), 1, "unaliased bare silent");
    }

    #[test]
    fn qualified_other_crates_silent() {
        let mixed = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::Mutex;",
                "let m = parking_lot::Mutex::new(1);",
            ],
        )];
        assert_eq!(std_sync_hits(&mixed).len(), 1, "parking_lot stays silent");
        let other = vec![test_file(
            "src/a.rs",
            &["use std::sync::Mutex;", "let m = foo::Mutex::new(1);"],
        )];
        assert_eq!(std_sync_hits(&other).len(), 1, "other paths stay silent");
        let spaced = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::RwLock;",
                "let r = parking_lot :: RwLock :: new(1);",
            ],
        )];
        assert_eq!(std_sync_hits(&spaced).len(), 1, "spaced paths stay silent");
    }

    #[test]
    fn multiline_and_qualified_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["use std::sync::{", "    Mutex,", "    RwLock,", "};"],
        )];
        assert_eq!(std_sync_hits(&files).len(), 1, "multiline flags once");
        let qualified = vec![test_file(
            "src/a.rs",
            &["fn f() {", "    let m = std::sync::Mutex::new(1);", "}"],
        )];
        assert_eq!(std_sync_hits(&qualified).len(), 1, "qualified flags");
    }

    #[test]
    fn strings_comments_locals_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                r#"let s = "std::sync::Mutex";"#,
                "let x = 1; // use std::sync::Mutex;",
            ],
        )];
        assert!(
            std_sync_hits(&files).is_empty(),
            "strings and tails stay silent"
        );
        let local = vec![test_file(
            "src/a.rs",
            &[
                "use std::sync::Mutex;",
                "struct Mutex;",
                "let m: Mutex = Mutex::new();",
            ],
        )];
        assert_eq!(std_sync_hits(&local).len(), 1, "locals exempt bare uses");
    }
}
