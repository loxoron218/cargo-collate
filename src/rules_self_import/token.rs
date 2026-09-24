//! Character-level matchers for use and module declarations.

/// Skip ASCII whitespace from `j`.
fn skip_ws_all(chars: &[char], mut j: usize) -> usize {
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j = j.saturating_add(1);
    }
    j
}

/// Consume one expected char at `j`; returns the index after it.
#[must_use]
pub fn take_char(chars: &[char], j: usize, expect: char) -> Option<usize> {
    (chars.get(j) == Some(&expect)).then_some(j.saturating_add(1))
}

/// Consume `word` at `j`; returns the index after it.
#[must_use]
pub fn take_word(chars: &[char], j: usize, word: &str) -> Option<usize> {
    let mut k = j;
    for c in word.chars() {
        if chars.get(k) != Some(&c) {
            return None;
        }
        k = k.saturating_add(1);
    }
    Some(k)
}

/// Match a cfg-test marker exactly at `i`; returns the exclusive end.
#[must_use]
pub fn match_cfg_test(chars: &[char], i: usize) -> Option<usize> {
    let mut j = take_char(chars, i, '#')?;
    j = take_char(chars, skip_ws_all(chars, j), '[')?;
    j = take_word(chars, skip_ws_all(chars, j), "cfg")?;
    j = take_char(chars, skip_ws_all(chars, j), '(')?;
    j = take_word(chars, skip_ws_all(chars, j), "test")?;
    j = take_char(chars, skip_ws_all(chars, j), ')')?;
    take_char(chars, skip_ws_all(chars, j), ']')
}

/// Match a module declaration exactly at `i`.
pub fn match_mod_decl(chars: &[char], i: usize) -> Option<(String, char, usize, usize)> {
    let mut j = take_word(chars, i, "mod")?;
    if !chars.get(j).is_some_and(|c| c.is_whitespace()) {
        return None;
    }
    j = skip_ws_all(chars, j);
    let start = j;
    if !chars
        .get(j)
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
    {
        return None;
    }
    while chars
        .get(j)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        j = j.saturating_add(1);
    }
    let name: String = chars
        .get(start..j)
        .map_or_else(String::new, |part| part.iter().collect());
    j = skip_ws_all(chars, j);
    let kind = match chars.get(j) {
        Some(';') => ';',
        Some('{') => '{',
        _ => return None,
    };
    Some((name, kind, j.saturating_add(1), j))
}

/// Match the use keyword with optional pub prefix exactly at `i`.
#[must_use]
pub fn match_use_kw(chars: &[char], i: usize) -> Option<usize> {
    if chars
        .get(i..)
        .is_some_and(|s| s.starts_with(&['p', 'u', 'b']))
    {
        return match_use_prefixed(chars, i);
    }
    match_bare_use_at(chars, i)
}

/// Match use after a pub prefix at `i`.
#[must_use]
pub fn match_use_prefixed(chars: &[char], i: usize) -> Option<usize> {
    let mut j = skip_ws_all(chars, i.saturating_add(3));
    if chars.get(j) == Some(&'(') {
        j = j.saturating_add(1);
        while chars.get(j).is_some_and(|c| *c != ')') {
            j = j.saturating_add(1);
        }
        if j >= chars.len() {
            return None;
        }
        j = skip_ws_all(chars, j.saturating_add(1));
    }
    match_bare_use_at(chars, j)
}

/// Match a bare use keyword at `j`; returns the exclusive end.
#[must_use]
pub fn match_bare_use_at(chars: &[char], j: usize) -> Option<usize> {
    if !chars
        .get(j..)
        .is_some_and(|s| s.starts_with(&['u', 's', 'e']))
    {
        return None;
    }
    if chars
        .get(j.saturating_add(3))
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        return None;
    }
    Some(j.saturating_add(3))
}
