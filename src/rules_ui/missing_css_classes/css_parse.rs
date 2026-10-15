//! Class-list parsing for default-class checks.
//!
//! Holds quote-aware `css_classes` and `set_css_classes` argument scans plus
//! shared line slicing and hint formatting, so builder chains and post-build
//! setters share one spelling of list extraction. Dynamic variables stay
//! silent while empty literals flag.

use crate::{
    rules_ui::{missing_css_classes::class_literal::quoted_in, ui_ext::is_ident_byte},
    scan::SourceFile,
};

/// Byte index of the `(` opening `method` at `dot`, if any.
///
/// Requires a leading dot plus a trailing identifier boundary with gaps
/// allowed before the opener, so longer spellings stay silent. Shared with
/// setter scans so both spellings read one call boundary.
///
/// # Arguments
///
/// * `body` - Original text with strings intact.
///
/// * `dot` - Byte index of the candidate dot.
///
/// * `method` - Expected spelling such as `css_classes`.
///
/// # Returns
///
/// * `Option<usize>` - Opener index on bounded matches.
#[must_use]
pub fn method_open_at(body: &str, dot: usize, method: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    if bytes.get(dot) != Some(&b'.') {
        return None;
    }
    let start = dot.saturating_add(1);
    let end = start.saturating_add(method.len());
    if body.get(start..end) != Some(method) {
        return None;
    }
    if bytes.get(end).is_some_and(|next| is_ident_byte(*next)) {
        return None;
    }
    gap_open(bytes, end)
}

/// Opener past gaps at `from`, if `(` opens there.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
///
/// * `from` - Index past the method name.
///
/// # Returns
///
/// * `Option<usize>` - Opener index on matches.
fn gap_open(bytes: &[u8], from: usize) -> Option<usize> {
    let mut open = from;
    while bytes
        .get(open)
        .is_some_and(|next| *next == b' ' || *next == b'\t' || *next == b'\n' || *next == b'\r')
    {
        open = open.saturating_add(1);
    }
    (bytes.get(open) == Some(&b'(')).then_some(open)
}

/// Byte index of the `(` opening `method` in `body`, if any.
///
/// # Arguments
///
/// * `body` - Original text with strings intact.
///
/// * `method` - Expected spelling such as `css_classes`.
///
/// # Returns
///
/// * `Option<usize>` - First bounded opener index.
fn first_open(body: &str, method: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        if let Some(open) = method_open_at(body, idx, method) {
            return Some(open);
        }
        idx = idx.saturating_add(1);
    }
    None
}

/// Resume index past a string escape at `idx`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
///
/// * `idx` - Index of the backslash.
///
/// # Returns
///
/// * `usize` - Resume index past the escaped byte.
fn step_escape(bytes: &[u8], idx: usize) -> usize {
    idx.saturating_add(2).min(bytes.len().saturating_add(1))
}

/// Resume index past the byte at `idx` inside a string.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
///
/// * `idx` - Current index inside a string.
///
/// * `in_string` - Mutable string state cleared on the closer.
///
/// # Returns
///
/// * `usize` - Resume index past this byte.
fn step_string(bytes: &[u8], idx: usize, in_string: &mut bool) -> usize {
    if bytes.get(idx) == Some(&b'\\') {
        return step_escape(bytes, idx);
    }
    if bytes.get(idx) == Some(&b'"') {
        *in_string = false;
    }
    idx.saturating_add(1)
}

/// Resume index past the byte at `idx` inside a char literal.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
///
/// * `idx` - Current index inside a char literal.
///
/// * `in_char` - Mutable char state cleared on the closer.
///
/// # Returns
///
/// * `usize` - Resume index past this byte.
fn step_char(bytes: &[u8], idx: usize, in_char: &mut bool) -> usize {
    if bytes.get(idx) == Some(&b'\\') {
        return step_escape(bytes, idx);
    }
    if bytes.get(idx) == Some(&b'\'') {
        *in_char = false;
    }
    idx.saturating_add(1)
}

/// Closer past the code byte at `idx`, if it closes the call.
///
/// # Arguments
///
/// * `byte` - Byte under scan.
///
/// * `idx` - Current index in code.
///
/// * `depth` - Mutable paren depth.
///
/// * `in_string` - Mutable string state set on openers.
///
/// * `in_char` - Mutable char state set on openers.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the matching closer.
const fn step_code(
    byte: u8,
    idx: usize,
    depth: &mut i32,
    in_string: &mut bool,
    in_char: &mut bool,
) -> Option<usize> {
    match byte {
        b'"' => *in_string = true,
        b'\'' => *in_char = true,
        b'(' => *depth = depth.saturating_add(1),
        b')' => {
            *depth = depth.saturating_sub(1);
            if *depth <= 0 {
                return Some(idx.saturating_add(1));
            }
        }
        _ => {}
    }
    None
}

