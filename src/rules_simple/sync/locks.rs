//! `use` statement collection and lock import resolution.

use crate::{
    lexer::marker::cut_line_comment,
    rules_imports::rank::is_use_start,
    rules_simple::sync::detect::{
        CELL_ITEMS, CELL_ROOTS, RC_ITEMS, RC_ROOTS, SYNC_ITEMS, is_ident_byte, is_word_start,
    },
    scan::SourceFile,
};

/// One joined `use` statement with its source range.
#[derive(Debug)]
pub struct UseBlock {
    /// Zero-based line where the statement starts.
    pub start: usize,
    /// Zero-based line where the statement ends.
    pub end: usize,
    /// Joined code of the statement without line comments.
    pub joined: String,
}

/// True if `word` matches at `i` with boundaries on both sides.
fn word_at(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    let after = i.saturating_add(word.len());
    is_word_start(bytes, i)
        && bytes.get(after).is_none_or(|b| !is_ident_byte(*b))
        && bytes.get(i..after) == Some(word)
}

/// End index and joined code of the `use` statement at `start`, if closed.
fn use_span(file: &SourceFile, start: usize) -> Option<(usize, String)> {
    let first = file.stripped.get(start)?;
    let mut joined = cut_line_comment(first).to_owned();
    let mut end = start;
    while !joined.contains(';') {
        let next = file.stripped.get(end.saturating_add(1))?;
        end = end.saturating_add(1);
        joined.push('\n');
        joined.push_str(cut_line_comment(next));
    }
    Some((end, joined))
}

/// Joined `use` statements of one file with line ranges.
fn collect_use_blocks(file: &SourceFile) -> Vec<UseBlock> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < file.stripped.len() {
        let Some(code) = file.stripped.get(idx) else {
            break;
        };
        if !is_use_start(cut_line_comment(code)) {
            idx = idx.saturating_add(1);
            continue;
        }
        let Some((end, joined)) = use_span(file, idx) else {
            break;
        };
        out.push(UseBlock {
            start: idx,
            end,
            joined,
        });
        idx = end.saturating_add(1);
    }
    out
}

/// Blocks and per-block std lock names of one file.
#[must_use]
pub fn use_blocks_with_locks(file: &SourceFile) -> (Vec<UseBlock>, Vec<Vec<String>>) {
    let blocks = collect_use_blocks(file);
    let mut names = Vec::new();
    for block in &blocks {
        names.push(block_std_names(&block.joined));
    }
    (blocks, names)
}

/// Blocks and per-block thread-unsafe names of one file.
#[must_use]
pub fn use_blocks_with_thread_unsafe(file: &SourceFile) -> (Vec<UseBlock>, Vec<Vec<String>>) {
    let blocks = collect_use_blocks(file);
    let mut names = Vec::new();
    for block in &blocks {
        names.push(block_thread_names(&block.joined));
    }
    (blocks, names)
}

/// Text after the leading `use` keyword of a joined statement.
fn use_body(joined: &str) -> String {
    let bytes = joined.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if word_at(bytes, i, b"use".as_slice()) {
            return joined.get(i.saturating_add(3)..).unwrap_or("").to_owned();
        }
        i = i.saturating_add(1);
    }
    String::new()
}

/// Push the trimmed slice as one entry, if non-empty.
fn push_top_entry(out: &mut Vec<String>, text: &str, start: usize, end: usize) {
    if let Some(part) = text.get(start..end)
        && !part.trim().is_empty()
    {
        out.push(part.trim().to_owned());
    }
}

/// Top-level comma entries of `text`, keeping nested braces whole.
fn split_top_level(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes.get(i) {
            Some(b'{') => depth = depth.saturating_add(1),
            Some(b'}') => depth = depth.saturating_sub(1),
            Some(b',') if depth == 0 => {
                push_top_entry(&mut out, text, start, i);
                start = i.saturating_add(1);
            }
            Some(_) | None => {}
        }
        i = i.saturating_add(1);
    }
    push_top_entry(&mut out, text, start, text.len());
    out
}

/// Byte index of the `}` matching the `{` at `open`, if any.
fn find_matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth: i32 = 0;
    let mut i = open;
    while i < bytes.len() {
        match bytes.get(i) {
            Some(b'{') => depth = depth.saturating_add(1),
            Some(b'}') if depth == 1 => return Some(i),
            Some(b'}') => depth = depth.saturating_sub(1),
            Some(_) | None => {}
        }
        i = i.saturating_add(1);
    }
    None
}

/// Push leaf `use` paths of `text` with `prefix`, expanding braces.
fn expand_tree(text: &str, prefix: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let Some(open) = trimmed.find('{') else {
        out.push(format!("{prefix}{trimmed}"));
        return;
    };
    let Some(close) = find_matching_brace(trimmed, open) else {
        out.push(format!("{prefix}{trimmed}"));
        return;
    };
    let head = trimmed.get(..open).unwrap_or("");
    let Some(inner) = trimmed.get(open.saturating_add(1)..close) else {
        return;
    };
    let mut next = prefix.to_owned();
    next.push_str(head);
    for entry in split_top_level(inner) {
        expand_tree(&entry, &next, out);
    }
}

/// Leaf `use` paths of one statement body.
fn expand_use_body(body: &str) -> Vec<String> {
    let cleaned = body.trim().trim_end_matches(';').trim().to_owned();
    let mut out = Vec::new();
    expand_tree(&cleaned, "", &mut out);
    out
}

/// Path and alias parts of a leaf around a word-boundary `as`, if any.
fn alias_split(leaf: &str) -> (&str, Option<&str>) {
    let bytes = leaf.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if word_at(bytes, i, b"as".as_slice()) {
            let path = leaf.get(..i).unwrap_or("");
            let alias = leaf.get(i.saturating_add(2)..).unwrap_or("").trim();
            return (path, Some(alias));
        }
        i = i.saturating_add(1);
    }
    (leaf, None)
}

