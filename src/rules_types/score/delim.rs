//! Delimiter stepping for complexity scoring.

use crate::rules_types::score::chars::{ident_end, is_qualified_open, nest_at, prev_word};

/// Resume index after the lifetime at `i`, if any.
#[must_use]
pub fn skip_lifetime(chars: &[char], i: usize) -> Option<usize> {
    let body = i.saturating_add(1);
    let past = ident_end(chars, body);
    (chars.get(i) == Some(&'\'') && past > body).then_some(past)
}

/// True if `<` at `i` opens generics rather than an operator.
#[must_use]
pub fn is_generic_open(chars: &[char], i: usize) -> bool {
    let next = chars.get(i.saturating_add(1)).copied();
    !matches!(next, Some('<' | '=' | '-'))
}

/// True if `>` at `i` closes generics rather than an arrow or comparator.
///
/// Arrows (`->`, `=>`) and comparators (`>=`) never close; `>>` runs
/// close one level per bracket, exactly like Clippy parses them.
#[must_use]
pub fn is_generic_close(chars: &[char], i: usize) -> bool {
    let prev = if i > 0 {
        chars.get(i.saturating_sub(1)).copied()
    } else {
        None
    };
    let next = chars.get(i.saturating_add(1)).copied();
    !matches!(prev, Some('-' | '=')) && !matches!(next, Some('='))
}

/// Step one delimiter; returns the updated depth.
///
/// `(` after a `Fn`-family name (`fn`, `Fn`, `FnMut`, `FnOnce`, `AsyncFn`,
/// `AsyncFnMut`, `AsyncFnOnce` per the nightly std trait index) opens that
/// trait's parameter list, which stays part of the scored type instead of
/// opening a separate tuple node.
pub fn step_delimiter(
    chars: &[char],
    i: usize,
    depth: u64,
    score: &mut u64,
) -> Option<(usize, u64)> {
    match chars.get(i).copied()? {
        '&' | '*' => {
            *score = score.saturating_add(1);
            Some((i.saturating_add(1), depth))
        }
        '<' if is_generic_open(chars, i) => {
            if is_qualified_open(chars, i) {
                *score = score.saturating_add(10_u64.saturating_mul(nest_at(depth)));
            }
            Some((i.saturating_add(1), depth.saturating_add(1)))
        }
        '>' if is_generic_close(chars, i) => Some((i.saturating_add(1), depth.saturating_sub(1))),
        '(' => {
            let next = depth.saturating_add(1);
            let owned = prev_word(chars, i);
            let is_fn_params = owned.as_deref().is_some_and(|prev| {
                matches!(
                    prev,
                    "fn" | "Fn" | "FnMut" | "FnOnce" | "AsyncFn" | "AsyncFnMut" | "AsyncFnOnce"
                )
            });
            if !is_fn_params {
                *score = score.saturating_add(10_u64.saturating_mul(nest_at(depth)));
            }
            Some((i.saturating_add(1), next))
        }
        ')' | ']' => Some((i.saturating_add(1), depth.saturating_sub(1))),
        '[' => {
            *score = score.saturating_add(10_u64.saturating_mul(nest_at(depth)));
            Some((i.saturating_add(1), depth.saturating_add(1)))
        }
        '\'' => skip_lifetime(chars, i).map(|next| (next, depth)),
        _ => None,
    }
}
