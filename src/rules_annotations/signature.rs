//! Signature parsing for parameter lists and return types.
//!
//! Bracket-aware helpers shared by detectors that inspect `fn` signatures.

use crate::lexer::word::has_word;

/// True if `c` opens a bracket level.
///
/// Angle brackets stay out: `->`, `=>`, and comparisons hold unbalanced `>`.
#[must_use]
pub const fn is_bracket_opener(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

/// True if `c` closes a bracket level.
///
/// Angle brackets stay out: `->`, `=>`, and comparisons hold unbalanced `>`.
#[must_use]
pub const fn is_bracket_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

/// Bracket depth after scanning `code`, plus true on a top-level `{` or `;`.
///
/// Terminators inside brackets do not count, so array lengths like `[T; N]`
/// and struct patterns keep joining.
#[must_use]
pub fn scan_depth(code: &str, mut depth: i32) -> (i32, bool) {
    for c in code.chars() {
        if matches!(c, '(' | '[') {
            depth = depth.saturating_add(1);
            continue;
        }
        if matches!(c, ')' | ']') {
            depth = depth.saturating_sub(1);
            continue;
        }
        if c == '{' && depth > 0 {
            depth = depth.saturating_add(1);
            continue;
        }
        if c == '}' {
            depth = depth.saturating_sub(1);
            continue;
        }
        if (c == '{' || c == ';') && depth <= 0 {
            return (depth, true);
        }
    }
    (depth, false)
}

/// Depths after consuming `c`, plus true on a top-level comma.
///
/// Angle depth rides alongside so generic commas stay inside; a `>` with no
/// open `<` (as in `->`) never closes anything. `prev` guards the `->`
/// arrow: only a `>` following another character may close an angle span.
const fn step_depths(c: char, prev: char, depth: i32, angle: i32) -> (i32, i32, bool) {
    if c == '<' {
        return (depth, angle.saturating_add(1), false);
    }
    if c == '>' && prev != '-' && angle > 0 {
        return (depth, angle.saturating_sub(1), false);
    }
    if is_bracket_opener(c) {
        return (depth.saturating_add(1), angle, false);
    }
    if is_bracket_closer(c) {
        return (depth.saturating_sub(1), angle, false);
    }
    (depth, angle, c == ',' && depth <= 0 && angle <= 0)
}

/// Parameter list of `sig` between the first `(` and its match, if any.
pub fn params_of(sig: &str) -> Option<String> {
    let open = sig.find('(')?;
    let mut depth: i32 = 0;
    let mut angle: i32 = 0;
    let mut prev = ' ';
    for (k, c) in sig.get(open..)?.char_indices() {
        let stepped = step_depths(c, prev, depth, angle);
        depth = stepped.0;
        angle = stepped.1;
        prev = c;
        if depth <= 0 {
            return sig
                .get(open.saturating_add(1)..open.saturating_add(k))
                .map(str::to_owned);
        }
    }
    None
}

/// Split `params` on top-level commas.
#[must_use]
pub fn split_params(params: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut angle: i32 = 0;
    let mut prev = ' ';
    let mut start = 0;
    for (k, c) in params.char_indices() {
        let stepped = step_depths(c, prev, depth, angle);
        depth = stepped.0;
        angle = stepped.1;
        prev = c;
        if stepped.2 {
            out.push(params.get(start..k).unwrap_or(""));
            start = k.saturating_add(1);
        }
    }
    out.push(params.get(start..).unwrap_or(""));
    out
}

/// Return-type fragment of `sig` after `->`, if any.
///
/// Cuts the body opener and trailing semicolons so only the type remains.
#[must_use]
pub fn return_tail(sig: &str) -> Option<String> {
    let arrow = sig.find("->")?;
    let mut tail = sig
        .get(arrow.saturating_add(2)..)
        .unwrap_or("")
        .trim_start()
        .to_owned();
    if let Some(end) = tail.find(['{', ';']) {
        tail.truncate(end);
    }
    Some(tail)
}

/// True if `sig` returns unit or never, or has no return arrow.
#[must_use]
pub fn sig_returns_unit(sig: &str) -> bool {
    let Some(tail) = return_tail(sig) else {
        return true;
    };
    let tail = tail.trim_start();
    tail.is_empty() || tail.starts_with("()") || tail.starts_with('!')
}

/// Declared name of one signature piece, excluding receivers and wildcards.
pub fn param_name_of(piece: &str) -> Option<String> {
    if has_word(piece, "self") {
        return None;
    }
    let trimmed = piece.trim();
    if trimmed.is_empty() {
        return None;
    }
    let named = trimmed
        .strip_prefix("mut")
        .filter(|rest| rest.starts_with(|c: char| c.is_whitespace()))
        .map_or(trimmed, str::trim_start);
    if named == "_" || named.starts_with("_:") {
        return None;
    }
    let colon = trimmed.find(':')?;
    let before = trimmed.get(..colon)?;
    let mut rest = before.trim();
    loop {
        let clean = rest.trim_start();
        let next = clean
            .strip_prefix("mut")
            .filter(|r| r.is_empty() || r.starts_with(|c: char| c.is_whitespace()))
            .or_else(|| {
                clean
                    .strip_prefix("ref")
                    .filter(|r| r.is_empty() || r.starts_with(|c: char| c.is_whitespace()))
            });
        let Some(following) = next else {
            rest = clean;
            break;
        };
        rest = following.trim_start();
    }
    let token = rest.split_whitespace().last().unwrap_or("");
    let token = token.strip_prefix("r#").unwrap_or(token);
    if token.is_empty() || token == "_" {
        return None;
    }
    let first = token.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(token.to_owned())
}

/// Parameter names declared by `sig`, excluding receivers and wildcards.
#[must_use]
pub fn sig_param_names(sig: &str) -> Vec<String> {
    let Some(params) = params_of(sig) else {
        return Vec::new();
    };
    split_params(&params)
        .iter()
        .filter_map(|piece| param_name_of(piece))
        .collect()
}