/// Path segments of a leaf before any `as` alias.
fn leaf_segments(leaf: &str) -> Vec<String> {
    let mut segments = Vec::new();
    for part in alias_split(leaf).0.split("::") {
        let segment = part.trim().trim_start_matches("r#");
        if !segment.is_empty() {
            segments.push(segment.to_owned());
        }
    }
    segments
}

/// Local name of a leaf after any `as` alias, if any.
fn leaf_alias(leaf: &str) -> Option<String> {
    let alias = alias_split(leaf).1?;
    let name = alias.trim().trim_start_matches("r#");
    (!name.is_empty()).then(|| name.to_owned())
}

/// True if `segments` name a `std::sync` lock or glob.
fn is_sync_path(segments: &[String], last: &str) -> bool {
    segments.len() == 3
        && segments.first().is_some_and(|head| head.as_str() == "std")
        && segments.get(1).is_some_and(|mid| mid.as_str() == "sync")
        && segments.get(2).is_some_and(|tail| tail.as_str() == last)
}

/// True if `leaf` is a `std::sync::*` glob import.
fn is_std_glob(leaf: &str) -> bool {
    is_sync_path(&leaf_segments(leaf), "*")
}

/// Local name when `leaf` imports a `std` sync item, if any.
fn std_lock_local(leaf: &str) -> Option<String> {
    let segments = leaf_segments(leaf);
    let is_lock = SYNC_ITEMS.iter().any(|name| is_sync_path(&segments, name));
    if !is_lock {
        return None;
    }
    if let Some(alias) = leaf_alias(leaf) {
        return Some(alias);
    }
    segments.get(2).cloned()
}

/// Push the std lock name of `leaf` to `out`, if any.
fn extend_lock_name(leaf: &str, out: &mut Vec<String>) {
    if let Some(name) = std_lock_local(leaf) {
        out.push(name);
    }
}

/// Push every `SYNC_ITEMS` name to `out` for a glob import.
fn extend_glob_names(out: &mut Vec<String>) {
    for name in SYNC_ITEMS {
        out.push(name.to_owned());
    }
}

/// Local lock names a joined `use` statement pulls from `std`.
fn block_std_names(joined: &str) -> Vec<String> {
    let mut out = Vec::new();
    for leaf in expand_use_body(&use_body(joined)) {
        if is_std_glob(&leaf) {
            extend_glob_names(&mut out);
        } else {
            extend_lock_name(&leaf, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// True if `segments` name a thread-unsafe item or glob under cell/rc roots.
fn is_thread_path(segments: &[String], last: &str) -> bool {
    if segments.len() != 3 {
        return false;
    }
    let Some(head) = segments.first() else {
        return false;
    };
    let Some(mid) = segments.get(1) else {
        return false;
    };
    let Some(tail) = segments.get(2) else {
        return false;
    };
    if tail.as_str() != last {
        return false;
    }
    CELL_ROOTS
        .iter()
        .chain(RC_ROOTS.iter())
        .any(|(root, module)| head.as_str() == *root && mid.as_str() == *module)
}

/// Items a glob on `segments` pulls in, if any.
fn thread_glob_items(segments: &[String]) -> Option<&'static [&'static str]> {
    if segments.len() != 3 {
        return None;
    }
    let head = segments.first()?.as_str();
    let mid = segments.get(1)?.as_str();
    if CELL_ROOTS
        .iter()
        .any(|(root, module)| head == *root && mid == *module)
    {
        return Some(&CELL_ITEMS);
    }
    if RC_ROOTS
        .iter()
        .any(|(root, module)| head == *root && mid == *module)
    {
        return Some(&RC_ITEMS);
    }
    None
}

/// True if `leaf` is a thread-unsafe glob import.
fn is_thread_glob(leaf: &str) -> bool {
    is_thread_path(&leaf_segments(leaf), "*")
}

/// Local name when `leaf` imports a thread-unsafe item, if any.
fn thread_local(leaf: &str) -> Option<String> {
    let segments = leaf_segments(leaf);
    let items = thread_glob_items(&segments)?;
    let tail = segments.get(2)?;
    if !items.contains(&tail.as_str()) {
        return None;
    }
    if let Some(alias) = leaf_alias(leaf) {
        return Some(alias);
    }
    Some(tail.clone())
}

/// Push the thread-unsafe name of `leaf` to `out`, if any.
fn extend_thread_name(leaf: &str, out: &mut Vec<String>) {
    if let Some(name) = thread_local(leaf) {
        out.push(name);
    }
}

/// Push every item of `items` to `out`.
fn push_thread_items(items: &[&str], out: &mut Vec<String>) {
    for name in items {
        out.push((*name).to_owned());
    }
}

/// Push globbed items of `segments` to `out`, if any.
fn push_glob_segments(segments: &[String], out: &mut Vec<String>) {
    if let Some(items) = thread_glob_items(segments) {
        push_thread_items(items, out);
    }
}

/// Push every globbed thread-unsafe name of `leaf` to `out`, if any.
fn extend_thread_glob(leaf: &str, out: &mut Vec<String>) {
    if is_thread_glob(leaf) {
        push_glob_segments(&leaf_segments(leaf), out);
    } else {
        extend_thread_name(leaf, out);
    }
}

/// Local thread-unsafe names a joined `use` statement pulls in.
fn block_thread_names(joined: &str) -> Vec<String> {
    let mut out = Vec::new();
    for leaf in expand_use_body(&use_body(joined)) {
        extend_thread_glob(&leaf, &mut out);
    }
    out.sort();
    out.dedup();
    out
}
