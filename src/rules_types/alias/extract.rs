//! Enclosing-type extraction for alias use sites.
//!
//! Builds a stripped code window around each mention and walks both
//! directions to the enclosing type expression, so verdicts score what
//! Clippy would see after removal rather than the alias alone.

use crate::{
    lexer::word::word_match_at,
    rules_tests::range::code_of,
    rules_types::alias::{
        generics::{angle_end, args_open_at},
        head::split_top_level,
    },
};

/// Lines of lookbehind around a use site when building its window.
const USE_BACK: usize = 8;

/// Lines of lookahead around a use site when building its window.
const USE_FWD: usize = 16;

/// Byte offsets of whole-word `name` mentions in stripped `code`.
#[must_use]
pub fn word_offsets(code: &str, name: &str) -> Vec<usize> {
    if name.is_empty() || code.len() < name.len() {
        return Vec::new();
    }
    let last = code.len().saturating_sub(name.len());
    (0..=last)
        .filter(|&i| word_match_at(code, name, i))
        .collect()
}

/// Stripped code window around a use plus absolute offset and bounds.
///
/// Returns the window text, the absolute use offset, the first window line,
/// and the last window line for inconclusive-edge detection downstream.
fn use_window(lines: &[String], li: usize, pos: usize) -> (String, usize, usize, usize) {
    let from = li.saturating_sub(USE_BACK);
    let mut window = String::new();
    if let Some(back) = lines.get(from..li) {
        for line in back {
            window.push_str(&code_of(line));
            window.push('\n');
        }
    }
    let abs = window.len().saturating_add(pos);
    let last = li
        .saturating_add(USE_FWD)
        .min(lines.len().saturating_sub(1));
    if let Some(fwd) = lines.get(li..last.saturating_add(1)) {
        for line in fwd {
            window.push_str(&code_of(line));
            window.push('\n');
        }
    }
    (window, abs, from, last)
}

/// Previous non-whitespace byte before `j`, if any.
fn prev_code_byte(bytes: &[u8], j: usize) -> Option<u8> {
    let mut k = j;
    while k > 0 {
        k = k.saturating_sub(1);
        if let Some(b) = bytes.get(k).copied()
            && !b.is_ascii_whitespace()
        {
            return Some(b);
        }
    }
    None
}

/// True if the `>` at `j` continues `=>` with the operator before it.
fn is_fat_arrow(bytes: &[u8], j: usize) -> bool {
    prev_code_byte(bytes, j).is_some_and(|b| b == b'=')
}

/// True if the `>` at `j` continues `->` with the operator before it.
fn is_thin_arrow(bytes: &[u8], j: usize) -> bool {
    prev_code_byte(bytes, j).is_some_and(|b| b == b'-')
}

/// True if the arrow at `j` sits inside generics.
///
/// Scans left past balanced groups; an unmatched `<` means sugar like
/// `Vec<fn(u8) -> X>`, while a delimiter first means return position like
/// `fn f() -> X`, where the type starts after the arrow.
fn arrow_in_generics(bytes: &[u8], j: usize) -> bool {
    let mut depth: i32 = 0;
    let mut k = j;
    while k > 0 {
        k = k.saturating_sub(1);
        match bytes.get(k).copied() {
            Some(b'<') if depth <= 0 => return true,
            Some(b'>' | b')' | b']' | b'}') => depth = depth.saturating_add(1),
            Some(b'(' | b'[' | b'{') if depth > 0 => {
                depth = depth.saturating_sub(1);
            }
            Some(b';') if depth <= 0 => return false,
            Some(_) | None => {}
        }
    }
    false
}

/// True if the colons ending at `j` form a `::` pair.
fn is_colon_pair(bytes: &[u8], j: usize) -> bool {
    bytes.get(j).copied() == Some(b':')
        && j > 0
        && bytes.get(j.saturating_sub(1)).copied() == Some(b':')
}

/// Updated depths moving left over a group opener.
///
/// Openers with outstanding debt match sibling groups; the rest head groups
/// containing the use and raise the nesting for the rightward walk.
const fn step_open_left(depth: i32, debt: i32) -> (i32, i32) {
    if debt > 0 {
        (depth, debt.saturating_sub(1))
    } else {
        (depth.saturating_add(1), debt)
    }
}

