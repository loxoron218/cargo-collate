//! `dyn` and `impl` object scoring for complexity checks.

use crate::rules_types::score::chars::{ident_word_at, is_ident_char, is_ident_start};

/// True if a `for<>` binder opens inside the `dyn` or `impl` bounds from `i`.
///
/// Scans to the next top-level `,`, `;`, `>`, `)`, `]`, `}`, or `|` for a
/// `for<...>` binder holding a lifetime, matching Clippy's
/// `bound_generic_params` check. Bare region bounds (`+ 'a`) never count.
#[must_use]
pub fn dyn_has_lifetime(chars: &[char], i: usize) -> bool {
    let mut j = i;
    let mut depth: u64 = 0;
    while let Some(c) = chars.get(j).copied() {
        depth = step_dyn_depth(depth, c);
        if depth == 0 && matches!(c, ',' | ';' | '}' | '|' | '{') {
            break;
        }
        if depth == 0 && is_for_at(chars, j) && for_binder_has_lifetime(chars, j) {
            return true;
        }
        j = j.saturating_add(1);
    }
    false
}

/// True if `for` opens at `j` with identifier boundaries.
fn is_for_at(chars: &[char], j: usize) -> bool {
    let want = ['f', 'o', 'r'];
    if chars.get(j..j.saturating_add(3)) != Some(&want) {
        return false;
    }
    let before_ok = j == 0
        || chars
            .get(j.saturating_sub(1))
            .is_none_or(|c| !is_ident_char(*c));
    let after = j.saturating_add(3);
    let after_ok = chars.get(after).is_none_or(|c| !is_ident_char(*c));
    if !(before_ok && after_ok) {
        return false;
    }
    let mut k = after;
    while chars.get(k).is_some_and(|c| c.is_whitespace()) {
        k = k.saturating_add(1);
    }
    chars.get(k).copied() == Some('<')
}

/// True if the `for<...>` group at `j` holds a lifetime marker.
fn for_binder_has_lifetime(chars: &[char], j: usize) -> bool {
    let mut k = j.saturating_add(3);
    while chars.get(k).is_some_and(|c| c.is_whitespace()) {
        k = k.saturating_add(1);
    }
    if chars.get(k).copied() != Some('<') {
        return false;
    }
    let mut depth: u64 = 0;
    k = k.saturating_add(1);
    while let Some(c) = chars.get(k).copied() {
        match c {
            '<' => depth = depth.saturating_add(1),
            '>' if depth == 0 => break,
            '>' => depth = depth.saturating_sub(1),
            '\'' if chars
                .get(k.saturating_add(1))
                .is_some_and(|n| is_ident_start(*n)) =>
            {
                return true;
            }
            _ => {}
        }
        k = k.saturating_add(1);
    }
    false
}

/// Track generic delimiter depth for the lifetime scan.
const fn step_dyn_depth(depth: u64, c: char) -> u64 {
    match c {
        '<' | '(' | '[' => depth.saturating_add(1),
        '>' | ')' | ']' => depth.saturating_sub(1),
        _ => depth,
    }
}

/// True if `dyn` or `impl` opens at `i` with identifier boundaries.
#[must_use]
pub fn is_dyn_keyword_at(chars: &[char], i: usize) -> bool {
    let Some((word, _)) = ident_word_at(chars, i) else {
        return false;
    };
    word == "dyn" || word == "impl"
}
