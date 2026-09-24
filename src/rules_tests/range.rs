//! Test-range scanning for cfg-test modules.

use crate::{
    lexer::{
        literal::{raw_open_span, strip_strings},
        marker::cut_line_comment,
        word::has_word,
    },
    rules_simple::is_real_cfg_line,
};

/// Brace-scan state carried across lines for multi-line literals.
///
/// Single-line blanking miscounts braces on the closing line of a multi-line
/// raw string (such as the `}"#;` ending a JSON fixture) and inside block
/// comments, which closed test-module ranges early and flagged test code as
/// live library code.
#[derive(Default)]
struct SpanState {
    /// Hash count of the raw string still open after the previous line, if any.
    raw_hashes: Option<usize>,
    /// True if a quoted string continues past the previous line ending.
    quoted_cont: bool,
    /// Nesting depth of block comments still open after the previous line.
    block_depth: i32,
}

/// Strip strings then cut the line tail.
#[must_use]
pub fn code_of(line: &str) -> String {
    cut_line_comment(&strip_strings(line)).to_owned()
}

/// Count occurrences of `brace` in stripped `code`.
#[must_use]
pub fn brace_count(code: &str, brace: char) -> i32 {
    let count = code.chars().filter(|c| *c == brace).count();
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Net open-minus-close count of `line` after stripping.
#[must_use]
pub fn brace_delta(line: &str) -> i32 {
    let code = code_of(line);
    brace_count(&code, '{')
        .checked_sub(brace_count(&code, '}'))
        .unwrap_or(0)
}

/// True if `line` may sit between a cfg-test marker and its module.
#[must_use]
pub fn stray_gap(line: &str) -> bool {
    line.trim().is_empty() || (line.trim_start().starts_with('#') && !line.contains("mod"))
}

/// True if `line` may sit between markers in the lenient flavor.
///
/// Doc lines between marker and module stay allowed.
#[must_use]
pub fn anyhow_gap(line: &str) -> bool {
    let trimmed = line.trim_start();
    line.trim().is_empty()
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || (trimmed.starts_with('#') && !line.contains("mod"))
}

/// Index just past the terminator opening at `j`, if `"` there closes the span.
///
/// A terminator is a `"` followed by exactly the opening hash count.
fn raw_close_at(chars: &[char], j: usize, hashes: usize) -> Option<usize> {
    if chars.get(j).is_none_or(|c| *c != '"') {
        return None;
    }
    let mut k = j.saturating_add(1);
    let mut seen = 0;
    while seen < hashes && chars.get(k).is_some_and(|c| *c == '#') {
        k = k.saturating_add(1);
        seen = seen.saturating_add(1);
    }
    (seen == hashes).then_some(k)
}

/// Index just past the raw-string terminator from `from`, if it closes here.
fn raw_close(chars: &[char], from: usize, hashes: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if let Some(past) = raw_close_at(chars, j, hashes) {
            return Some(past);
        }
        j = j.saturating_add(1);
    }
    None
}

/// Index just past the closing quote from `from`, if the string ends here.
///
/// Backslash escapes stay skipped so an escaped quote never ends the string.
#[must_use]
pub fn quoted_close(chars: &[char], from: usize) -> Option<usize> {
    let mut j = from;
    while let Some(c) = chars.get(j).copied() {
        if c == '"' {
            return Some(j.saturating_add(1));
        }
        j = j.saturating_add(1);
        if c == '\\' {
            j = j.saturating_add(1);
        }
    }
    None
}

/// Resume index past the raw string opening at `i`.
///
/// A same-line terminator is skipped; otherwise the hashes persist in `state`
/// and the rest of the line counts as string content.
fn skip_raw_open(state: &mut SpanState, chars: &[char], i: usize) -> usize {
    let Some((hashes, start)) = raw_open_span(chars, i) else {
        return i.saturating_add(1);
    };
    if let Some(past) = raw_close(chars, start, hashes) {
        return past;
    }
    state.raw_hashes = Some(hashes);
    chars.len()
}

/// Resume index past the escape starting at the backslash `j`.
fn skip_char_escape(chars: &[char], j: usize) -> usize {
    let after = j.saturating_add(1);
    if chars.get(after) == Some(&'u') && chars.get(after.saturating_add(1)) == Some(&'{') {
        let mut k = after.saturating_add(2);
        while chars.get(k).is_some_and(|c| *c != '}') {
            k = k.saturating_add(1);
        }
        return k.saturating_add(1).min(chars.len());
    }
    let past = after.saturating_add(1);
    let past = if chars.get(past) == Some(&'\'') {
        past.saturating_add(1)
    } else {
        past
    };
    past.min(chars.len())
}

/// Resume index past the char literal or lifetime starting at `i`.
///
/// Char literals (including escapes and `'\u{...}'`) are skipped whole so
/// braces like `'}'` never count; lifetimes and stray quotes advance one step.
fn skip_char_or_lifetime(chars: &[char], i: usize) -> usize {
    let j = i.saturating_add(1);
    if chars.get(j) == Some(&'\\') {
        return skip_char_escape(chars, j);
    }
    if chars.get(j).is_some() && chars.get(j.saturating_add(1)) == Some(&'\'') {
        return j.saturating_add(2);
    }
    i.saturating_add(1)
}