/// True if `(` at `j` opens call-like arguments rather than a type group.
///
/// A paren preceded by a name (`Source(`, `foo(`) wraps a variant field or
/// a value whose type stands alone past the paren; a bare paren instead
/// groups tuple types. Function traits keep their parameter lists as part
/// of the scored type: complete `Fn` family from
/// `doc.rust-lang.org/nightly/std/all.html` `#traits`
/// (nightly 1.100.0): `fn`, `Fn`, `FnMut`, `FnOnce`, `AsyncFn`,
/// `AsyncFnMut`, `AsyncFnOnce`.
fn is_call_paren(bytes: &[u8], j: usize) -> bool {
    let mut k = j;
    while k > 0
        && bytes
            .get(k.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        k = k.saturating_sub(1);
    }
    let mut start = k;
    while start > 0
        && bytes
            .get(start.saturating_sub(1))
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        start = start.saturating_sub(1);
    }
    if start >= k {
        return false;
    }
    let word = bytes.get(start..k).unwrap_or(&[]);
    !matches!(
        word,
        &[b'f', b'n']
            | &[b'F', b'n']
            | &[b'F', b'n', b'M', b'u', b't']
            | &[b'F', b'n', b'O', b'n', b'c', b'e']
            | &[b'A', b's', b'y', b'n', b'c', b'F', b'n']
            | &[b'A', b's', b'y', b'n', b'c', b'F', b'n', b'M', b'u', b't']
            | &[
                b'A', b's', b'y', b'n', b'c', b'F', b'n', b'O', b'n', b'c', b'e',
            ]
    )
}

/// Start index of the enclosing type before absolute offset `abs`.
///
/// Extends back to the nearest hard delimiter (`:`, `=`, `;`, `{`, `}`)
/// or return arrow; `::` pairs stay skipped while `=>` and
/// in-generics `->` stay included. A `Name(` opener ends the walk so
/// variant fields and call arguments score as their own type instead of
/// bleeding across comma-separated neighbors; bare parens still group
/// tuple types. Over-grabbing past these stops only inflates the score,
/// which errs silent. Also returns the nesting depth for the rightward
/// walk.
fn type_span_start(window: &str, abs: usize) -> (usize, i32) {
    let bytes = window.as_bytes();
    let mut i = abs;
    let mut depth: i32 = 0;
    let mut debt: i32 = 0;
    while i > 0 {
        let j = i.saturating_sub(1);
        match bytes.get(j).copied() {
            Some(b';' | b'{' | b'=') if debt <= 0 => break,
            Some(b'{') => {
                debt = debt.saturating_sub(1);
                i = j;
            }
            Some(b':') if !is_colon_pair(bytes, j) => break,
            Some(b'(') if debt <= 0 && depth <= 0 && is_call_paren(bytes, j) => break,
            Some(b'<' | b'(' | b'[') => {
                (depth, debt) = step_open_left(depth, debt);
                i = j;
            }
            Some(b'>') if is_fat_arrow(bytes, j) => {
                i = j;
            }
            Some(b'>') if is_thin_arrow(bytes, j) && arrow_in_generics(bytes, j) => {
                i = j;
            }
            Some(b'>') if is_thin_arrow(bytes, j) => break,
            Some(b'>' | b')' | b']' | b'}') => {
                debt = debt.saturating_add(1);
                i = j;
            }
            Some(_) | None => i = j,
        }
    }
    (i, depth)
}

/// Resume index moving right over the colon at `i`, if part of `::`.
fn colon_right(bytes: &[u8], i: usize) -> Option<usize> {
    (bytes.get(i.saturating_add(1)).copied() == Some(b':')).then(|| i.saturating_add(2))
}

/// Resume index moving right from `i`, or `None` at the type boundary.
///
/// Terminators stop only at depth zero; `{` and `}` always stop so bodies
/// never inflate the score.
fn step_span_end(bytes: &[u8], i: usize, depth: &mut i32) -> Option<usize> {
    match bytes.get(i).copied()? {
        b'<' | b'(' | b'[' => {
            *depth = depth.saturating_add(1);
            Some(i.saturating_add(1))
        }
        b'>' if is_fat_arrow(bytes, i) || is_thin_arrow(bytes, i) => Some(i.saturating_add(1)),
        b'>' | b')' | b']' if *depth > 0 => {
            *depth = depth.saturating_sub(1);
            Some(i.saturating_add(1))
        }
        b',' | b';' | b'=' if *depth > 0 => Some(i.saturating_add(1)),
        b'>' | b')' | b']' | b',' | b';' | b'=' | b'{' | b'}' => None,
        b':' => colon_right(bytes, i),
        _ => Some(i.saturating_add(1)),
    }
}

