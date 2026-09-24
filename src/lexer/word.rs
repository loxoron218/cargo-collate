//! Word-boundary and identifier helpers.

/// Word characters that forbid an identifier boundary on the left.
fn is_word_left(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True if `c` can start an ASCII identifier.
const fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// True if `c` can continue an ASCII identifier.
const fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if `text` ends at `len` or continues with a non-identifier char.
#[must_use]
pub fn boundary_after(text: &str, len: usize) -> bool {
    text.len() == len
        || text
            .get(len..)
            .is_some_and(|tail| tail.starts_with(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
}

/// True if `want` matches `text` at byte `i` with boundaries on both sides.
#[must_use]
pub fn word_match_at(text: &str, want: &str, i: usize) -> bool {
    if want.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let want = want.as_bytes();
    bytes.get(i..i.saturating_add(want.len())) == Some(want)
        && (i == 0
            || bytes
                .get(i.saturating_sub(1))
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_'))
        && bytes
            .get(i.saturating_add(want.len()))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// End offset past the char at byte `i`, if any.
///
/// Shared verbatim tail for whole-word substitution walkers, so their
/// scan loops never clone each other.
#[must_use]
pub fn char_end(text: &str, i: usize) -> Option<usize> {
    let c = text.get(i..)?.chars().next()?;
    Some(i.saturating_add(c.len_utf8()))
}

/// True if `word` occurs in `text` with boundaries on both sides.
#[must_use]
pub fn has_word(text: &str, word: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let want: Vec<char> = word.chars().collect();
    let Some(count) = chars.len().checked_sub(want.len()) else {
        return false;
    };
    !want.is_empty() && (0..=count).any(|i| word_at(&chars, &want, i, count))
}

/// True if `want` matches `chars` at `i` with boundaries on both sides.
fn word_at(chars: &[char], want: &[char], i: usize, count: usize) -> bool {
    let Some(body) = chars.get(i..i.saturating_add(want.len())) else {
        return false;
    };
    body.iter().eq(want.iter()) && gap_before(chars, i) && gap_after(chars, want.len(), i, count)
}

/// True if the char before `i` ends any identifier run.
fn gap_before(chars: &[char], i: usize) -> bool {
    i == 0
        || chars
            .get(i.saturating_sub(1))
            .is_some_and(|p| !is_ident_char(*p))
}

/// True if the char after the match ends any identifier run.
fn gap_after(chars: &[char], want_len: usize, i: usize, count: usize) -> bool {
    let after = i.saturating_add(want_len);
    after == count.saturating_add(want_len) || chars.get(after).is_some_and(|c| !is_ident_char(*c))
}

/// Read the identifier ending exactly at `end`; returns its start.
#[must_use]
pub fn ident_end_before(chars: &[char], end: usize) -> Option<usize> {
    if end == 0
        || chars
            .get(end.saturating_sub(1))
            .is_none_or(|c| !is_ident_char(*c))
    {
        return None;
    }
    let mut s = end.saturating_sub(1);
    while s > 0
        && chars
            .get(s.saturating_sub(1))
            .is_some_and(|p| is_ident_char(*p))
    {
        s = s.saturating_sub(1);
    }
    Some(s)
}

/// Read the identifier starting exactly at `start`; returns its end.
#[must_use]
pub fn ident_start_at(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start).is_none_or(|c| !is_ident_start(*c)) {
        return None;
    }
    let mut e = start.saturating_add(1);
    while chars.get(e).is_some_and(|c| is_ident_char(*c)) {
        e = e.saturating_add(1);
    }
    Some(e)
}

/// Declared `fn` name on `code` followed by `(` or `<`, if any.
#[must_use]
pub fn declared_fn_name(code: &str) -> Option<String> {
    code.match_indices("fn")
        .find_map(|(i, _)| fn_name_at(code, i))
}

/// `fn` name at byte `i`, requiring `(` or `<` after the name, if any.
///
/// Keyword boundaries stay required on both sides, so `fn`-prefixed call
/// names such as `fn_body` never count as declarations.
fn fn_name_at(code: &str, i: usize) -> Option<String> {
    if i > 0
        && code
            .as_bytes()
            .get(i.saturating_sub(1))
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return None;
    }
    if code
        .as_bytes()
        .get(i.saturating_add(2))
        .is_none_or(|b| !b.is_ascii_whitespace())
    {
        return None;
    }
    let after = code.get(i.saturating_add(2)..).unwrap_or("").trim_start();
    if !after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }
    let end = after
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(after.len());
    let name = after.get(..end).unwrap_or("");
    let tail = after.get(end..).unwrap_or("").trim_start();
    (tail.starts_with('(') || tail.starts_with('<')).then_some(name.to_owned())
}

/// True if `code` holds an underscore-prefixed identifier outside strings.
#[must_use]
pub fn has_underscore_ident(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        chars.get(i).is_some_and(|c| *c == '_')
            && chars
                .get(i.saturating_add(1))
                .is_some_and(char::is_ascii_alphabetic)
            && gap_before_underscore(&chars, i)
    })
}

/// True if position `i` opens an identifier run.
fn gap_before_underscore(chars: &[char], i: usize) -> bool {
    i == 0
        || chars
            .get(i.saturating_sub(1))
            .is_some_and(|p| !is_word_left(*p) && *p != '"' && *p != '}' && *p != '*' && *p != '`')
}
