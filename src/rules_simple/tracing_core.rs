//! Shared mask state for tracing literal scans.
//!
//! Leaf holder for the length-preserving mask plus literal recording, so the
//! code driver and the string scanners both build on it without importing
//! each other.

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

/// Incremental scanner behind `mask_text`.
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
