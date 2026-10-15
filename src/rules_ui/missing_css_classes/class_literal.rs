//! Literal extraction for class-list parsing.
//!
//! Holds backslash-aware double-quoted literal scans, so list parsing reads
//! one spelling of string contents. Byte escapes decode to their escaped
//! byte while unterminated quotes stay silent.

/// Escape and resume index for the backslash at `cursor`, if escapable.
///
/// # Arguments
///
/// * `bytes` - Argument bytes under scan.
///
/// * `cursor` - Index of the backslash.
///
/// * `literal` - Mutable literal contents.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the escaped byte.
fn escape_at(bytes: &[u8], cursor: usize, literal: &mut String) -> Option<usize> {
    let next = bytes.get(cursor.saturating_add(1)).copied()?;
    literal.push(char::from(next));
    Some(cursor.saturating_add(2))
}

/// Literal and resume index for the quote at `idx`, if it closes.
///
/// # Arguments
///
/// * `bytes` - Argument bytes under scan.
///
/// * `idx` - Index of the opening quote.
///
/// # Returns
///
/// * `Option<(String, usize)>` - Literal with resume past its closer.
fn quoted_at(bytes: &[u8], idx: usize) -> Option<(String, usize)> {
    let mut literal = String::new();
    let mut cursor = idx.saturating_add(1);
    while cursor < bytes.len() {
        let byte = bytes.get(cursor).copied().unwrap_or(0);
        if byte == b'\\' {
            cursor = escape_at(bytes, cursor, &mut literal)?;
            continue;
        }
        if byte == b'"' {
            return Some((literal, cursor.saturating_add(1)));
        }
        literal.push(char::from(byte));
        cursor = cursor.saturating_add(1);
    }
    None
}

/// Quoted literals inside `inner`, handling backslash escapes.
///
/// # Arguments
///
/// * `inner` - Call argument text with strings intact.
///
/// # Returns
///
/// * `Vec<String>` - Literal contents without quotes.
#[must_use]
pub fn quoted_in(inner: &str) -> Vec<String> {
    let bytes = inner.as_bytes();
    let mut out = Vec::new();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        if bytes.get(idx) != Some(&b'"') {
            idx = idx.saturating_add(1);
            continue;
        }
        let Some((literal, next)) = quoted_at(bytes, idx) else {
            break;
        };
        out.push(literal);
        idx = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_css_classes::class_literal::quoted_in;

    #[test]
    fn escapes_decode() {
        assert_eq!(
            quoted_in("\"a\\\"b\""),
            vec!["a\"b".to_owned()],
            "escapes decode"
        );
        assert!(quoted_in("\"open").is_empty(), "open stays empty");
    }
}
