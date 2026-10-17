//! Cfg-test marker matching for single lines.
//!
//! Leaf text matcher shared by range scans and file-level detectors, so
//! neither direction imports the other.

use crate::lexer::{literal::strip_strings, marker::cut_line_comment};

/// Index after ASCII whitespace from `i`.
///
/// # Arguments
///
/// * `bytes` - Raw line bytes.
/// * `i` - Start index.
///
/// # Returns
///
/// * `usize` - First non-whitespace index at or after `i`.
pub fn skip_ws_at(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i = i.saturating_add(1);
    }
    i
}

/// Index after `token` at `i` with whitespace allowed before it, if any.
fn skip_token(bytes: &[u8], i: usize, token: &str) -> Option<usize> {
    let start = skip_ws_at(bytes, i);
    let end = start.saturating_add(token.len());
    (bytes.get(start..end) == Some(token.as_bytes())).then_some(end)
}

/// True if an exact cfg-test marker opens at byte `i`.
///
/// Only `cfg(test)` with optional whitespace counts; predicates like
/// `not(test)` stay excluded.
fn cfg_test_open(bytes: &[u8], i: usize) -> bool {
    let mut j = i;
    for token in ["#", "[", "cfg", "(", "test", ")", "]"] {
        let Some(next) = skip_token(bytes, j, token) else {
            return false;
        };
        j = next;
    }
    true
}

/// True if `line` holds a real cfg-test attribute outside strings and tails.
///
/// # Arguments
///
/// * `line` - Original source line.
///
/// # Returns
///
/// * `bool` - True when `#[cfg(test)]` appears.
#[must_use]
pub fn is_real_cfg_line(line: &str) -> bool {
    let stripped = strip_strings(line);
    let code = cut_line_comment(&stripped);
    if code.contains("/*") {
        return false;
    }
    let bytes = code.as_bytes();
    (0..bytes.len()).any(|i| cfg_test_open(bytes, i))
}
