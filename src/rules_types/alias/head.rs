//! Alias-head parsing for type definitions.
//!
//! Validates the `Name<params?> =` head of a candidate alias, so prose,
//! match arms, and item bodies never resolve into a declaration.

use crate::rules_simple::skip_ws_at;

/// A declared generic parameter with its optional default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    /// Parameter name (`T`, `'a`, `N` — a `const` prefix stays stripped).
    pub name: String,
    /// Default right-hand side (`T = u32`), if any.
    pub default: Option<String>,
}

/// True if `b` can continue an ASCII identifier.
const fn is_head_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// End exclusive of the identifier starting at `i`.
fn head_ident_end(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(|b| is_head_ident(*b)) {
        i = i.saturating_add(1);
    }
    i
}

/// True if an arrow or comparator opens at `i`, swallowing the `>`.
#[must_use]
pub fn skip_arrow(bytes: &[u8], i: usize) -> Option<usize> {
    let next = bytes.get(i.saturating_add(1)).copied();
    (bytes.get(i).copied() == Some(b'-') && next == Some(b'>')
        || bytes.get(i).copied() == Some(b'=') && matches!(next, Some(b'>' | b'=')))
    .then_some(i.saturating_add(2))
}

/// End exclusive of the generic group opening at `i`, if balanced.
///
/// Arrows and comparators never affect depth, so `Fn() -> u8` bounds stay
/// intact; unbalanced groups stay skipped, keeping the lint silent.
fn generic_close(bytes: &[u8], i: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut j = i;
    while j < bytes.len() {
        if let Some(past) = skip_arrow(bytes, j) {
            j = past;
        } else if closes_group(bytes, depth, j) {
            return Some(j.saturating_add(1));
        } else {
            depth = step_generic_depth(bytes, depth, j);
            j = j.saturating_add(1);
        }
    }
    None
}

/// True if the `>` at `j` closes the group given the current `depth`.
fn closes_group(bytes: &[u8], depth: i32, j: usize) -> bool {
    bytes.get(j).copied() == Some(b'>') && depth <= 1
}

/// Depth after the bracket at `j`, if any.
fn step_generic_depth(bytes: &[u8], depth: i32, j: usize) -> i32 {
    match bytes.get(j).copied() {
        Some(b'<') => depth.saturating_add(1),
        Some(b'>') => depth.saturating_sub(1),
        Some(_) | None => depth,
    }
}

/// Resume index past the lifetime or char literal opening at `j`, if any.
///
/// A quote followed by one char and a closing quote is a char literal;
/// otherwise a lifetime spans the quote plus its identifier.
#[must_use]
pub fn squote_end(bytes: &[u8], j: usize) -> Option<usize> {
    let body = bytes.get(j.saturating_add(1)).copied()?;
    if body != b'\\' && bytes.get(j.saturating_add(2)).copied() == Some(b'\'') {
        return Some(j.saturating_add(3));
    }
    if !body.is_ascii_alphabetic() && body != b'_' {
        return None;
    }
    let mut k = j.saturating_add(2);
    while bytes
        .get(k)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        k = k.saturating_add(1);
    }
    Some(k)
}

/// Resume index past the string opening at `j`, if it terminates.
///
/// Strings stay blanked upstream, so this only fires defensively.
#[must_use]
pub fn dquote_end(bytes: &[u8], j: usize) -> Option<usize> {
    let mut k = j.saturating_add(1);
    while let Some(b) = bytes.get(k).copied() {
        if b == b'\\' {
            k = k.saturating_add(2);
        } else if b == b'"' {
            return Some(k.saturating_add(1));
        } else {
            k = k.saturating_add(1);
        }
    }
    None
}

/// Resume index past a quoted span opening at `j`, if any.
#[must_use]
pub fn skip_quoted(bytes: &[u8], j: usize) -> Option<usize> {
    match bytes.get(j).copied()? {
        b'\'' => squote_end(bytes, j),
        b'"' => dquote_end(bytes, j),
        _ => None,
    }
}

/// One scan step at `j`: resume index and updated bracket depth.
///
/// Arrows and quoted spans pass through untouched while brackets adjust
/// the depth, so `fn(u8) -> u8` bounds, lifetimes, and quoted commas or
/// brackets never disturb group matching.
#[must_use]
pub fn step_group(bytes: &[u8], j: usize, depth: i32) -> Option<(usize, i32)> {
    if let Some(past) = skip_arrow(bytes, j) {
        return Some((past, depth));
    }
    if let Some(past) = skip_quoted(bytes, j) {
        return Some((past, depth));
    }
    let next = match bytes.get(j).copied()? {
        b'<' | b'(' | b'[' | b'{' => depth.saturating_add(1),
        b'>' | b')' | b']' | b'}' => depth.saturating_sub(1),
        _ => depth,
    };
    Some((j.saturating_add(1), next))
}

