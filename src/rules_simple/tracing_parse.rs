//! Call grouping and argument analysis for tracing events.

use crate::lexer::word::word_match_at;

/// Bang index of the next tracing invocation at or after `from`, if any.
#[must_use]
pub fn next_bang(masked: &str, from: usize) -> Option<usize> {
    let mut pos = from;
    while pos < masked.len() {
        let bang = bang_at_pos(masked, pos);
        if bang.is_some() {
            return bang;
        }
        pos = pos.saturating_add(1);
    }
    None
}

/// Bang index of a tracing invocation opening exactly at `pos`, if any.
///
/// Bare paths and `tracing::` qualifiers stay covered; other qualified
/// paths stay excluded since structured-field syntax differs per crate.
fn bang_at_pos(masked: &str, pos: usize) -> Option<usize> {
    for name in ["info", "warn", "error"] {
        let bang = bang_end_at(masked, name, pos);
        if bang.is_some() && is_tracing_path(masked, pos) {
            return bang;
        }
    }
    None
}

/// True if the macro at `name_pos` is bare or `tracing`-qualified.
///
/// Walks qualified segments backward to the leftmost one, so `log::` and
/// other facades stay excluded while `::tracing::` stays covered.
fn is_tracing_path(masked: &str, name_pos: usize) -> bool {
    let mut root = None;
    let mut scan = name_pos;
    loop {
        scan = skip_ws_back(masked, scan);
        if masked.get(scan.saturating_sub(2)..scan) != Some("::") {
            break;
        }
        scan = skip_ws_back(masked, scan.saturating_sub(2));
        let ident_start = ident_start_back(masked, scan);
        if ident_start == scan {
            break;
        }
        root = masked.get(ident_start..scan);
        scan = ident_start;
    }
    root.is_none_or(|segment| segment == "tracing")
}

/// Offset after skipping ASCII whitespace backward from `scan`.
fn skip_ws_back(masked: &str, scan: usize) -> usize {
    let mut pos = scan;
    while pos > 0
        && masked
            .as_bytes()
            .get(pos.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        pos = pos.saturating_sub(1);
    }
    pos
}

/// Start of the identifier ending at `scan`, scanning backward.
fn ident_start_back(masked: &str, scan: usize) -> usize {
    let mut pos = scan;
    while pos > 0
        && masked
            .as_bytes()
            .get(pos.saturating_sub(1))
            .copied()
            .is_some_and(is_ident_byte)
    {
        pos = pos.saturating_sub(1);
    }
    pos
}

/// True if `byte` continues an ASCII identifier.
const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Bang index when macro `name` invokes exactly at `pos`, if any.
///
/// Whole words only, so `my_info!` and `info_desc!` stay excluded.
fn bang_end_at(masked: &str, name: &str, pos: usize) -> Option<usize> {
    if !word_match_at(masked, name, pos) {
        return None;
    }
    let mut scan = pos.saturating_add(name.len());
    while masked
        .as_bytes()
        .get(scan)
        .is_some_and(u8::is_ascii_whitespace)
    {
        scan = scan.saturating_add(1);
    }
    (masked.as_bytes().get(scan) == Some(&b'!')).then_some(scan)
}

/// True if `text` invokes macro `name` with a `!`.
#[must_use]
pub fn has_bang_name(text: &str, name: &str) -> bool {
    let mut pos = 0;
    while pos < text.len() {
        if bang_end_at(text, name, pos).is_some() {
            return true;
        }
        pos = pos.saturating_add(1);
    }
    false
}

/// Index of the `(` opening the call after the `!` at `bang`, if any.
///
/// Only whitespace may sit between the bang and the paren.
pub fn paren_at(masked: &str, bang: usize) -> Option<usize> {
    let mut scan = bang.saturating_add(1);
    loop {
        let byte = masked.as_bytes().get(scan);
        if byte.is_some_and(u8::is_ascii_whitespace) {
            scan = scan.saturating_add(1);
        } else if byte == Some(&b'(') {
            return Some(scan);
        } else {
            return None;
        }
    }
}

/// Closing paren index of the group opening at `open`, if balanced.
#[must_use]
pub fn group_end(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 1_usize;
    let mut pos = open.saturating_add(1);
    while pos < masked.len() {
        depth = paren_depth(masked.as_bytes(), pos, depth);
        if depth == 0 {
            return Some(pos);
        }
        pos = pos.saturating_add(1);
    }
    None
}