/// Resume index after block-comment spans continued from `from`.
///
/// Nested openers increment and closers decrement `state`; scanning resumes
/// in code once the depth returns to zero.
fn skip_block_tail(state: &mut SpanState, chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() && state.block_depth > 0 {
        let open = chars.get(j) == Some(&'/') && chars.get(j.saturating_add(1)) == Some(&'*');
        let close = chars.get(j) == Some(&'*') && chars.get(j.saturating_add(1)) == Some(&'/');
        if open {
            state.block_depth = state.block_depth.saturating_add(1);
            j = j.saturating_add(2);
        } else if close {
            state.block_depth = state.block_depth.saturating_sub(1);
            j = j.saturating_add(2);
        } else {
            j = j.saturating_add(1);
        }
    }
    j
}

/// Resume index into `chars` after spans continued from the previous line.
///
/// Updates `state` when a continued raw string or quoted string closes on
/// this line.
fn resume_spans(state: &mut SpanState, chars: &[char]) -> usize {
    if let Some(hashes) = state.raw_hashes {
        if let Some(past) = raw_close(chars, 0, hashes) {
            state.raw_hashes = None;
            return past;
        }
        return chars.len();
    }
    if state.quoted_cont {
        if let Some(past) = quoted_close(chars, 0) {
            state.quoted_cont = false;
            return past;
        }
        return chars.len();
    }
    0
}

/// Resume index past the quoted string at `i`, or `None` to end the line.
///
/// An unterminated string continues past the line ending for backslash
/// continuations, so `state` records the spillover.
fn scan_quoted(state: &mut SpanState, chars: &[char], i: usize, c: char) -> Option<usize> {
    let quote = i.saturating_add(usize::from(c == 'b')).saturating_add(1);
    if let Some(past) = quoted_close(chars, quote) {
        return Some(past);
    }
    state.quoted_cont = true;
    None
}

/// Resume index after the token at `i`, or `None` to end the line.
///
/// Braces in code adjust `delta`; literals, comments, and sigils advance
/// past their spans without counting.
fn span_step(state: &mut SpanState, chars: &[char], i: usize, delta: &mut i32) -> Option<usize> {
    if state.block_depth > 0 {
        return Some(skip_block_tail(state, chars, i));
    }
    let c = chars.get(i).copied()?;
    let next = chars.get(i.saturating_add(1)).copied();
    if c == '/' && next == Some('/') {
        return None;
    }
    if c == '/' && next == Some('*') {
        state.block_depth = state.block_depth.saturating_add(1);
        return Some(i.saturating_add(2));
    }
    if c == '"' || (c == 'b' && next == Some('"')) {
        return scan_quoted(state, chars, i, c);
    }
    if c == 'r' || (c == 'b' && next == Some('r')) {
        return Some(skip_raw_open(state, chars, i));
    }
    if c == '\'' {
        return Some(skip_char_or_lifetime(chars, i));
    }
    match c {
        '{' => *delta = delta.saturating_add(1),
        '}' => *delta = delta.saturating_sub(1),
        _ => {}
    }
    Some(i.saturating_add(1))
}

/// Net open-minus-close brace count of `line`, continuing `state` across lines.
///
/// Behaves like [`brace_delta`] on ordinary lines, but raw strings, quoted
/// strings, char literals, and block comments never contribute braces even
/// when they span lines, so multi-line fixtures never unbalance range scans.
fn spanning_brace_delta(state: &mut SpanState, line: &str) -> i32 {
    let chars: Vec<char> = line.chars().collect();
    let mut delta: i32 = 0;
    let mut i = resume_spans(state, &chars);
    while i < chars.len() {
        let Some(next) = span_step(state, &chars, i, &mut delta) else {
            break;
        };
        i = next;
    }
    delta
}

/// Test-module range starting at `i`, if any.
///
/// Returns zero-based start and end spans; `gap` skips filler lines.
fn test_range_from(lines: &[String], i: usize, gap: fn(&str) -> bool) -> Option<(usize, usize)> {
    if lines.get(i).is_none_or(|line| !is_real_cfg_line(line)) {
        return None;
    }
    let mut j = i.saturating_add(1);
    while lines.get(j).is_some_and(|line| gap(line)) {
        j = j.saturating_add(1);
        if j > i.saturating_add(6) {
            break;
        }
    }
    if j >= lines.len()
        || lines.get(j).is_none_or(|line| !has_word(line, "mod"))
        || lines.get(j).is_none_or(|line| !line.contains('{'))
    {
        return None;
    }
    let mut depth: i32 = 0;
    let mut state = SpanState::default();
    let mut k = j;
    while let Some(line) = lines.get(k) {
        depth = depth.saturating_add(spanning_brace_delta(&mut state, line));
        k = k.saturating_add(1);
        if depth <= 0 && k > j.saturating_add(1) {
            break;
        }
    }
    Some((j, k))
}

/// Test-module ranges as zero-based spans.
pub fn test_ranges(lines: &[String], gap: fn(&str) -> bool) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some((start, end)) = test_range_from(lines, i, gap) {
            ranges.push((start, end));
            i = end;
        } else {
            i = i.saturating_add(1);
        }
    }
    ranges
}

/// True if zero-based line `ln` sits inside a test range.
#[must_use]
pub fn in_ranges(ranges: &[(usize, usize)], ln: usize) -> bool {
    ranges.iter().any(|(a, b)| *a <= ln && ln < *b)
}
