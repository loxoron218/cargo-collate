//! Quoted, raw, and char literal scanning for tracing masks.

use crate::rules_simple::tracing_state::{Masker, mask_blank, mask_close, mask_opener, mask_push};

/// Next scan position after the quoted string opening at `pos`.
pub fn mask_quoted(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    let quote = pos.saturating_add(usize::from(bytes.get(pos) == Some(&b'b')));
    let content = quote.saturating_add(1);
    let mut scan = content;
    while scan < bytes.len() {
        let token = quoted_token(bytes, scan);
        if token == b'n' {
            return mask_quoted_split(masker, bytes, pos, quote, content, scan);
        }
        if token == b'e' {
            scan = scan.saturating_add(2);
            continue;
        }
        if token == b'c' {
            mask_push(masker, pos, bytes.get(content..scan).unwrap_or_default());
            mask_blank(masker, quote.saturating_add(1), scan);
            return scan.saturating_add(1);
        }
        scan = scan.saturating_add(1);
    }
    mask_push(masker, pos, bytes.get(content..scan).unwrap_or_default());
    mask_blank(masker, quote.saturating_add(1), scan);
    scan
}

/// Next scan position when a quoted string reaches a newline at `scan`.
fn mask_quoted_split(
    masker: &mut Masker,
    bytes: &[u8],
    pos: usize,
    quote: usize,
    content: usize,
    scan: usize,
) -> usize {
    if scan > content && bytes.get(scan.saturating_sub(1)) == Some(&b'\\') {
        masker.pending_start = pos;
        masker.pending.extend_from_slice(
            bytes
                .get(content..scan.saturating_sub(1))
                .unwrap_or_default(),
        );
        mask_blank(masker, quote.saturating_add(1), scan);
        masker.quoted = true;
        return scan;
    }
    mask_push(masker, pos, bytes.get(content..scan).unwrap_or_default());
    mask_blank(masker, quote.saturating_add(1), scan);
    scan
}

/// Next scan position after the raw string opening at `pos`.
pub fn mask_raw_open(
    masker: &mut Masker,
    bytes: &[u8],
    pos: usize,
    hashes: usize,
    content: usize,
) -> usize {
    let mut scan = content;
    while scan < bytes.len() {
        let newline = bytes.get(scan) == Some(&b'\n');
        let close = is_raw_close(bytes, scan, hashes);
        if newline {
            return mask_defer(masker, bytes, pos, content, scan, hashes);
        }
        if close {
            let past = scan.saturating_add(hashes.saturating_add(1));
            mask_push(masker, pos, bytes.get(content..scan).unwrap_or_default());
            mask_opener(masker, pos);
            mask_blank(masker, pos.saturating_add(1), past);
            return past;
        }
        scan = scan.saturating_add(1);
    }
    mask_defer(masker, bytes, pos, content, scan, hashes)
}

/// Defer a raw string opened at `pos` with content from `content`.
fn mask_defer(
    masker: &mut Masker,
    bytes: &[u8],
    pos: usize,
    content: usize,
    scan: usize,
    hashes: usize,
) -> usize {
    masker.pending_start = pos;
    masker
        .pending
        .extend_from_slice(bytes.get(content..scan).unwrap_or_default());
    mask_opener(masker, pos);
    mask_blank(masker, pos.saturating_add(1), scan);
    masker.raw = Some(hashes);
    scan
}

/// Next scan position inside a carried raw string from `pos`.
pub fn mask_raw(masker: &mut Masker, bytes: &[u8], pos: usize, hashes: usize) -> usize {
    let mut scan = pos;
    while scan < bytes.len() {
        let newline = bytes.get(scan) == Some(&b'\n');
        let close = is_raw_close(bytes, scan, hashes);
        if newline {
            masker
                .pending
                .extend_from_slice(bytes.get(pos..scan).unwrap_or_default());
            mask_blank(masker, pos, scan);
            return scan.saturating_add(1);
        }
        if close {
            let past = scan.saturating_add(hashes.saturating_add(1));
            masker
                .pending
                .extend_from_slice(bytes.get(pos..scan).unwrap_or_default());
            mask_close(masker);
            mask_blank(masker, pos, past);
            masker.raw = None;
            return past;
        }
        scan = scan.saturating_add(1);
    }
    masker
        .pending
        .extend_from_slice(bytes.get(pos..scan).unwrap_or_default());
    mask_blank(masker, pos, scan);
    scan
}

/// Next scan position inside a carried quoted string from `pos`.
pub fn mask_carried(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    let mut scan = pos;
    while scan < bytes.len() {
        let token = quoted_token(bytes, scan);
        if token == b'n' {
            return mask_carried_split(masker, bytes, pos, scan);
        }
        if token == b'e' {
            scan = scan.saturating_add(2);
            continue;
        }
        if token == b'c' {
            return mask_finish(masker, bytes, pos, scan, scan.saturating_add(1));
        }
        scan = scan.saturating_add(1);
    }
    masker
        .pending
        .extend_from_slice(bytes.get(pos..scan).unwrap_or_default());
    mask_blank(masker, pos, scan);
    scan
}

