//! Use-visibility decisions for type aliases.
//!
//! Decides whether a whole-word mention can resolve to the alias: item
//! heads declare a shadowing item instead of using one, and files without
//! import evidence cannot name another module's alias at all.
use std::mem::take;

use crate::{lexer::word::has_word, rules_tests::range::code_of};

/// True if the prefix ends with an item-introducing keyword.
///
/// Matches `struct|enum|union|trait|mod|fn|type|const|static|macro|extern`
/// heads (visibility included), so a shadowing definition — including
/// cfg-gated duplicate alias definitions — never counts as a use.
/// `impl` stays handled separately via [`impl_head_ends_here`]; `use` stays
/// out since import mentions never sit in checked type positions anyway.
/// Reference: Rust Reference items
/// (`doc.rust-lang.org/reference/items.html`).
fn declares_keyword_item(pre: &str) -> bool {
    const KIND: [&str; 11] = [
        "struct", "enum", "union", "trait", "mod", "fn", "type", "const", "static", "macro",
        "extern",
    ];
    KIND.iter().any(|kind| {
        pre.strip_suffix(kind).is_some_and(|rest| {
            rest.is_empty() || rest.ends_with(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        })
    })
}

/// True if the prefix is an `impl` header ending at the mention.
///
/// Covers `impl X` and `impl<A> X<B>` self types; bound uses such as
/// `fn f(x: impl X)` start with `fn` instead and stay analyzed.
fn impl_head_ends_here(pre: &str) -> bool {
    let trimmed = pre.trim_start();
    trimmed.starts_with("impl")
        && trimmed.get(4..).is_none_or(|rest| {
            rest.is_empty() || rest.starts_with(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        })
}

/// True if the mention at `pos` declares an item rather than using one.
///
/// Shadowing definitions (`pub struct Processor`, `impl<G> Processor<G>`,
/// cfg duplicates) name a different item than the alias.
#[must_use]
pub fn is_item_head(code: &str, pos: usize) -> bool {
    let Some(pre) = code.get(..pos) else {
        return false;
    };
    let pre = pre.trim_end();
    declares_keyword_item(pre) || impl_head_ends_here(pre)
}

/// True if a path continues right after the mention ending at `end`.
///
/// A `::` suffix means namespace-style access (`Name::CONST`,
/// `Name::new`, `Name::Assoc`); only bare and generic mentions can sit in
/// checked type positions, so path heads never inform the verdict.
#[must_use]
pub fn continues_path(code: &str, end: usize) -> bool {
    let rest = code.get(end..).unwrap_or("");
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace());
    rest.starts_with("::")
}

/// True if a value construction opens right after the mention ending at `end`.
///
/// Tuple-struct calls (`Name(...)`) and struct literals (`Name { ... }`)
/// sit in unchecked value positions, so they never inform the verdict.
#[must_use]
pub fn constructs_value(code: &str, end: usize) -> bool {
    let rest = code.get(end..).unwrap_or("");
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace());
    rest.starts_with('(') || rest.starts_with('{')
}