/// End index (exclusive) of the enclosing type from absolute offset `abs`.
fn type_span_end(window: &str, abs: usize, mut depth: i32) -> usize {
    let bytes = window.as_bytes();
    let mut i = abs;
    while i < bytes.len() {
        let Some(next) = step_span_end(bytes, i, &mut depth) else {
            break;
        };
        i = next;
    }
    i
}

/// Enclosing stripped type text for the use at byte `pos` on line `li`.
///
/// `None` when the window runs out first; callers keep the alias rather
/// than risk a verdict on a truncated type.
pub fn enclosing_type(lines: &[String], li: usize, pos: usize) -> Option<String> {
    let (window, abs, from, last) = use_window(lines, li, pos);
    let (start, depth) = type_span_start(&window, abs);
    if start == 0 && from > 0 {
        return None;
    }
    let end = type_span_end(&window, abs, depth);
    if end >= window.len() && last.saturating_add(1) < lines.len() {
        return None;
    }
    window.get(start..end).map(str::to_owned)
}

/// Applied arguments and raw group of the mention ending at `name_len`.
///
/// Reads the window at the mention (`pos` on line `li`): a `<` (past
/// whitespace and an optional turbofish `::`) opens the group parsed with
/// [`angle_end`], otherwise the mention is bare. `None` when the group
/// never closes inside the window, keeping the lint silent.
#[must_use]
pub fn applied_args_at(
    lines: &[String],
    li: usize,
    pos: usize,
    name_len: usize,
) -> Option<(Vec<String>, String)> {
    let (window, abs, _, _) = use_window(lines, li, pos);
    let bytes = window.as_bytes();
    let j = args_open_at(&window, abs.saturating_add(name_len));
    if bytes.get(j).copied() != Some(b'<') {
        return Some((Vec::new(), String::new()));
    }
    let end = angle_end(&window, j)?;
    let inner = window.get(j.saturating_add(1)..end.saturating_sub(1))?;
    let group = window.get(j..end)?.to_owned();
    Some((split_top_level(inner), group))
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_tests::range::code_of,
        rules_types::{
            alias::{
                extract::{enclosing_type, word_offsets},
                sites::splice_alias,
            },
            score::type_score,
        },
    };

    #[test]
    fn word_offsets_respect_boundaries() {
        assert_eq!(word_offsets("use Foo; let x: Foo;", "Foo"), vec![4, 16]);
        assert!(
            word_offsets("let x = Food;", "Foo").is_empty(),
            "trailing ident chars stay silent"
        );
        assert!(
            word_offsets("let x = MyFoo;", "Foo").is_empty(),
            "leading ident chars stay silent"
        );
    }

    fn site_of(raw: &str, name: &str) -> Option<String> {
        let lines = vec![raw.to_owned()];
        let pos = word_offsets(&code_of(raw), name).first().copied()?;
        enclosing_type(&lines, 0, pos)
    }

    #[test]
    fn extraction_spans_types() {
        let cases = [
            ("fn f() -> UserId {}", "UserId", " UserId "),
            ("type X = Vec<fn(u8) -> Foo>;", "Foo", "Vec<fn(u8) -> Foo>"),
            ("fn f(m: HashMap<Foo, Bar>) {}", "Foo", "HashMap<Foo, Bar>"),
        ];
        for (raw, name, want) in cases {
            assert!(
                site_of(raw, name).is_some_and(|text| text.contains(want)),
                "extraction spans {raw}"
            );
        }
    }

    #[test]
    fn variant_field_stops_at_paren() {
        assert!(
            site_of("    Source(#[source] BoxDynError),", "BoxDynError")
                .is_some_and(|text| !text.contains("Execute")),
            "variant fields never bleed into neighbors"
        );
    }

    #[test]
    fn tuple_paren_still_groups() {
        assert!(
            site_of("fn f(pair: (MetaResult, u8)) {}", "MetaResult")
                .is_some_and(|text| text.contains("(MetaResult")),
            "bare parens still group tuple types"
        );
    }

    #[test]
    fn fn_trait_paren_still_groups() {
        assert!(
            site_of("fn f(cb: fn(Huge)) {}", "Huge").is_some_and(|text| text.contains("fn(")),
            "function pointer lists stay attached"
        );
    }

    #[test]
    fn enclosing_type_covers_wrapper() {
        let raw = "fn f(tx: Sender<(i64, MetaResult)>) {}";
        assert!(
            site_of(raw, "MetaResult").is_some_and(|text| {
                text.contains("Sender<")
                    && type_score(&splice_alias(
                        &text,
                        "MetaResult",
                        "(String, String, String, Option<String>, String, i64)",
                    )) > 250
            }),
            "wrapper extraction scores over"
        );
    }
}