/// Top-level comma arguments of a group interior.
///
/// Shares its scanning discipline with [`super::generics::angle_end`], so
/// nested groups and quoted commas (`Foo<',', u8>`) never split.
#[must_use]
pub fn split_top_level(inner: &str) -> Vec<String> {
    let bytes = inner.as_bytes();
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    let mut j = 0;
    while j < bytes.len() {
        if bytes.get(j).copied() == Some(b',') && depth <= 0 {
            parts.push(inner.get(start..j).unwrap_or("").trim().to_owned());
            start = j.saturating_add(1);
        }
        let Some((next, next_depth)) = step_group(bytes, j, depth) else {
            break;
        };
        depth = next_depth;
        j = next;
    }
    parts.push(inner.get(start..).unwrap_or("").trim().to_owned());
    parts
}

/// Leading identifier characters of `head`.
fn ident_prefix(head: &str) -> String {
    head.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// Name of one generic parameter: the leading identifier or lifetime.
///
/// A `const` prefix stays stripped; bounds never reach this point since the
/// caller splits the default off first.
fn param_name(head: &str) -> Option<String> {
    let trimmed = head.trim();
    let head = trimmed.strip_prefix("const ").map_or(trimmed, str::trim);
    head.strip_prefix('\'').map_or_else(
        || {
            let ident = ident_prefix(head);
            (!ident.is_empty() && !ident.starts_with(|c: char| c.is_ascii_digit())).then_some(ident)
        },
        |lifetime| {
            let ident = ident_prefix(lifetime);
            (!ident.is_empty()).then(|| format!("'{ident}"))
        },
    )
}

/// Name and default of one generic parameter, if parseable.
///
/// The default is the text past the first bare `=`; bounds stay ignored
/// since substitution only needs names and defaults.
fn param_parts(part: &str) -> Option<GenericParam> {
    let (head, default) = match part.split_once('=') {
        Some((before, after)) if !after.starts_with(['=', '>']) => {
            (before, Some(after.trim().to_owned()))
        }
        _ => (part, None),
    };
    let name = param_name(head)?;
    Some(GenericParam { name, default })
}

/// Declared name, generic parameters, and `=` offset of the alias head.
///
/// `head_start` is the index of `type` in `joined`. The head must hold an
/// ASCII name with optional balanced generics (defaults like `T = u32`
/// included) before the assignment; anything else — prose from a
/// line-continued string, match arms, item bodies — rejects the candidate
/// instead of misreading a later `=` or `=>` as the head. Unparseable
/// parameters reject the head too, so substitution can never misalign
/// arguments with parameters.
#[must_use]
pub fn head_parts(joined: &str, head_start: usize) -> Option<(String, Vec<GenericParam>, usize)> {
    let bytes = joined.as_bytes();
    let start = skip_ws_at(bytes, head_start.saturating_add(4));
    if bytes
        .get(start)
        .is_none_or(|b| !b.is_ascii_alphabetic() && *b != b'_')
    {
        return None;
    }
    let name_end = head_ident_end(bytes, start);
    let mut i = skip_ws_at(bytes, name_end);
    let mut params = Vec::new();
    if bytes.get(i).copied() == Some(b'<') {
        let close = generic_close(bytes, i)?;
        let inner = joined.get(i.saturating_add(1)..close.saturating_sub(1))?;
        for part in split_top_level(inner) {
            params.push(param_parts(&part)?);
        }
        i = skip_ws_at(bytes, close);
    }
    if bytes.get(i).copied() != Some(b'=') || bytes.get(i.saturating_add(1)).copied() == Some(b'>')
    {
        return None;
    }
    let name = joined.get(start..name_end)?.to_owned();
    Some((name, params, i))
}

#[cfg(test)]
mod tests {
    use crate::rules_types::alias::head::{GenericParam, head_parts};

    fn params_of(head: &str) -> Vec<GenericParam> {
        head_parts(head, 0).map_or_else(Vec::new, |(_, params, _)| params)
    }

    #[test]
    fn generic_params_parsed() {
        assert_eq!(
            params_of("type Map<K, V> = HashMap<K, V>;"),
            vec![
                GenericParam {
                    name: "K".to_owned(),
                    default: None,
                },
                GenericParam {
                    name: "V".to_owned(),
                    default: None,
                },
            ]
        );
        assert!(
            params_of("type Plain = u32;").is_empty(),
            "plain heads carry no params"
        );
    }

    #[test]
    fn defaults_lifetimes_const_parsed() {
        assert_eq!(
            params_of("type Foo<T = u32> = Vec<T>;"),
            vec![GenericParam {
                name: "T".to_owned(),
                default: Some("u32".to_owned()),
            }]
        );
        assert_eq!(
            params_of("type C<'a> = &'a str;"),
            vec![GenericParam {
                name: "'a".to_owned(),
                default: None,
            }]
        );
        assert_eq!(
            params_of("type A<const N: usize> = [u8; N];"),
            vec![GenericParam {
                name: "N".to_owned(),
                default: None,
            }]
        );
    }

    #[test]
    fn nested_groups_never_split() {
        assert_eq!(
            params_of("type F<T = Vec<u8>, U: Fn(u8) -> u8> = T;")
                .iter()
                .map(|param| param.name.clone())
                .collect::<Vec<_>>(),
            vec!["T".to_owned(), "U".to_owned()],
            "defaults and fn bounds hold commas and arrows"
        );
    }
}
