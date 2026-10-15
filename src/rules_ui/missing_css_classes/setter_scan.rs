//! Call-shape scans for setter default-class checks.
//!
//! Holds paren matching plus line counting for post-build setter lists, so
//! the driver maps blanked-code offsets onto original lines on one spelling
//! of call boundaries.

use crate::rules_ui::missing_css_classes::css_inventory::step_paren;

/// Exclusive end index past the paren closing `open`, if it closes.
///
/// Strings stay blanked, so every paren counts.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `open` - Byte index of the argument opener.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the matching closer.
#[must_use]
pub fn paren_end(body: &str, open: usize) -> Option<usize> {
    if body.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let tail = body.as_bytes().get(open.saturating_add(1)..)?;
    let mut depth: i32 = 1;
    for (offset, byte) in tail.iter().enumerate() {
        step_paren(Some(*byte), &mut depth);
        if depth <= 0 {
            return Some(
                open.saturating_add(1)
                    .saturating_add(offset)
                    .saturating_add(1),
            );
        }
    }
    None
}

/// Newlines before `pos` in `body`.
///
/// # Arguments
///
/// * `body` - Brace-balanced `fn` body with strings blanked and tails cut.
///
/// * `pos` - Byte index under scan.
///
/// # Returns
///
/// * `usize` - Count of newline bytes before `pos`.
#[must_use]
pub fn newlines_before(body: &str, pos: usize) -> usize {
    let bytes = body.as_bytes();
    let Some(head) = bytes.get(..pos) else {
        return 0;
    };
    let mut count: usize = 0;
    for byte in head {
        if *byte == b'\n' {
            count = count.saturating_add(1);
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_css_classes::setter_scan::{newlines_before, paren_end};

    #[test]
    fn closers_resolve() {
        assert_eq!(paren_end("f(a, (b))", 1), Some(9), "nested closes");
        assert!(paren_end("f(a,", 1).is_none(), "open stays empty");
    }

    #[test]
    fn line_counts_resolve() {
        assert_eq!(newlines_before("a\nb\nc", 2), 1, "one newline counts");
        assert_eq!(newlines_before("a", 9), 0, "past end stays zero");
    }
}
