//! Syntactic `clippy::type_complexity` scoring.
//!
//! Mirrors the Clippy visitor without type resolution: references and raw
//! pointers add 1, named paths, slices, arrays, and tuples add `10 * nest`,
//! `fn` pointers add `50 * nest`, and `dyn` or `impl` objects add `20 * nest`
//! (`50 * nest` with `for<>` binders). Nesting is one plus the depth of open
//! `<`, `(`, and `[` delimiters, so `Vec<String>` scores 30 exactly like
//! Clippy. Path segments after `::` or `as` contribute no node of their own
//! (`a::b::c<T>` is one node plus its args), while a `<` outside generic
//! args opens a qualified-path node (`&<T as Trait>`), both matching Clippy.
//! Bare `+ 'a` region bounds never trigger the `50` case (only `for<'a>`
//! binders do, matching Clippy's `bound_generic_params` check). Bound trait
//! names inside `dyn`/`impl` objects (`Future`, `Send`) score nothing, only
//! their generic arguments do. Binding names (`Output = T`) score nothing.
//! Residual gaps stay small: `fn` return types sit one nest level low, so
//! only function-heavy aliases within a hair of the threshold could misread.

pub mod chars;
pub mod delim;
pub mod dyn_obj;
pub mod ident;

use {
    chars::is_ident_start, delim::step_delimiter, dyn_obj::is_dyn_keyword_at, ident::score_ident,
};

/// True if an identifier may open at `i` (raw identifiers included).
fn ident_opens_at(chars: &[char], i: usize) -> bool {
    chars.get(i).is_some_and(|c| {
        is_ident_start(*c) || (*c == 'r' && chars.get(i.saturating_add(1)) == Some(&'#'))
    })
}

/// Resume index after scoring the identifier at `i`, if any.
///
/// Records a `dyn`/`impl` scope before delegating to `score_ident`, so
/// bound trait names at that depth score nothing.
fn step_ident(
    chars: &[char],
    i: usize,
    depth: u64,
    dyn_top: &mut Option<u64>,
    score: &mut u64,
) -> Option<usize> {
    if !ident_opens_at(chars, i) {
        return None;
    }
    if is_dyn_keyword_at(chars, i) {
        *dyn_top = Some(depth);
    }
    score_ident(chars, i, depth, *dyn_top, score)
}

/// True if the delimiter at `i` closes the current `dyn`/`impl` bound list.
fn closes_dyn_scope(chars: &[char], i: usize, depth: u64, dyn_top: Option<u64>) -> bool {
    dyn_top.is_some_and(|top| top == depth)
        && chars
            .get(i)
            .is_some_and(|c| matches!(c, ',' | ';' | '>' | ')' | ']' | '{' | '}' | '|'))
}

/// Resume index and depth after the delimiter at `i`, if any.
///
/// Clears a closed `dyn`/`impl` scope so later identifiers score normally.
fn step_delim(
    chars: &[char],
    i: usize,
    depth: u64,
    dyn_top: &mut Option<u64>,
    score: &mut u64,
) -> Option<(usize, u64)> {
    let (next, next_depth) = step_delimiter(chars, i, depth, score)?;
    if closes_dyn_scope(chars, i, depth, *dyn_top) {
        *dyn_top = None;
    }
    Some((next, next_depth))
}

/// Complexity score of a type-alias right-hand side.
///
/// Expects code with strings blanked and line comments cut; block comments and
/// `where` clauses should already be removed by the caller.
#[must_use]
pub fn type_score(rhs: &str) -> u64 {
    let chars: Vec<char> = rhs.chars().collect();
    let mut score: u64 = 0;
    let mut depth: u64 = 0;
    let mut dyn_top: Option<u64> = None;
    let mut i = 0;
    while i < chars.len() {
        if let Some(next) = step_ident(&chars, i, depth, &mut dyn_top, &mut score) {
            i = next;
            continue;
        }
        if let Some((next, next_depth)) = step_delim(&chars, i, depth, &mut dyn_top, &mut score) {
            i = next;
            depth = next_depth;
            continue;
        }
        i = i.saturating_add(1);
    }
    score
}

#[cfg(test)]
mod tests {
    use crate::rules_types::score::{chars::is_skipped_word, type_score};

    #[test]
    fn simple_paths_score_low() {
        assert_eq!(type_score("u32"), 10);
        assert_eq!(type_score("Vec < String >"), 30);
        assert_eq!(type_score("& str"), 11);
    }

    #[test]
    fn nesting_multiplies() {
        assert_eq!(type_score("HashMap < String , Vec < u8 > >"), 80);
    }

    #[test]
    fn nested_closes_score_flat() {
        assert_eq!(type_score("(Vec<Vec<u8>>, u8)"), 120);
    }

    #[test]
    fn fn_pointer_scores_fifty() {
        assert_eq!(type_score("fn ( u8 ) -> String"), 80);
    }

    #[test]
    fn dyn_object_scores_twenty() {
        assert_eq!(type_score("dyn Display + Send"), 20);
        assert_eq!(
            type_score("dyn Display + Send + 'static"),
            20,
            "bare region bounds never trigger the fifty case"
        );
    }

    #[test]
    fn dyn_for_binder_scores_fifty() {
        assert_eq!(type_score("dyn for<'a> Fn(&'a str)"), 71);
    }

    #[test]
    fn paths_and_chains_score_once() {
        assert_eq!(type_score("std::vec::Vec<u8>"), 30);
        assert_eq!(type_score("<I as Iterator>::Item"), 30);
        assert_eq!(type_score("Vec<u8>::IntoIter"), 30);
    }

    #[test]
    fn qualified_fn_param_scores_low() {
        assert_eq!(
            type_score("Filter<I, fn(&<I as Iterator>::Item) -> bool>"),
            221
        );
    }

    #[test]
    fn complex_nesting_exceeds_default() {
        let rhs = "HashMap < String , HashMap < String , HashMap < String , HashMap < String , \
                   Vec < u8 > > > > >";
        assert!(
            type_score(rhs) > 250,
            "deeply nested RHS stays above threshold"
        );
    }

    #[test]
    fn async_fn_traits_attach_params() {
        assert_eq!(
            type_score("AsyncFn ( u8 ) -> String"),
            type_score("Fn ( u8 ) -> String"),
            "`AsyncFn` params attach like `Fn`"
        );
        assert_eq!(
            type_score("dyn AsyncFn ( u8 )"),
            type_score("dyn Fn ( u8 )"),
            "`dyn AsyncFn` scores like `dyn Fn`"
        );
    }

    #[test]
    fn skipped_keywords_never_score() {
        for word in [
            "async", "await", "union", "try", "yield", "gen", "move", "while", "loop", "match",
            "return", "box",
        ] {
            assert!(
                is_skipped_word(word),
                "`{word}` never denotes a scored type"
            );
        }
    }
}