/// Updated paren depth after the byte at `pos`.
fn paren_depth(bytes: &[u8], pos: usize, depth: usize) -> usize {
    if bytes.get(pos) == Some(&b'(') {
        depth.saturating_add(1)
    } else if bytes.get(pos) == Some(&b')') {
        depth.saturating_sub(1)
    } else {
        depth
    }
}

/// Non-metadata argument spans of `inner` with empties dropped.
///
/// Leading `target:` and `parent:` pairs never count as fields.
pub fn effective_args(inner: &str) -> Vec<(usize, usize)> {
    let args = arg_spans(inner);
    let mut first = 0;
    while first < args.len()
        && args
            .get(first)
            .is_some_and(|span| is_meta_arg(span_text(inner, *span)))
    {
        first = first.saturating_add(1);
    }
    args.get(first..).map_or_else(Vec::new, Vec::from)
}

/// Top-level comma-separated argument spans of `inner` with empties dropped.
fn arg_spans(inner: &str) -> Vec<(usize, usize)> {
    let bytes = inner.as_bytes();
    let mut raw = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0_usize;
    let mut pos = 0_usize;
    while pos < bytes.len() {
        if is_open_bracket(bytes, pos) {
            depth = depth.saturating_add(1);
        }
        if is_close_bracket(bytes, pos) {
            depth = depth.saturating_sub(1);
        }
        if bytes.get(pos) == Some(&b',') && depth == 0 {
            raw.push((start, pos));
            start = pos.saturating_add(1);
        }
        pos = pos.saturating_add(1);
    }
    raw.push((start, pos));
    raw.into_iter()
        .filter_map(|span| trim_span(inner, span))
        .collect()
}

/// True if an opening bracket sits at `pos`.
fn is_open_bracket(bytes: &[u8], pos: usize) -> bool {
    bytes.get(pos) == Some(&b'(') || bytes.get(pos) == Some(&b'[') || bytes.get(pos) == Some(&b'{')
}

/// True if a closing bracket sits at `pos`.
fn is_close_bracket(bytes: &[u8], pos: usize) -> bool {
    bytes.get(pos) == Some(&b')') || bytes.get(pos) == Some(&b']') || bytes.get(pos) == Some(&b'}')
}

/// Trimmed span unless blank.
fn trim_span(inner: &str, span: (usize, usize)) -> Option<(usize, usize)> {
    let bytes = inner.as_bytes();
    let mut start = span.0;
    let mut end = span.1;
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start = start.saturating_add(1);
    }
    while end > start
        && bytes
            .get(end.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        end = end.saturating_sub(1);
    }
    (start < end).then_some((start, end))
}

/// Argument text for `span`, empty on bad ranges.
#[must_use]
pub fn span_text(inner: &str, span: (usize, usize)) -> &str {
    inner.get(span.0..span.1).unwrap_or("")
}

/// True if `arg` holds tracing metadata, not a field.
fn is_meta_arg(arg: &str) -> bool {
    is_named_prefix(arg, "target") || is_named_prefix(arg, "parent")
}

/// True if `arg` starts with `name` plus an optional-space colon.
fn is_named_prefix(arg: &str, name: &str) -> bool {
    if !word_match_at(arg, name, 0) {
        return false;
    }
    let mut scan = name.len();
    while arg
        .as_bytes()
        .get(scan)
        .is_some_and(u8::is_ascii_whitespace)
    {
        scan = scan.saturating_add(1);
    }
    arg.as_bytes().get(scan) == Some(&b':')
}

/// True if `arg` is a well-formed tracing field.
///
/// Accepts shorthand idents, sigil values, and `name = value` assignments.
#[must_use]
pub fn is_field_arg(arg: &str) -> bool {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.as_bytes().first() == Some(&b'%') || trimmed.as_bytes().first() == Some(&b'?') {
        return trimmed.len() > 1;
    }
    if has_top_level_eq(trimmed) {
        return true;
    }
    is_bare_ident(trimmed)
}

/// True if `arg` holds a top-level `=` outside brackets.
fn has_top_level_eq(arg: &str) -> bool {
    let mut depth = 0_usize;
    for byte in arg.bytes() {
        if byte == b'(' || byte == b'[' || byte == b'{' {
            depth = depth.saturating_add(1);
        }
        if byte == b')' || byte == b']' || byte == b'}' {
            depth = depth.saturating_sub(1);
        }
        if byte == b'=' && depth == 0 {
            return true;
        }
    }
    false
}

/// True if `text` is a bare identifier usable as shorthand field.
fn is_bare_ident(text: &str) -> bool {
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() || first == b'_' => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
