//! Visibility and file-module declaration helpers.

/// Strip a bare pub word from trimmed `code` with no scope.
#[must_use]
pub fn strip_pub_word(code: &str) -> &str {
    if code.starts_with("pub")
        && code
            .get(3..)
            .is_some_and(|rest| rest.starts_with(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
    {
        code.get(3..).map_or("", |rest| rest.trim_start())
    } else {
        code
    }
}

/// Remainder after a pub visibility block, if `code` opens with one.
#[must_use]
pub fn strip_pub_block(code: &str) -> Option<&str> {
    let rest = code.strip_prefix("pub")?;
    if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let trimmed = rest.trim_start();
    if let Some(after) = trimmed.strip_prefix('(') {
        let end = after.find(')')?;
        let rest = after.get(end.saturating_add(1)..).map_or("", |part| part);
        let tail = rest.trim_start();
        return (tail.len() < rest.len()).then_some(tail);
    }
    (trimmed.len() < rest.len()).then_some(trimmed)
}

/// Remainder after a pub file-module group, if `line` opens with one.
fn pub_group_filemod(line: &str) -> Option<&str> {
    let after = line.strip_prefix("pub")?;
    let rest = if let Some(paren) = after.strip_prefix('(') {
        let end = paren.find(')')?;
        paren.get(end.saturating_add(1)..).map_or("", |part| part)
    } else {
        after
    };
    rest.strip_prefix(char::is_whitespace)
        .map(|_| rest.trim_start())
}

/// Parse the name after a `mod` keyword; returns name and remainder.
pub fn mod_name_tail(rest: &str) -> Option<(String, &str)> {
    let after = rest.strip_prefix("mod")?;
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    let trimmed = after.trim_start();
    let name: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then(|| {
        let tail = trimmed.get(name.len()..).map_or("", |part| part);
        (name, tail)
    })
}

/// Match a trailing file-module terminator; returns the name.
fn match_mod_semi(rest: &str) -> Option<String> {
    let (name, tail) = mod_name_tail(rest)?;
    tail.trim_start().strip_prefix(';').map(|_| name)
}

/// Module name of a file declaration, if `line` holds one.
#[must_use]
pub fn match_file_mod_decl(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    match_mod_semi(pub_group_filemod(trimmed).unwrap_or(trimmed))
}

/// Module name of a file declaration anywhere in `line`, if any.
#[must_use]
pub fn find_mod_semi(line: &str) -> Option<String> {
    if let Some(name) = match_file_mod_decl(line) {
        return Some(name);
    }
    let trimmed = line.trim_start();
    scan_mod_semi(trimmed)
}

/// Scan word starts in `trimmed` for a file-module declaration.
fn scan_mod_semi(trimmed: &str) -> Option<String> {
    for (i, _) in trimmed.match_indices("mod") {
        if prev_is_ident(trimmed, i) {
            continue;
        }
        if let Some(name) = trimmed.get(i..).and_then(match_mod_semi) {
            return Some(name);
        }
    }
    None
}

/// True if the char before byte `i` continues an identifier.
fn prev_is_ident(trimmed: &str, i: usize) -> bool {
    trimmed.get(..i).is_some_and(|prefix| {
        prefix
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// True if `pub` at byte `i` in `sig` is bare (not `pub(` scoped).
#[must_use]
pub fn is_bare_pub_at(sig: &str, i: usize) -> bool {
    let before_ok = i == 0
        || sig
            .as_bytes()
            .get(i.saturating_sub(1))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
    let after = sig.get(i.saturating_add(3)..).unwrap_or("");
    let after_ok = after
        .as_bytes()
        .first()
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
    before_ok && after_ok && !after.trim_start().starts_with('(')
}