/// Class of the quoted-string byte at `scan` for line splitting.
///
/// Returns `b'n'` for newlines, `b'e'` for escapes, `b'c'` for closers, and
/// `b'p'` for plain content bytes.
fn quoted_token(bytes: &[u8], scan: usize) -> u8 {
    if bytes.get(scan) == Some(&b'\n') {
        b'n'
    } else if bytes.get(scan) == Some(&b'\\') {
        b'e'
    } else if bytes.get(scan) == Some(&b'"') {
        b'c'
    } else {
        b'p'
    }
}

/// Record the pending literal through `scan` and resume code at `past`.
fn mask_finish(masker: &mut Masker, bytes: &[u8], pos: usize, scan: usize, past: usize) -> usize {
    masker
        .pending
        .extend_from_slice(bytes.get(pos..scan).unwrap_or_default());
    mask_close(masker);
    mask_blank(masker, pos, past);
    masker.quoted = false;
    past
}

/// Next scan position when a carried string reaches a newline at `scan`.
fn mask_carried_split(masker: &mut Masker, bytes: &[u8], pos: usize, scan: usize) -> usize {
    if scan > pos && bytes.get(scan.saturating_sub(1)) == Some(&b'\\') {
        masker
            .pending
            .extend_from_slice(bytes.get(pos..scan.saturating_sub(1)).unwrap_or_default());
        mask_blank(masker, pos, scan);
        return scan.saturating_add(1);
    }
    mask_finish(masker, bytes, pos, scan, scan)
}

/// Hashes and content offset of the raw string opening at `pos`, if any.
///
/// Byte prefixes and any hash count stay covered; `r#ident` stays excluded.
#[must_use]
pub fn raw_open(bytes: &[u8], pos: usize) -> Option<(usize, usize)> {
    let mut scan = pos;
    if bytes.get(scan) == Some(&b'b') {
        scan = scan.saturating_add(1);
    }
    if bytes.get(scan) != Some(&b'r') {
        return None;
    }
    scan = scan.saturating_add(1);
    let mut hashes = 0_usize;
    while bytes.get(scan) == Some(&b'#') {
        hashes = hashes.saturating_add(1);
        scan = scan.saturating_add(1);
    }
    (bytes.get(scan) == Some(&b'"')).then_some((hashes, scan.saturating_add(1)))
}

/// True if a raw-string closer with `hashes` opens at `scan`.
fn is_raw_close(bytes: &[u8], scan: usize, hashes: usize) -> bool {
    let quote = bytes.get(scan) == Some(&b'"');
    let run = bytes
        .get(scan.saturating_add(1)..)
        .unwrap_or_default()
        .get(..hashes)
        .unwrap_or_default();
    quote && run.len() == hashes && run.iter().all(|byte| *byte == b'#')
}

/// End offset past the char literal opening at `pos`, if any.
///
/// Lifetimes and stray quotes stay excluded.
#[must_use]
pub fn char_end(bytes: &[u8], pos: usize) -> Option<usize> {
    if bytes.get(pos) != Some(&b'\'') {
        return None;
    }
    if bytes.get(pos.saturating_add(1)) == Some(&b'\\') {
        return escaped_end(bytes, pos);
    }
    simple_end(bytes, pos)
}

/// End offset past an escaped char literal from `pos`, if any.
fn escaped_end(bytes: &[u8], pos: usize) -> Option<usize> {
    if bytes.get(pos.saturating_add(2)) == Some(&b'u')
        && bytes.get(pos.saturating_add(3)) == Some(&b'{')
    {
        return unicode_end(bytes, pos);
    }
    if bytes.get(pos.saturating_add(3)) == Some(&b'\'') {
        return Some(pos.saturating_add(4));
    }
    None
}

/// End offset past a `\u{...}` char literal from `pos`, if any.
fn unicode_end(bytes: &[u8], pos: usize) -> Option<usize> {
    let close = scan_until_brace(bytes, pos.saturating_add(4))?;
    let past = close.saturating_add(1);
    if bytes.get(past) == Some(&b'\'') {
        return Some(past.saturating_add(1));
    }
    None
}

/// Offset of the next `}` at or after `from`, if any.
fn scan_until_brace(bytes: &[u8], from: usize) -> Option<usize> {
    let mut scan = from;
    while scan < bytes.len() {
        if bytes.get(scan) == Some(&b'}') {
            return Some(scan);
        }
        scan = scan.saturating_add(1);
    }
    None
}

/// End offset past a plain char literal from `pos`, if any.
fn simple_end(bytes: &[u8], pos: usize) -> Option<usize> {
    let body = bytes
        .get(pos.saturating_add(1))
        .copied()
        .is_some_and(is_char_body);
    if body && bytes.get(pos.saturating_add(2)) == Some(&b'\'') {
        return Some(pos.saturating_add(3));
    }
    None
}

/// True if `byte` continues a plain char literal body.
const fn is_char_body(byte: u8) -> bool {
    byte != b'\'' && byte != b'\n'
}
