//! Line-marker lexing for comment detection.

/// True if two slashes open at byte `i`.
fn slash_pair_at(bytes: &[u8], i: usize) -> bool {
    bytes.get(i).is_some_and(|left| *left == b'/')
        && bytes
            .get(i.saturating_add(1))
            .is_some_and(|right| *right == b'/')
}

/// Cut code already stripped at its first line marker, if any.
#[must_use]
pub fn cut_line_comment(code: &str) -> &str {
    let bytes = code.as_bytes();
    (0..bytes.len().saturating_sub(1))
        .find(|&i| slash_pair_at(bytes, i))
        .map_or(code, |i| code.get(..i).map_or(code, |part| part))
}

/// Byte index of the first plain marker, if any.
///
/// Doc and quadruple sequences stay exempt.
#[must_use]
pub fn plain_comment_at(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    (0..bytes.len().saturating_sub(1)).find(|&i| slash_pair_at(bytes, i) && plain_marker(bytes, i))
}

/// True if the marker at `i` is plain, not doc nor doubled.
fn plain_marker(bytes: &[u8], i: usize) -> bool {
    let prev_ok = i == 0
        || bytes
            .get(i.saturating_sub(1))
            .is_some_and(|prev| *prev != b'/');
    let next = bytes.get(i.saturating_add(2)).copied();
    prev_ok && next != Some(b'/') && next != Some(b'!')
}

/// True if `code` holds a plain marker, doc sequences exempt.
#[must_use]
pub fn has_plain_line_comment(code: &str) -> bool {
    plain_comment_at(code).is_some()
}

/// True if `code` (string-stripped) is a full-line `///` doc line.
///
/// Quadruple `////` sequences stay exempt.
#[must_use]
pub fn is_doc_line(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("///") && !trimmed.starts_with("////")
}

/// End index past a same-line block-comment closer from `open`, if any.
///
/// Openers nest, so each inner pair stays skipped with the comment.
fn block_close_line(chars: &[char], open: usize) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut j = open.saturating_add(2);
    while depth > 0 && j.saturating_add(1) < chars.len() {
        if chars.get(j) == Some(&'/') && chars.get(j.saturating_add(1)) == Some(&'*') {
            depth = depth.saturating_add(1);
            j = j.saturating_add(2);
        } else if chars.get(j) == Some(&'*') && chars.get(j.saturating_add(1)) == Some(&'/') {
            depth = depth.saturating_sub(1);
            j = j.saturating_add(2);
        } else {
            j = j.saturating_add(1);
        }
    }
    (depth <= 0).then_some(j)
}

/// Copy a same-line block comment to `out`; returns the resume index.
///
/// Quotes inside the comment copy through without opening strings; an
/// unclosed opener stays for legacy processing.
pub fn skip_block_comment(chars: &[char], i: usize, out: &mut String) -> Option<usize> {
    if chars.get(i) != Some(&'/') || chars.get(i.saturating_add(1)) != Some(&'*') {
        return None;
    }
    let end = block_close_line(chars, i)?;
    for c in chars.get(i..end).unwrap_or(&[]) {
        out.push(*c);
    }
    Some(end)
}
