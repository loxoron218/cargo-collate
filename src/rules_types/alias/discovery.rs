//! Free-standing `type` alias discovery.
//!
//! Collects `type Name = RHS;` spans at any nesting except inside `trait` or
//! `impl` blocks, where the item is an associated type rather than a free
//! alias. Right-hand sides join across lines up to the terminating `;` with
//! the `where` clause truncated, matching what `clippy::type_complexity`
//! scores.

use crate::{
    lexer::word::{has_word, word_match_at},
    rules_tests::range::code_of,
    rules_types::alias::{
        head::{GenericParam, head_parts},
        macros::{cover_hits, macro_cover},
    },
};

/// Maximum lines joined for one alias right-hand side.
const JOIN_LIMIT: usize = 20;

/// A free-standing type alias with its stripped right-hand side.
#[derive(Debug)]
pub struct TypeAlias {
    /// Zero-based start line of the declaration.
    pub start: usize,
    /// Zero-based end line (inclusive) of the declaration.
    pub end: usize,
    /// Declared name.
    pub name: String,
    /// Declared generic parameters, empty for plain aliases.
    pub params: Vec<GenericParam>,
    /// `where`-truncated right-hand side.
    pub rhs: String,
}

/// True if `type` opens at byte `i` with identifier boundaries.
fn is_type_at(bytes: &[u8], i: usize) -> bool {
    if bytes.get(i..i.saturating_add(4)) != Some(b"type") {
        return false;
    }
    let before_ok = i == 0
        || bytes
            .get(i.saturating_sub(1))
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
    let after_ok = bytes
        .get(i.saturating_add(4))
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
    before_ok && after_ok
}

/// Byte index of `type` with identifier boundaries in `code`, if any.
fn type_keyword_at(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut i: usize = 0;
    while i.saturating_add(4) <= bytes.len() {
        if is_type_at(bytes, i) {
            return Some(i);
        }
        i = i.saturating_add(1);
    }
    None
}

/// True if a statement header opens an associated-type block.
///
/// The header spans lines (e.g. `where` clauses), so `impl` blocks with the
/// brace on its own line stay covered.
fn is_assoc_header(header: &str) -> bool {
    has_word(header, "trait") || has_word(header, "impl")
}

/// Update the block-kind stack with every brace in `code`.
///
/// Each `{` pushes whether its statement header (accumulated in `segment`
/// since the previous delimiter) mentions `trait` or `impl`; each `}`
/// pops. Headers spanning lines keep `where`-clause `impl` blocks covered.
/// A `;` ends the header only outside brackets, so array lengths like the
/// one in `impl Frame for [S; N]` never truncate it. Exotic same-line mixes
/// classify toward exempt, keeping the lint silent rather than noisy.
fn step_blocks(stack: &mut Vec<bool>, segment: &mut String, code: &str) {
    let mut depth: i32 = 0;
    for c in code.chars() {
        match c {
            '{' => {
                stack.push(is_assoc_header(segment));
                segment.clear();
            }
            '}' => {
                let _: Option<bool> = stack.pop();
                segment.clear();
            }
            ';' if depth <= 0 => segment.clear(),
            '<' | '(' | '[' => {
                depth = depth.saturating_add(1);
                segment.push(c);
            }
            '>' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                segment.push(c);
            }
            _ => segment.push(c),
        }
    }
}