/// True if the keyword `as` opens right before the mention at `pos`.
///
/// Cast targets (`x as Name`) sit in unchecked expression positions, and
/// rename definitions (`use Q as Name`) declare rather than use, so neither
/// can ever trip `type_complexity` on removal.
#[must_use]
pub fn preceded_by_as(code: &str, pos: usize) -> bool {
    let bytes = code.as_bytes();
    let j = ws_back_at(bytes, pos);
    if j < 2 {
        return false;
    }
    let (Some(a), Some(s)) = (
        bytes.get(j.saturating_sub(2)).copied(),
        bytes.get(j.saturating_sub(1)).copied(),
    ) else {
        return false;
    };
    if (a, s) != (b'a', b's') {
        return false;
    }
    j == 2
        || bytes
            .get(j.saturating_sub(3))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// Byte index after ASCII whitespace before `i`.
fn ws_back_at(bytes: &[u8], mut i: usize) -> usize {
    while i > 0
        && bytes
            .get(i.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        i = i.saturating_sub(1);
    }
    i
}

/// True if a path qualifies the mention at `pos` from the left.
///
/// `fmt::Result` names a path segment, which only resolves to the alias
/// through its home crate; callers gate foreign mentions on the segment.
#[must_use]
pub fn preceded_by_path(code: &str, pos: usize) -> bool {
    let bytes = code.as_bytes();
    let j = ws_back_at(bytes, pos);
    j >= 2
        && bytes.get(j.saturating_sub(2)).copied() == Some(b':')
        && bytes.get(j.saturating_sub(1)).copied() == Some(b':')
}

/// True if the path before the mention at `pos` roots in `seg`.
///
/// Walks qualifier segments back to the first (`a::b::Name` roots in
/// `a`), so home-crate paths (`member_a::Token`, global `::member_a::T`)
/// count while `fmt::`, `Self::`, and `crate::` never do. Anything but a
/// plain identifier chain (generic closers included) stays unmatched.
#[must_use]
pub fn qualifier_matches(code: &str, pos: usize, seg: &str) -> bool {
    if seg.is_empty() {
        return false;
    }
    let bytes = code.as_bytes();
    let mut j = ws_back_at(bytes, pos);
    if !(j >= 2
        && bytes.get(j.saturating_sub(2)).copied() == Some(b':')
        && bytes.get(j.saturating_sub(1)).copied() == Some(b':'))
    {
        return false;
    }
    j = j.saturating_sub(2);
    let mut first = "";
    loop {
        j = ws_back_at(bytes, j);
        let end = j;
        while j > 0
            && bytes
                .get(j.saturating_sub(1))
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            j = j.saturating_sub(1);
        }
        if j == end {
            break;
        }
        let Some(part) = code.get(j..end) else {
            break;
        };
        first = part;
        j = ws_back_at(bytes, j);
        if !(j >= 2
            && bytes.get(j.saturating_sub(2)).copied() == Some(b':')
            && bytes.get(j.saturating_sub(1)).copied() == Some(b':'))
        {
            break;
        }
        j = j.saturating_sub(2);
    }
    first == seg
}

/// First qualifier segment left of the mention at `pos`, if path-qualified.
///
/// Collects every `ident::` link back to the chain head and returns the
/// outermost one (`a` for `a::b::Name`), so import evidence can decide
/// whether the path may resolve home. Returns `None` for bare mentions.
#[must_use]
pub fn qualifier_root(code: &str, pos: usize) -> Option<String> {
    if !preceded_by_path(code, pos) {
        return None;
    }
    let bytes = code.as_bytes();
    let mut links: Vec<String> = Vec::new();
    let mut cursor = ws_back_at(bytes, pos);
    while cursor >= 2
        && bytes.get(cursor.saturating_sub(2)).copied() == Some(b':')
        && bytes.get(cursor.saturating_sub(1)).copied() == Some(b':')
    {
        cursor = cursor.saturating_sub(2);
        cursor = ws_back_at(bytes, cursor);
        let tail = cursor;
        while cursor > 0
            && bytes
                .get(cursor.saturating_sub(1))
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            cursor = cursor.saturating_sub(1);
        }
        if cursor >= tail {
            break;
        }
        let Some(link) = code.get(cursor..tail) else {
            break;
        };
        links.push(link.to_owned());
        cursor = ws_back_at(bytes, cursor);
    }
    links.into_iter().next_back()
}

/// True if stripped `code` opens a `use` item (visibility allowed).
///
/// Matches the first three whitespace-separated tokens so `pub(crate)` and
/// `pub(in path)` forms stay covered.
#[must_use]
pub fn is_use_item(code: &str) -> bool {
    let mut words = code.split_whitespace();
    for _ in 0..3 {
        if words.next() == Some("use") {
            return true;
        }
    }
    false
}

/// True per line belonging to a `use` statement, continuations included.
///
/// Import blocks often span lines (`use crate::{ a::{B}, };`); only the
/// opener carries the keyword, so lines through the terminating `;` join
/// the statement. Unterminated statements run to end of file, keeping
/// import text out of type positions rather than misreading it as types.
#[must_use]
pub fn use_statement_lines(lines: &[String]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut in_use = false;
    for (li, line) in lines.iter().enumerate() {
        let code = code_of(line);
        if !in_use && !is_use_item(&code) {
            continue;
        }
        in_use = true;
        if let Some(slot) = mask.get_mut(li) {
            *slot = true;
        }
        if code.contains(';') {
            in_use = false;
        }
    }
    mask
}

