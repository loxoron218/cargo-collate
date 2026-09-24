//! Character and word helpers for complexity scoring.

/// True if `c` can start an identifier.
#[must_use]
pub const fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// True if `c` can continue an identifier.
#[must_use]
pub const fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if `word` is a keyword or sigil that never denotes a scored type.
///
/// Complete keyword set from the Rust Reference (strict + reserved + weak,
/// `doc.rust-lang.org/reference/keywords.html`): any word that can
/// appear in a type string but never denotes a type node of its own.
/// `Self` stays scored since it denotes the current type; `true`/`false`
/// are value sigils, never types.
#[must_use]
pub fn is_skipped_word(word: &str) -> bool {
    matches!(
        word,
        "where"
            | "for"
            | "in"
            | "pub"
            | "crate"
            | "self"
            | "super"
            | "mut"
            | "const"
            | "ref"
            | "static"
            | "extern"
            | "unsafe"
            | "trait"
            | "struct"
            | "enum"
            | "union"
            | "type"
            | "mod"
            | "use"
            | "as"
            | "fn"
            | "dyn"
            | "impl"
            | "true"
            | "false"
            | "async"
            | "await"
            | "break"
            | "continue"
            | "else"
            | "if"
            | "loop"
            | "match"
            | "return"
            | "let"
            | "move"
            | "while"
            | "box"
            | "try"
            | "yield"
            | "gen"
            | "macro"
            | "abstract"
            | "become"
            | "do"
            | "final"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
    )
}

/// True if `word` holds only ASCII digits (a const generic, never a type).
#[must_use]
pub fn is_numeric(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

/// Nesting level for `depth` open delimiters.
#[must_use]
pub const fn nest_at(depth: u64) -> u64 {
    depth.saturating_add(1)
}

/// Index of the first non-whitespace char from `i`, if any.
#[must_use]
pub fn skip_ws(chars: &[char], mut i: usize) -> Option<usize> {
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i = i.saturating_add(1);
    }
    chars.get(i).map(|_| i)
}

/// Index of the previous non-whitespace char before `i`, if any.
#[must_use]
pub fn prev_nonws_idx(chars: &[char], i: usize) -> Option<usize> {
    (0..i)
        .rev()
        .find(|&j| chars.get(j).is_some_and(|c| !c.is_whitespace()))
}

/// True if `::` closes right before `i`; returns the first colon on success.
#[must_use]
pub fn colon_pair_before(chars: &[char], i: usize) -> Option<usize> {
    let second = prev_nonws_idx(chars, i)?;
    if chars.get(second).copied() != Some(':') {
        return None;
    }
    let first = prev_nonws_idx(chars, second)?;
    (chars.get(first).copied() == Some(':')).then_some(first)
}

/// True if `c` can end a path before `::` or generic args.
#[must_use]
pub const fn is_path_end(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '>' | ']' | ')')
}

/// True if the identifier at `start` continues a `::` chain.
///
/// Only the first segment scores: `a::b::c<T>` is one path node plus its
/// args, exactly like Clippy counts it.
#[must_use]
pub fn continues_chain(chars: &[char], start: usize) -> bool {
    colon_pair_before(chars, start).is_some_and(|first| {
        prev_nonws_idx(chars, first).is_some_and(|k| chars.get(k).is_some_and(|c| is_path_end(*c)))
    })
}

/// True if the identifier at `start` names the trait in `<Q as Trait>`.
///
/// Trait names contribute no node of their own; the qualified path and its
/// payload carry the score.
#[must_use]
pub fn follows_as(chars: &[char], start: usize) -> bool {
    prev_word(chars, start).as_deref() == Some("as")
}

/// True if `<` at `i` opens a qualified path rather than generic args.
///
/// Generic args follow a path segment (`Vec<…>`); anything else
/// (`&<T as Trait>`) opens a qualified path, which Clippy scores as its own
/// node on top of the payload.
#[must_use]
pub fn is_qualified_open(chars: &[char], i: usize) -> bool {
    prev_nonws_idx(chars, i).is_none_or(|k| {
        chars
            .get(k)
            .is_none_or(|c| !is_ident_char(*c) && !matches!(c, '>' | ']' | ')' | '<' | '?'))
    })
}

/// Word immediately before `i` (whitespace skipped), if any.
pub fn prev_word(chars: &[char], i: usize) -> Option<String> {
    let mut end = i;
    while end > 0
        && chars
            .get(end.saturating_sub(1))
            .is_some_and(|c| c.is_whitespace())
    {
        end = end.saturating_sub(1);
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0
        && chars
            .get(start.saturating_sub(1))
            .is_some_and(|c| is_ident_char(*c))
    {
        start = start.saturating_sub(1);
    }
    if start >= end {
        return None;
    }
    Some(
        chars
            .get(start..end)
            .map_or_else(String::new, |part| part.iter().collect()),
    )
}

/// End exclusive of the identifier starting at `i`.
#[must_use]
pub fn ident_end(chars: &[char], i: usize) -> usize {
    let mut j = i;
    while chars.get(j).is_some_and(|c| is_ident_char(*c)) {
        j = j.saturating_add(1);
    }
    j
}

/// Word and end exclusive of the identifier opening at `i`.
///
/// Handles raw identifiers (`r#type`); returns the word without the prefix
/// and the end offset past it.
pub fn ident_word_at(chars: &[char], i: usize) -> Option<(String, usize)> {
    let mut start = i;
    if chars.get(start) == Some(&'r') && chars.get(start.saturating_add(1)) == Some(&'#') {
        start = start.saturating_add(2);
    }
    if chars.get(start).is_none_or(|c| !is_ident_start(*c)) {
        return None;
    }
    let end = ident_end(chars, start);
    let word: String = chars
        .get(start..end)
        .map_or_else(String::new, |part| part.iter().collect());
    Some((word, end))
}