/// Net unclosed `(`/`[` depth before byte `pos`, atop carried `base`.
///
/// A `type` keyword past depth zero sits inside macro arguments, never a
/// real item, so callers keep it silent.
fn group_depth_at(code: &str, pos: usize, base: i32) -> i32 {
    let mut depth = base;
    for c in code.get(..pos).unwrap_or("").chars() {
        match c {
            '(' | '[' => depth = depth.saturating_add(1),
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// True if the `type` at `pos` sits after a same-line `trait` or `impl` block
/// opener, covering single-line associated types.
fn prefix_opens_assoc(code: &str, pos: usize) -> bool {
    let prefix = code.get(..pos).unwrap_or("");
    prefix.contains('{') && (has_word(prefix, "trait") || has_word(prefix, "impl"))
}

/// Byte index of `where` with identifier boundaries in `rhs`, if any.
fn where_at(rhs: &str) -> Option<usize> {
    if rhs.len() < "where".len() {
        return None;
    }
    let last = rhs.len().saturating_sub("where".len());
    (0..=last).find(|&i| word_match_at(rhs, "where", i))
}

/// Name, parameters, right-hand side, and end line for the alias at `idx`.
///
/// `kw` is the `type` keyword offset in the first joined line. Joins
/// stripped lines up to the terminating `;`, locates the head `=`, and
/// returns the `where`-truncated RHS. Unterminated items stay skipped.
fn alias_span(
    lines: &[String],
    idx: usize,
    kw: usize,
) -> Option<(String, Vec<GenericParam>, String, usize)> {
    let mut joined = String::new();
    let mut end = idx;
    for (off, line) in lines.iter().skip(idx).take(JOIN_LIMIT).enumerate() {
        joined.push_str(&code_of(line));
        joined.push('\n');
        end = idx.saturating_add(off);
        if joined.contains(';') {
            break;
        }
    }
    if !joined.contains(';') {
        return None;
    }
    let (name, params, eq) = head_parts(&joined, kw)?;
    let after = joined.get(eq.saturating_add(1)..)?;
    let mut rhs = after.split_once(';')?.0.to_owned();
    if let Some(pos) = where_at(&rhs) {
        rhs.truncate(pos);
    }
    Some((name, params, rhs, end))
}

/// Alias declarations with their stripped right-hand sides.
///
/// Associated types inside `trait` or `impl` blocks stay excluded, as do
/// `type` declarations without `=`, fragments inside macro arguments, and
/// templates inside macro definitions or invocations. Item bodies in
/// `macro` 2.0 definitions analyze as ordinary code, since their items
/// are real declarations rather than templates.
#[must_use]
pub fn collect_aliases(lines: &[String]) -> Vec<TypeAlias> {
    let cover = macro_cover(lines);
    let mut out = Vec::new();
    let mut blocks: Vec<bool> = Vec::new();
    let mut segment = String::new();
    let mut groups: i32 = 0;
    let mut idx = 0;
    while idx < lines.len() {
        let Some(line) = lines.get(idx) else {
            break;
        };
        let code = code_of(line);
        let in_assoc = blocks.iter().any(|b| *b);
        if !in_assoc
            && let Some(pos) = type_keyword_at(&code)
            && !cover_hits(&cover, idx, pos)
            && !prefix_opens_assoc(&code, pos)
            && group_depth_at(&code, pos, groups) == 0
            && let Some((name, params, rhs, end)) = alias_span(lines, idx, pos)
        {
            out.push(TypeAlias {
                start: idx,
                end,
                name,
                params,
                rhs,
            });
        }
        step_blocks(&mut blocks, &mut segment, &code);
        groups = group_depth_at(&code, code.len(), groups);
        idx = idx.saturating_add(1);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_types::alias::discovery::{TypeAlias, collect_aliases},
        scan::support::test_file,
    };

    #[test]
    fn alias_shape_survives() {
        let file = test_file("src/a.rs", &["type Foo = u32;"]);
        let aliases = collect_aliases(&file.lines);
        assert_eq!(aliases.len(), 1);
        assert!(
            aliases.first().is_some_and(|alias: &TypeAlias| {
                alias.start == 0 && alias.end == 0 && alias.name == "Foo"
            }),
            "span and name resolve"
        );
    }

    #[test]
    fn free_alias_collected() {
        let file = test_file("src/a.rs", &["type Foo = u32;"]);
        assert_eq!(collect_aliases(&file.lines).len(), 1);
    }

    #[test]
    fn assoc_types_skipped() {
        let bounded = test_file("src/a.rs", &["trait Api {", "    type Out = u32;", "}"]);
        assert!(
            collect_aliases(&bounded.lines).is_empty(),
            "trait defaults stay silent"
        );
        let single = test_file("src/b.rs", &["trait T { type O = u32; }"]);
        assert!(
            collect_aliases(&single.lines).is_empty(),
            "single-line trait stays silent"
        );
        let implemented = test_file(
            "src/c.rs",
            &["impl Api for Foo {", "    type Out = u32;", "}"],
        );
        assert!(
            collect_aliases(&implemented.lines).is_empty(),
            "impl items stay silent"
        );
    }

    #[test]
    fn multiline_and_pub_collected() {
        let file = test_file(
            "src/a.rs",
            &["pub type Pair =", "    ( i32 ,", "    String ) ;"],
        );
        assert_eq!(collect_aliases(&file.lines).len(), 1);
    }

    #[test]
    fn bare_assoc_without_eq_skipped() {
        let file = test_file("src/a.rs", &["trait T {", "    type Out;", "}"]);
        assert!(
            collect_aliases(&file.lines).is_empty(),
            "eq-less declarations stay silent"
        );
    }

    #[test]
    fn where_clause_impl_skipped() {
        let file = test_file(
            "src/a.rs",
            &[
                "impl<S> Iterator for UntilExhausted<S>",
                "where",
                "    S: Signal,",
                "{",
                "    type Item = S::Frame;",
                "}",
            ],
        );
        assert!(
            collect_aliases(&file.lines).is_empty(),
            "brace-on-own-line impls stay silent"
        );
    }

    #[test]
    fn array_impl_header_skipped() {
        let file = test_file(
            "src/a.rs",
            &[
                "impl<S, const N: usize> Frame for [S; N]",
                "where",
                "    S: Sample,",
                "{",
                "    type Sample = S;",
                "}",
            ],
        );
        assert!(
            collect_aliases(&file.lines).is_empty(),
            "array-impl items stay silent"
        );
    }

    #[test]
    fn fat_arrow_after_bogus_type_skipped() {
        let file = test_file(
            "src/a.rs",
            &[
                "let msg = \"element for this \\",
                "type contains a case\";",
                "let conclusion = match result {",
                "    _ => 1,",
                "};",
            ],
        );
        assert!(
            collect_aliases(&file.lines).is_empty(),
            "match arms never resolve an alias head"
        );
    }

    #[test]
    fn macro_arg_type_skipped() {
        let single = test_file(
            "src/a.rs",
            &["forward!(impl Add, add for Sugg, type Output = u32);"],
        );
        assert!(
            collect_aliases(&single.lines).is_empty(),
            "macro arguments stay silent"
        );
        let multi = test_file("src/b.rs", &["wrap!(", "    type Inner = u32,", ");"]);
        assert!(
            collect_aliases(&multi.lines).is_empty(),
            "multiline macro arguments stay silent"
        );
    }

    #[test]
    fn macro_rules_embedded_type_ignored() {
        let file = test_file(
            "src/a.rs",
            &["macro_rules! m {", "    () => { type Inner = u32; },", "}"],
        );
        assert!(
            collect_aliases(&file.lines).is_empty(),
            "macro templates never declare aliases"
        );
    }

    #[test]
    fn generic_default_finds_outer_eq() {
        let file = test_file("src/a.rs", &["type Foo<T = u32> = Vec<T>;"]);
        let aliases = collect_aliases(&file.lines);
        assert_eq!(aliases.len(), 1, "outer assignment resolves");
        assert!(
            aliases
                .first()
                .is_some_and(|alias| alias.rhs.contains("Vec")),
            "RHS holds the aliased type"
        );
    }
}
