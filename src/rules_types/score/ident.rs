//! Identifier scoring for complexity checks.

use crate::rules_types::score::{
    chars::{
        continues_chain, follows_as, ident_word_at, is_numeric, is_skipped_word, nest_at, skip_ws,
    },
    dyn_obj::dyn_has_lifetime,
};

/// Score update and resume index for the identifier at `i`, if any.
///
/// `dyn_top` holds the depth of an enclosing `dyn`/`impl` object whose
/// bounds are being scanned, if any. Bound trait names at that depth
/// (`Future`, `Send` in `dyn Future + Send`) denote no type of their own,
/// matching Clippy which visits bounds as trait refs rather than types;
/// only their generic arguments (at deeper depths) score.
pub fn score_ident(
    chars: &[char],
    i: usize,
    depth: u64,
    dyn_top: Option<u64>,
    score: &mut u64,
) -> Option<usize> {
    let (word, end) = ident_word_at(chars, i)?;
    if word == "fn" {
        let next = skip_ws(chars, end);
        if next.is_some_and(|j| chars.get(j) == Some(&'(') || chars.get(j) == Some(&'<')) {
            *score = score.saturating_add(50_u64.saturating_mul(nest_at(depth)));
        }
        return Some(end);
    }
    if word == "dyn" || word == "impl" {
        let add = if dyn_has_lifetime(chars, end) {
            50_u64.saturating_mul(nest_at(depth))
        } else {
            20_u64.saturating_mul(nest_at(depth))
        };
        *score = score.saturating_add(add);
        return Some(end);
    }
    if is_skipped_word(&word) || is_numeric(&word) {
        return Some(end);
    }
    if follows_as(chars, i) || continues_chain(chars, i) {
        return Some(end);
    }
    if is_assoc_binding(chars, end) {
        return Some(end);
    }
    if dyn_top.is_some_and(|top| top == depth) {
        return Some(end);
    }
    *score = score.saturating_add(10_u64.saturating_mul(nest_at(depth)));
    Some(end)
}

/// True if `=` opens an associated binding after `end` (whitespace skipped).
///
/// Binding names (`Output` in `Future<Output = T>`) denote no type of their
/// own, matching Clippy which never visits them.
fn is_assoc_binding(chars: &[char], end: usize) -> bool {
    let mut j = end;
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j = j.saturating_add(1);
    }
    chars.get(j).copied() == Some('=')
}
