//! Length-preserving source masking for tracing analysis.

use crate::rules_simple::tracing_string::{
    char_end, mask_carried, mask_quoted, mask_raw, mask_raw_open, raw_open,
};

/// Byte offset and content of one string literal in masked text.
#[derive(Debug)]
pub struct MaskedLiteral {
    /// Byte offset of the literal opener in masked text.
    pub start: usize,
    /// Raw bytes between the quotes with escapes unresolved.
    pub content: String,
}

/// Length-preserving mask of source text plus recorded literals.
#[derive(Debug)]
pub struct MaskedText {
    /// Source with strings, chars, and comments blanked to spaces.
    pub text: String,
    /// Literal offsets and contents in scan order.
    pub literals: Vec<MaskedLiteral>,
}

/// Incremental scanner behind [`mask_text`].
#[derive(Debug)]
pub struct Masker {
    /// Masked bytes with newlines preserved.
    pub out: Vec<u8>,
    /// Literal offsets and contents in scan order.
    pub literals: Vec<MaskedLiteral>,
    /// Open raw-string hashes, if any.
    pub raw: Option<usize>,
    /// True while a quoted string continues past a line break.
    pub quoted: bool,
    /// Open block-comment depth.
    pub block: usize,
    /// Offset of the pending literal opener in the source text.
    pub pending_start: usize,
    /// Content bytes collected for the pending literal.
    pub pending: Vec<u8>,
}

/// Mask one source text with literal contents recorded.
///
/// Strings, char literals, and comments blank to spaces with newlines kept,
/// so byte offsets in the mask match the input exactly.
///
/// # Arguments
///
/// * `text` - Joined source text with newlines preserved.
///
/// # Returns
///
/// * `MaskedText` - Aligned mask plus literal offsets and contents.
#[must_use]
pub fn mask_text(text: &str) -> MaskedText {
    let bytes = text.as_bytes();
    let mut masker = Masker {
        out: bytes.to_vec(),
        literals: Vec::new(),
        raw: None,
        quoted: false,
        block: 0,
        pending_start: 0,
        pending: Vec::new(),
    };
    let mut pos = 0;
    while pos < bytes.len() {
        pos = mask_step(&mut masker, bytes, pos);
    }
    if masker.raw.is_some() || masker.quoted {
        mask_close(&mut masker);
    }
    MaskedText {
        text: String::from_utf8_lossy(&masker.out).into_owned(),
        literals: masker.literals,
    }
}

/// Next scan position after the byte at `pos`.
fn mask_step(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    if let Some(hashes) = masker.raw {
        return mask_raw(masker, bytes, pos, hashes);
    }
    if masker.quoted {
        return mask_carried(masker, bytes, pos);
    }
    if masker.block > 0 {
        return mask_block(masker, bytes, pos);
    }
    mask_code(masker, bytes, pos)
}

/// Blank `from..end` to spaces with newlines preserved.
pub fn mask_blank(masker: &mut Masker, from: usize, end: usize) {
    let width = end.saturating_sub(from);
    for byte in masker.out.iter_mut().skip(from).take(width) {
        blank_byte(byte);
    }
}

/// Space out one masked byte with newlines preserved.
const fn blank_byte(byte: &mut u8) {
    if *byte != b'\n' {
        *byte = b' ';
    }
}

/// Mark a string opener at `pos` with a quote after blanking.
pub fn mask_opener(masker: &mut Masker, pos: usize) {
    if let Some(byte) = masker.out.get_mut(pos) {
        *byte = b'"';
    }
}

/// Record one literal spanning already-blanked bytes.
pub fn mask_push(masker: &mut Masker, start: usize, content: &[u8]) {
    masker.literals.push(MaskedLiteral {
        start,
        content: String::from_utf8_lossy(content).into_owned(),
    });
}

/// Record the pending multi-line literal and clear its buffer.
pub fn mask_close(masker: &mut Masker) {
    let content = masker.pending.clone();
    masker.pending.clear();
    mask_push(masker, masker.pending_start, &content);
}

/// Next scan position inside a block comment from `pos`.
fn mask_block(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    let mut scan = pos;
    while scan < bytes.len() && masker.block > 0 {
        let open = is_pair(bytes, scan, b'/', b'*');
        let close = is_pair(bytes, scan, b'*', b'/');
        if open {
            masker.block = masker.block.saturating_add(1);
            scan = scan.saturating_add(2);
        } else if close {
            masker.block = masker.block.saturating_sub(1);
            scan = scan.saturating_add(2);
        } else {
            scan = scan.saturating_add(1);
        }
    }
    mask_blank(masker, pos, scan);
    scan
}

/// True if `first` and `second` open at `scan`.
fn is_pair(bytes: &[u8], scan: usize, first: u8, second: u8) -> bool {
    bytes.get(scan) == Some(&first) && bytes.get(scan.saturating_add(1)) == Some(&second)
}

/// Next scan position for code bytes at `pos`.
fn mask_code(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    let byte = bytes.get(pos).copied().unwrap_or(0);
    if byte == b'/' {
        return mask_slash(masker, bytes, pos);
    }
    if byte == b'"' || (byte == b'b' && bytes.get(pos.saturating_add(1)) == Some(&b'"')) {
        return mask_quoted(masker, bytes, pos);
    }
    if byte == b'r' || (byte == b'b' && bytes.get(pos.saturating_add(1)) == Some(&b'r')) {
        return mask_raw_maybe(masker, bytes, pos);
    }
    if byte == b'\'' {
        return mask_char(masker, bytes, pos);
    }
    pos.saturating_add(1)
}

/// Next scan position after the slash at `pos`.
fn mask_slash(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    if bytes.get(pos.saturating_add(1)) == Some(&b'/') {
        let end = line_end(bytes, pos);
        mask_blank(masker, pos, end);
        return end;
    }
    if bytes.get(pos.saturating_add(1)) == Some(&b'*') {
        masker.block = masker.block.saturating_add(1);
        mask_blank(masker, pos, pos.saturating_add(2));
        return mask_block(masker, bytes, pos.saturating_add(2));
    }
    pos.saturating_add(1)
}

/// Next scan position after a possible raw opener at `pos`.
fn mask_raw_maybe(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    if let Some((hashes, content)) = raw_open(bytes, pos) {
        return mask_raw_open(masker, bytes, pos, hashes, content);
    }
    pos.saturating_add(1)
}

/// Next scan position after a possible char literal at `pos`.
fn mask_char(masker: &mut Masker, bytes: &[u8], pos: usize) -> usize {
    if let Some(end) = char_end(bytes, pos) {
        mask_blank(masker, pos.saturating_add(1), end.saturating_sub(1));
        return end;
    }
    pos.saturating_add(1)
}

/// Offset of the line ending at or after `pos`.
fn line_end(bytes: &[u8], pos: usize) -> usize {
    let mut scan = pos;
    while scan < bytes.len() && bytes.get(scan) != Some(&b'\n') {
        scan = scan.saturating_add(1);
    }
    scan
}
