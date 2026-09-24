//! Generic argument substitution for type aliases.
//!
//! Name-only splicing leaves applied arguments dangling (`M<String, u8>`
//! with `type M<K, V> = …` scores the leftover `K`/`V` plus the trailing
//! group), which inflates use-site scores toward silence. Substituting each
//! parameter with its actual argument first scores exactly what Clippy sees
//! after removal.
use std::cmp::Reverse;

use crate::{
    lexer::word::{char_end, word_match_at},
    rules_types::alias::head::{GenericParam, step_group},
};

/// True if `b` is ASCII whitespace, newlines included.
///
/// Wraps the standard check so call sites never pass a `::` path as a
/// call argument, which the `must_use` self-lint reads as forwarding.
const fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0C' | b'\r')
}

/// Offset past ASCII whitespace and an optional turbofish `::` from `j`.
///
/// Shared opener for applied-group parsing, so the window walk and the
/// site splice never clone each other.
#[must_use]
pub fn args_open_at(text: &str, j: usize) -> usize {
    let bytes = text.as_bytes();
    let mut k = j;
    while bytes.get(k).is_some_and(|b| is_ws_byte(*b)) {
        k = k.saturating_add(1);
    }
    if text.get(k..).is_some_and(|rest| rest.starts_with("::")) {
        k = k.saturating_add(2);
        while bytes.get(k).is_some_and(|b| is_ws_byte(*b)) {
            k = k.saturating_add(1);
        }
    }
    k
}

/// Resume index past the group closing the bracket at `open`, if balanced.
///
/// Accepts any opener (`<`, `(`, `[`, `{`) with the same discipline, so
/// callers match parameter lists as well as generic groups. Overrunning
/// the text stays `None`, keeping the lint silent.
#[must_use]
pub fn angle_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if !matches!(bytes.get(open).copied(), Some(b'<' | b'(' | b'[' | b'{')) {
        return None;
    }
    let mut depth: i32 = 0;
    let mut j = open;
    while j < bytes.len() {
        let (next, next_depth) = step_group(bytes, j, depth)?;
        depth = next_depth;
        j = next;
        if depth <= 0 {
            return Some(j);
        }
    }
    None
}

/// Argument bound to the parameter matching at `i`, if any.
///
/// Longest names match first, so `T` never shadows `Table`.
fn resolve_hit<'hit>(
    rhs: &str,
    i: usize,
    resolved: &[(&'hit str, &'hit str)],
) -> Option<(&'hit str, &'hit str)> {
    resolved
        .iter()
        .find(|(name, _)| word_match_at(rhs, name, i))
        .copied()
}

/// Right-hand side with each parameter replaced by its argument.
///
/// Missing arguments fall back to declared defaults, then to an elided
/// lifetime (`'_`) for lifetime parameters, which never affect the score.
/// Anything still unmapped stays `None`, keeping the lint silent.
/// Replacement runs in a single pass with longest names first, so an
/// argument naming another parameter (`Map<V, u8>`) never cascades into it.
#[must_use]
pub fn substitute(rhs: &str, params: &[GenericParam], args: &[String]) -> Option<String> {
    let mut resolved: Vec<(&str, &str)> = Vec::new();
    for (idx, param) in params.iter().enumerate() {
        if param.name.is_empty() {
            return None;
        }
        let arg = match args.get(idx) {
            Some(text) if !text.is_empty() => text.as_str(),
            _ => match param.default.as_deref() {
                Some(dflt) => dflt,
                None if param.name.starts_with('\'') => "'_",
                None => return None,
            },
        };
        resolved.push((param.name.as_str(), arg));
    }
    resolved.sort_by_key(|item| Reverse(item.0.len()));
    let mut out = String::new();
    let mut i = 0;
    while i < rhs.len() {
        if let Some((name, arg)) = resolve_hit(rhs, i, &resolved) {
            out.push_str(arg);
            i = i.saturating_add(name.len());
            continue;
        }
        let Some(next) = char_end(rhs, i) else {
            break;
        };
        out.push_str(rhs.get(i..next).unwrap_or(""));
        i = next;
    }
    Some(out)
}

/// End offset past the applied group at `j`, or `name_end` when bare.
///
/// Skips whitespace and an optional turbofish `::` before the `<`; a bare
/// mention followed by `<` that fails to parse stays unmatched, since only
/// the caller knows whether silence or a verbatim fallback applies.
fn match_group(site: &str, name_end: usize, group: &str) -> Option<usize> {
    let j = args_open_at(site, name_end);
    if group.is_empty() {
        return (!site.get(j..).is_some_and(|rest| rest.starts_with('<'))).then_some(name_end);
    }
    site.get(j..)
        .is_some_and(|rest| rest.starts_with(group))
        .then(|| j.saturating_add(group.len()))
}

