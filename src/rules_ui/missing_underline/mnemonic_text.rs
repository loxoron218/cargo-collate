//! Mnemonic literal scan for underline checks.
//!
//! Detects single-underscore `"_X"` markers inside same-line double-quoted
//! string literals when the underscore opens a word, so keyboard mnemonic
//! text counts while format placeholders (`{app_id}`), `snake_case`, URLs,
//! identifiers, comments, and char literals stay silent. Mid-word mnemonics
//! (`E_xit`) stay out by construction: the underscore must follow a
//! non-word byte.

/// True if a mnemonic marker opens at `idx` inside string content.
///
/// Requires a single underscore following a non-word byte plus an ASCII
/// alphanumeric mnemonic letter, so `"_Open"` and `"Save _As"` flag while
/// `{app_id}`, `a_b`, `E_xit`, and escaped doubles stay silent.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate underscore index.
///
/// # Returns
///
/// * `bool` - True for word-opening mnemonic markers.
fn mnemonic_at(bytes: &[u8], idx: usize) -> bool {
    if bytes.get(idx) != Some(&b'_') {
        return false;
    }
    if idx == 0 {
        return bytes
            .get(idx.saturating_add(1))
            .is_some_and(u8::is_ascii_alphanumeric);
    }
    if bytes
        .get(idx.saturating_sub(1))
        .is_some_and(|prev| prev.is_ascii_alphanumeric() || *prev == b'_')
    {
        return false;
    }
    bytes
        .get(idx.saturating_add(1))
        .is_some_and(u8::is_ascii_alphanumeric)
}

/// Resume index past a char literal at `idx`, if it closes on this line.
///
/// Covers simple `'x'` and escaped `'\\'` shapes so `'_'` never flags;
/// lifetimes and stray quotes fall through to normal stepping.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate quote index.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the closer, if it closes here.
fn char_past(bytes: &[u8], idx: usize) -> Option<usize> {
    if bytes.get(idx) != Some(&b'\'') {
        return None;
    }
    let second = idx.saturating_add(1);
    let third = idx.saturating_add(2);
    let fourth = idx.saturating_add(3);
    if bytes.get(second) == Some(&b'\\')
        && bytes.get(third).is_some()
        && bytes.get(fourth) == Some(&b'\'')
    {
        return Some(fourth.saturating_add(1));
    }
    if bytes
        .get(second)
        .is_some_and(|next| *next != b'\'' && *next != b'\n')
        && bytes.get(third) == Some(&b'\'')
    {
        return Some(third.saturating_add(1));
    }
    None
}

/// Advance inside a string, updating state on escapes and closers.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index inside a string.
/// * `in_string` - Mutable string state cleared on the closer.
///
/// # Returns
///
/// * `(usize, bool)` - Resume index and hit flag.
fn string_advance(bytes: &[u8], idx: usize, in_string: &mut bool) -> (usize, bool) {
    if bytes.get(idx) == Some(&b'\\') {
        return (idx.saturating_add(2), false);
    }
    if bytes.get(idx) == Some(&b'"') {
        *in_string = false;
        return (idx.saturating_add(1), false);
    }
    (idx.saturating_add(1), mnemonic_at(bytes, idx))
}

/// Advance in code, entering strings and ending at comments.
///
/// Returns a terminal verdict for slash-pair comments; char literals skip
/// whole while quotes enter string state.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index outside strings.
/// * `in_string` - Mutable string state set on quote openers.
///
/// # Returns
///
/// * `(usize, Option<bool>)` - Resume index and terminal verdict, if any.
fn code_advance(bytes: &[u8], idx: usize, in_string: &mut bool) -> (usize, Option<bool>) {
    if bytes.get(idx) == Some(&b'/') && bytes.get(idx.saturating_add(1)) == Some(&b'/') {
        return (idx, Some(false));
    }
    if let Some(past) = char_past(bytes, idx) {
        return (past, None);
    }
    if bytes.get(idx) == Some(&b'"') {
        *in_string = true;
    }
    (idx.saturating_add(1), None)
}

/// Advance one step, dispatching on string state.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `in_string` - Mutable string state.
///
/// # Returns
///
/// * `(usize, Option<bool>)` - Resume index and terminal verdict, if any.
fn scan_step(bytes: &[u8], idx: usize, in_string: &mut bool) -> (usize, Option<bool>) {
    if *in_string {
        let (next, hit) = string_advance(bytes, idx, in_string);
        if hit {
            return (next, Some(true));
        }
        return (next, None);
    }
    code_advance(bytes, idx, in_string)
}

/// True if `line` holds a mnemonic marker inside a string literal.
///
/// Tracks same-line double-quoted strings with backslash escapes; a
/// slash-pair outside strings ends the line so trailing comments never flag.
///
/// # Arguments
///
/// * `line` - Original source line with strings intact.
///
/// # Returns
///
/// * `bool` - True when `_<alphanumeric>` appears in a string.
fn line_has_mnemonic(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut idx = 0;
    let mut in_string = false;
    while idx < bytes.len() {
        let (next, verdict) = scan_step(bytes, idx, &mut in_string);
        if verdict == Some(true) {
            return true;
        }
        if verdict == Some(false) {
            return false;
        }
        idx = next;
    }
    false
}

/// True if `text` holds a mnemonic marker inside a string literal.
///
/// Joins multi-line bodies line by line, so each literal resolves on its own
/// line while code, comments, and char literals stay silent.
///
/// # Arguments
///
/// * `text` - Original body text with strings intact.
///
/// # Returns
///
/// * `bool` - True when `_<alphanumeric>` appears in a string.
#[must_use]
pub fn has_mnemonic_literal(text: &str) -> bool {
    for line in text.lines() {
        if line_has_mnemonic(line) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_underline::mnemonic_text::has_mnemonic_literal;

    #[test]
    fn literals_split_comments() {
        assert!(
            has_mnemonic_literal("let s = \"_Open\";"),
            "mnemonic literal flags"
        );
        assert!(
            has_mnemonic_literal("let s = \"Save _As\";"),
            "spaced mnemonic flags"
        );
        assert!(
            !has_mnemonic_literal("let s = \"Open\";"),
            "plain literal stays silent"
        );
        assert!(
            !has_mnemonic_literal("let s = \"__\";"),
            "escaped double stays silent"
        );
        assert!(
            !has_mnemonic_literal("let s = \"{app_id}\";"),
            "placeholders stay silent"
        );
        assert!(
            !has_mnemonic_literal("let s = \"a_b\";"),
            "snake case stays silent"
        );
        assert!(
            !has_mnemonic_literal("let s = \"E_xit\";"),
            "mid-word markers stay silent"
        );
        assert!(
            !has_mnemonic_literal("// \"_Open\""),
            "comments stay silent"
        );
        assert!(
            !has_mnemonic_literal("let x = 1; // \"_Open\""),
            "tails stay silent"
        );
        assert!(
            !has_mnemonic_literal("let _field = 1;"),
            "identifiers stay silent"
        );
        assert!(
            !has_mnemonic_literal("let c = '_';"),
            "char literals stay silent"
        );
    }
}
