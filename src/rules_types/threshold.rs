//! Type-complexity threshold shared by the type rules.
//!
//! Reads `type-complexity-threshold` from `clippy.toml` so the lint stays in
//! parity with `clippy::type_complexity`. Falls back to the Clippy default
//! when the file is missing or the key is absent.

use std::fs::read_to_string;

/// Clippy default for `type-complexity-threshold`.
const DEFAULT_THRESHOLD: u64 = 250;

/// Key spellings accepted from `clippy.toml`.
const KEYS: [&str; 2] = ["type-complexity-threshold", "type_complexity_threshold"];

/// Value parsed from one `clippy.toml` line, if any.
///
/// Accepts bare integers with optional trailing comments; quoted values stay
/// ignored since the Clippy key is numeric.
fn value_on_line(line: &str) -> Option<u64> {
    let code = line.split('#').next().unwrap_or("").trim();
    if code.is_empty() {
        return None;
    }
    let (left, right) = code.split_once('=')?;
    if !KEYS.contains(&left.trim()) {
        return None;
    }
    let mut value: u64 = 0;
    let mut digits = 0_u64;
    for c in right.trim().chars() {
        match c.to_digit(10) {
            Some(digit) => {
                value = value.saturating_mul(10).saturating_add(u64::from(digit));
                digits = digits.saturating_add(1);
            }
            None if digits > 0 => break,
            None if c.is_ascii_whitespace() => {}
            None => return None,
        }
    }
    (digits > 0).then_some(value)
}

/// Threshold parsed from `clippy.toml` text, if any.
fn threshold_in_text(text: &str) -> Option<u64> {
    text.lines().find_map(value_on_line)
}

/// Active complexity limit for type aliases.
///
/// Reads the workspace `clippy.toml`; missing files or keys fall back to the
/// Clippy default of 250.
#[must_use]
pub fn complexity_threshold() -> u64 {
    if let Ok(text) = read_to_string("clippy.toml")
        && let Some(value) = threshold_in_text(&text)
    {
        return value;
    }
    DEFAULT_THRESHOLD
}

#[cfg(test)]
mod tests {
    use crate::rules_types::threshold::{threshold_in_text, value_on_line};

    #[test]
    fn parses_dashed_key() {
        assert_eq!(
            threshold_in_text("type-complexity-threshold = 300\n"),
            Some(300)
        );
    }

    #[test]
    fn parses_underscored_key_with_comment() {
        assert_eq!(
            threshold_in_text("type_complexity_threshold = 120 # local\n"),
            Some(120)
        );
    }

    #[test]
    fn rejects_missing_and_quoted() {
        assert_eq!(threshold_in_text("excessive-nesting-threshold = 3\n"), None);
        assert_eq!(value_on_line("type-complexity-threshold = \"300\""), None);
        assert_eq!(value_on_line("# type-complexity-threshold = 300"), None);
    }
}