/// `site` with each matching mention replaced by its substituted `concrete`.
///
/// Applied mentions consume their whole `<…>` group so no argument dangles
/// past the splice; bare mentions replace only when no `<` follows, and
/// anything else (`Name::Assoc`, mismatched groups) stays verbatim rather
/// than scoring a half-substituted type.
#[must_use]
pub fn splice_applied(site: &str, name: &str, group: &str, concrete: &str) -> String {
    if name.is_empty() {
        return site.to_owned();
    }
    let bytes = site.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if word_match_at(site, name, i)
            && let Some(past) = match_group(site, i.saturating_add(name.len()), group)
        {
            out.push_str(concrete);
            i = past;
            continue;
        }
        let Some(next) = char_end(site, i) else {
            break;
        };
        out.push_str(site.get(i..next).unwrap_or(""));
        i = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules_types::{
        alias::{
            generics::{angle_end, splice_applied, substitute},
            head::{GenericParam, split_top_level},
        },
        score::type_score,
    };

    fn params(names: &[&str]) -> Vec<GenericParam> {
        names
            .iter()
            .map(|name| GenericParam {
                name: (*name).to_owned(),
                default: None,
            })
            .collect()
    }

    #[test]
    fn args_split_nested() {
        assert_eq!(
            split_top_level("String, Vec<u8>, fn(u8) -> u8"),
            vec![
                "String".to_owned(),
                "Vec<u8>".to_owned(),
                "fn(u8) -> u8".to_owned(),
            ]
        );
        assert_eq!(
            split_top_level("',', u8"),
            vec!["','".to_owned(), "u8".to_owned()],
            "quoted commas never split"
        );
    }

    #[test]
    fn angle_end_matches_nested() {
        assert_eq!(angle_end("Map<String, Vec<u8>>;", 3), Some(20));
        assert_eq!(angle_end("Map<String, Vec<u8>;", 3), None);
    }

    #[test]
    fn substitute_replaces_params() {
        let got = substitute(
            "HashMap<K, Vec<V>>",
            &params(&["K", "V"]),
            &["String".to_owned(), "u8".to_owned()],
        );
        assert_eq!(got.as_deref(), Some("HashMap<String, Vec<u8>>"));
    }

    #[test]
    fn substitute_needs_every_param() {
        assert!(
            substitute(
                "HashMap<K, Vec<V>>",
                &params(&["K", "V"]),
                &["String".to_owned()],
            )
            .is_none(),
            "unmapped params stay silent"
        );
        let defaulted = vec![GenericParam {
            name: "T".to_owned(),
            default: Some("u32".to_owned()),
        }];
        assert_eq!(
            substitute("Vec<T>", &defaulted, &[]).as_deref(),
            Some("Vec<u32>"),
            "defaults fill bare mentions"
        );
    }

    #[test]
    fn substitute_never_cascades() {
        let got = substitute(
            "HashMap<K, Vec<V>>",
            &params(&["K", "V"]),
            &["V".to_owned(), "u8".to_owned()],
        );
        assert_eq!(
            got.as_deref(),
            Some("HashMap<V, Vec<u8>>"),
            "arguments naming parameters stay intact"
        );
    }

    #[test]
    fn substituted_site_matches_manual_inline() {
        let site = " M<String, u8>";
        let concrete = substitute(
            "HashMap<K, HashMap<String, HashMap<String, Vec<V>>>>",
            &params(&["K", "V"]),
            &["String".to_owned(), "u8".to_owned()],
        );
        let scored =
            concrete.map(|rhs| type_score(&splice_applied(site, "M", "<String, u8>", &rhs)));
        assert_eq!(
            scored,
            Some(type_score(
                " HashMap<String, HashMap<String, HashMap<String, Vec<u8>>>>"
            )),
            "substituted use scores exactly like the manual inline"
        );
    }

    #[test]
    fn substitute_fills_elided_lifetimes() {
        let both = vec![
            GenericParam {
                name: "'a".to_owned(),
                default: None,
            },
            GenericParam {
                name: "T".to_owned(),
                default: None,
            },
        ];
        assert!(
            substitute("Foo<'a>", &both, &[]).is_none(),
            "missing type params still stay silent"
        );
        let single = vec![GenericParam {
            name: "'a".to_owned(),
            default: None,
        }];
        assert_eq!(
            substitute("&'a u8", &single, &[]).as_deref(),
            Some("&'_ u8"),
            "elided lifetimes fill without scoring"
        );
    }

    #[test]
    fn splice_applied_consumes_group() {
        assert_eq!(
            splice_applied("M<String>", "M", "<String>", "Vec<String>"),
            "Vec<String>"
        );
        assert_eq!(
            splice_applied("x: M = y;", "M", "", "u32"),
            "x: u32 = y;",
            "bare mentions replace"
        );
        assert_eq!(
            splice_applied("M::Assoc", "M", "", "Vec<u8>"),
            "Vec<u8>::Assoc",
            "path heads resolve against the substituted type"
        );
    }
}