/// Exclusive end index past the paren closing `open`, if it closes.
///
/// Skips double-quoted literals with backslash escapes plus char literals,
/// so parens inside strings never unbalance the scan.
///
/// # Arguments
///
/// * `body` - Original text with strings intact.
///
/// * `open` - Byte index of the argument opener.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the matching closer.
fn match_close(body: &str, open: usize) -> Option<usize> {
    let bytes = body.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth: i32 = 1;
    let mut idx = open.saturating_add(1);
    let mut in_string = false;
    let mut in_char = false;
    while idx < bytes.len() {
        if in_string {
            idx = step_string(bytes, idx, &mut in_string);
            continue;
        }
        if in_char {
            idx = step_char(bytes, idx, &mut in_char);
            continue;
        }
        let byte = bytes.get(idx).copied().unwrap_or(0);
        if let Some(end) = step_code(byte, idx, &mut depth, &mut in_string, &mut in_char) {
            return Some(end);
        }
        idx = idx.saturating_add(1);
    }
    None
}

/// Class list for `method` in `original`, if statically known.
///
/// Returns `None` for missing calls and dynamic variables, `Some` with
/// possibly zero entries for empty literals.
///
/// # Arguments
///
/// * `original` - Original text with strings intact.
///
/// * `method` - Expected spelling such as `css_classes`.
///
/// # Returns
///
/// * `Option<Vec<String>>` - Parsed class names when statically known.
#[must_use]
pub fn list_for(original: &str, method: &str) -> Option<Vec<String>> {
    let open = first_open(original, method)?;
    let close = match_close(original, open)?;
    let inner = original.get(open.saturating_add(1)..close.saturating_sub(1))?;
    if inner.contains('"') {
        return Some(quoted_in(inner));
    }
    if inner.contains('[') {
        return Some(Vec::new());
    }
    None
}

/// Original lines from `start` through `end` one-based lines.
///
/// Shared with chain and setter scans so both directions slice files on one
/// spelling of line ranges.
///
/// # Arguments
///
/// * `file` - Scanned source file.
///
/// * `start` - One-based opening line.
///
/// * `end` - One-based closing line.
///
/// # Returns
///
/// * `String` - Joined original lines for list parsing.
#[must_use]
pub fn original_slice(file: &SourceFile, start: usize, end: usize) -> String {
    let begin = start.saturating_sub(1);
    if begin >= end || end > file.lines.len() {
        return String::new();
    }
    let Some(slice) = file.lines.get(begin..end) else {
        return String::new();
    };
    slice.join("\n")
}

/// Hint listing missing default classes.
///
/// # Arguments
///
/// * `missing` - Missing class spellings in inventory order.
///
/// # Returns
///
/// * `String` - Quoted spellings for the fix hint.
#[must_use]
pub fn missing_hint(missing: &[String]) -> String {
    missing
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<String>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_css_classes::css_parse::list_for;

    #[test]
    fn literals_parse() {
        assert_eq!(
            list_for(".css_classes([\"toggle\", \"flat\"])", "css_classes"),
            Some(vec!["toggle".to_owned(), "flat".to_owned()]),
            "literals parse"
        );
        assert_eq!(
            list_for(".set_css_classes(&[])", "set_css_classes"),
            Some(Vec::new()),
            "empty flags as empty"
        );
    }

    #[test]
    fn dynamics_stay_silent() {
        assert!(
            list_for(".css_classes(classes)", "css_classes").is_none(),
            "variables stay silent"
        );
        assert!(
            list_for(".tooltip_text(\"x\")", "css_classes").is_none(),
            "missing calls stay silent"
        );
    }

    #[test]
    fn gaps_and_multiline_parse() {
        assert_eq!(
            list_for(".css_classes ( [\"narrow\"] )", "css_classes"),
            Some(vec!["narrow".to_owned()]),
            "gaps parse"
        );
        assert_eq!(
            list_for(".css_classes([\n\"view\",\n])", "css_classes"),
            Some(vec!["view".to_owned()]),
            "multiline parses"
        );
    }
}