/// Import statement texts (opener through `;`) in stripped lines.
///
/// Groups the `use` mask into contiguous statements so each import's
/// names resolve against its own path; unterminated tails join the last
/// statement, keeping the lint silent rather than misreading them.
#[must_use]
pub fn use_statements(lines: &[String]) -> Vec<String> {
    let mask = use_statement_lines(lines);
    let mut out = Vec::new();
    let mut current = String::new();
    for line in lines
        .iter()
        .zip(mask)
        .filter_map(|(line, used)| used.then_some(line))
    {
        let code = code_of(line);
        let terminated = code.contains(';');
        current.push_str(&code);
        current.push('\n');
        if terminated {
            out.push(take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// True if a `use` statement names `name` from home segment `seg`.
///
/// Paths must carry the home crate segment (`member_a::Token`, globs
/// included), so `std::error::Error` never evidences another crate's
/// `Error` while genuine re-exports still count. Dashes match
/// underscores, since `member-a` ships as `member_a`; the empty segment
/// (root package) never evidences, as its crate name stays unknowable.
#[must_use]
pub fn imports_from(lines: &[String], name: &str, seg: &str) -> bool {
    if seg.is_empty() {
        return false;
    }
    let seg = seg.replace('-', "_");
    use_statements(lines)
        .iter()
        .any(|stmt| (has_word(stmt, name) || stmt.contains("::*")) && has_word(stmt, &seg))
}

#[cfg(test)]
mod tests {
    use crate::rules_types::alias::scope::{imports_from, preceded_by_as, qualifier_root};

    fn owned(lines: &[&str]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn imports_from_matches_home_segment() {
        let naming = owned(&["use member_a::Token;"]);
        assert!(imports_from(&naming, "Token", "member_a"));
        assert!(!imports_from(&naming, "Token", "member_b"));
        assert!(!imports_from(&naming, "Other", "member_a"));
        assert!(
            imports_from(&naming, "Token", "member-a"),
            "dashes match underscores"
        );
        let nested = owned(&["use member_a::{", "    Token,", "};"]);
        assert!(
            imports_from(&nested, "Token", "member_a"),
            "multi-line imports resolve per statement"
        );
        let std_import = owned(&["use std::error::Error;"]);
        assert!(
            !imports_from(&std_import, "Error", "sqlx_macros_core"),
            "foreign paths never evidence"
        );
        let glob = owned(&["use member_a::inner::*;"]);
        assert!(
            imports_from(&glob, "Token", "member_a"),
            "home globs evidence"
        );
        assert!(
            !imports_from(&glob, "Token", "other"),
            "foreign globs stay out"
        );
        assert!(
            !imports_from(&[], "Token", "member_a"),
            "missing imports stay out"
        );
        assert!(
            !imports_from(&naming, "Token", ""),
            "the root segment never evidences"
        );
    }

    #[test]
    fn as_cast_mentions_skipped() {
        assert!(preceded_by_as("let y = c as Foo;", 13));
        assert!(preceded_by_as("use other::Q as Foo;", 16));
        assert!(
            !preceded_by_as("let y: Foo = x;", 7),
            "annotations stay analyzed"
        );
        assert!(
            !preceded_by_as("let has Foo;", 8),
            "as-suffix words stay analyzed"
        );
    }

    #[test]
    fn qualifier_roots_resolve_outermost() {
        assert_eq!(
            qualifier_root("fn g() -> fmt::Result", 15).as_deref(),
            Some("fmt")
        );
        assert_eq!(
            qualifier_root("let x: a::b::Name = y;", 13).as_deref(),
            Some("a"),
            "chains report their head"
        );
        assert!(
            qualifier_root("fn f(x: Name) {}", 8).is_none(),
            "bare mentions have no root"
        );
    }
}
