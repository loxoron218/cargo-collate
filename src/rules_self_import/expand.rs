//! Brace-group expansion for use-tree paths.

use std::mem::take;

/// Split `s` on top-level commas with braces respected.
#[must_use]
pub fn split_top(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        if ch == '{' {
            depth = depth.saturating_add(1);
        }
        if ch == '}' {
            depth = depth.saturating_sub(1);
        }
        if ch == ',' && depth == 0 {
            parts.push(take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    parts.push(cur);
    parts
}

/// End exclusive of the brace group opened at `open_idx`, if balanced.
fn brace_group_end(chars: &[char], open_idx: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut k = open_idx;
    while let Some(current) = chars.get(k).copied() {
        depth = depth.saturating_add(brace_step(current));
        if current == '}' && depth == 0 {
            return Some(k);
        }
        k = k.saturating_add(1);
    }
    None
}

/// Step for brace depth tracking.
const fn brace_step(c: char) -> i32 {
    if c == '{' {
        1
    } else if c == '}' {
        -1
    } else {
        0
    }
}

/// Step for bracket depth tracking.
const fn bracket_delta(c: char) -> i32 {
    if c == '[' {
        1
    } else if c == ']' {
        -1
    } else {
        0
    }
}

/// Index after a bracketed attribute opened at `i`, or EOF when unbalanced.
#[must_use]
pub fn attr_resume(chars: &[char], i: usize) -> usize {
    let n = chars.len();
    let mut depth: i32 = 0;
    for (k, c) in chars.iter().enumerate().skip(i) {
        depth = depth.saturating_add(bracket_delta(*c));
        if *c == ']' && depth == 0 {
            return k.saturating_add(1).min(n);
        }
    }
    n
}

/// Expand one level of grouped paths recursively.
pub fn expand(path: &str) -> Vec<String> {
    let Some(open) = path.find('{') else {
        return vec![path.to_owned()];
    };
    let chars: Vec<char> = path.chars().collect();
    let open_idx = path.get(..open).map_or(0, |part| part.chars().count());
    let Some(close_idx) = brace_group_end(&chars, open_idx) else {
        return vec![path.to_owned()];
    };
    let head: String = chars
        .get(..open_idx)
        .map_or_else(String::new, |part| part.iter().collect());
    let inner: String = chars
        .get(open_idx.saturating_add(1)..close_idx)
        .map_or_else(String::new, |part| part.iter().collect());
    let tail: String = chars
        .get(close_idx.saturating_add(1)..)
        .map_or_else(String::new, |part| part.iter().collect());
    let mut out = Vec::new();
    for alt in split_top(&inner) {
        if alt.trim().is_empty() {
            continue;
        }
        for mid in expand(&format!("{head}{alt}")) {
            out.extend(expand(&format!("{mid}{tail}")));
        }
    }
    out
}
