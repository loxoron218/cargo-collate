//! Stray `use` predicates for headers, modules, and macros.
//!
//! Boolean classifiers shared by the scope scan; each answers one question
//! about a stripped line so the state machine stays declarative.

use crate::{
    lexer::{
        visibility::{mod_name_tail, strip_pub_block},
        word::{boundary_after, has_word},
    },
    rules_imports::rank::is_use_start,
    rules_tests::order::{is_macro_call, strip_attrs},
};

/// True if stripped `code` opens an `extern` item.
#[must_use]
pub fn is_extern_open(code: &str) -> bool {
    let flat = strip_attrs(code);
    let trimmed = flat.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    rest.starts_with("extern") && boundary_after(rest, 6)
}

/// Name and tail of the `mod` head opening stripped `code`, if any.
#[must_use]
pub fn mod_head(code: &str) -> Option<(String, String)> {
    let flat = strip_attrs(code);
    let trimmed = flat.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    let (name, tail) = mod_name_tail(rest)?;
    Some((name, tail.to_owned()))
}

/// True if stripped `code` declares a `mod` in any semicolon segment.
#[must_use]
pub fn has_mod_decl(code: &str) -> bool {
    code.split(';').any(|seg| mod_head(seg).is_some())
}

/// Name of the macro called by stripped `code`, if any.
#[must_use]
pub fn macro_callee(code: &str) -> Option<String> {
    if !is_macro_call(code) {
        return None;
    }
    let bang = code.find('!')?;
    let head = code.get(..bang).map_or("", |prefix| prefix).trim_end();
    let name = head.rsplit(':').next().unwrap_or("");
    (!name.is_empty()).then_some(name.to_owned())
}

/// True if stripped `code` holds a sealing macro call.
///
/// `compile_error!` guards never seal; they expand to no items.
#[must_use]
pub fn macro_seals(code: &str) -> bool {
    macro_callee(code).is_some_and(|name| name != "compile_error")
}

/// True if stripped `code` seals the import header of its scope.
///
/// Attributes, `use`, `extern crate`, and `mod` lines never seal; any other
/// item, macro definition, or sealing macro call does.
#[must_use]
pub fn seals_header(code: &str) -> bool {
    const KINDS: [&str; 9] = [
        "fn", "struct", "enum", "trait", "impl", "const", "static", "type", "macro",
    ];
    if is_use_start(code) || is_extern_open(code) || has_mod_decl(code) {
        return false;
    }
    if code.trim_start().starts_with("macro_rules!") {
        return true;
    }
    KINDS.iter().any(|kind| has_word(code, kind)) || macro_seals(code)
}

/// True for blank, attribute-only, or comment-only stripped `code`.
#[must_use]
pub fn is_filler(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Name of the `mod` whose opener sits at byte `brace_at` in `code`, if any.
///
/// Only the segment after the last `;` counts, so earlier statements on the
/// same line never leak their meaning into the brace.
#[must_use]
pub fn mod_name_before(code: &str, brace_at: usize) -> Option<String> {
    let prefix = code.get(..brace_at)?;
    let seg = prefix.rsplit(';').next().unwrap_or("");
    let (name, tail) = mod_head(seg)?;
    (!tail.contains(';')).then_some(name)
}

/// Name of a lone `mod` head without opener or terminator, if any.
#[must_use]
pub fn undecided_mod(code: &str) -> Option<String> {
    let (name, tail) = mod_head(code)?;
    (!tail.contains('{') && !tail.contains(';')).then_some(name)
}

/// True if the brace at byte `brace_at` opens a macro definition.
///
/// Only `macro_rules!` and `macro` items count; plain invocations hold
/// literal code tracked normally. Only the segment after the last `;`
/// counts, so earlier statements never leak into the brace.
#[must_use]
pub fn is_macro_def_open(code: &str, brace_at: usize) -> bool {
    let Some(prefix) = code.get(..brace_at) else {
        return false;
    };
    let seg = prefix.rsplit(';').next().unwrap_or("");
    let flat = strip_attrs(seg);
    let trimmed = flat.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    if let Some(tail) = rest.strip_prefix("macro_rules") {
        return tail.trim_start().starts_with('!');
    }
    rest.strip_prefix("macro")
        .is_some_and(|tail| tail.starts_with(char::is_whitespace))
}

/// True if the bracket at byte `open_at` opens a `quote!`-family body.
///
/// Token templates never hold module items, so their contents stay opaque.
/// Only the segment after the last `;` counts, and the callee is the last
/// path word before the bang.
#[must_use]
pub fn is_quote_open(code: &str, open_at: usize) -> bool {
    let Some(prefix) = code.get(..open_at) else {
        return false;
    };
    let seg = prefix.rsplit(';').next().unwrap_or("");
    let trimmed = seg.trim_end();
    if !trimmed.ends_with('!') {
        return false;
    }
    let before = trimmed.get(..trimmed.len().saturating_sub(1)).unwrap_or("");
    let word = before.split_whitespace().next_back().unwrap_or("");
    let callee = word.rsplit(':').next().unwrap_or("");
    matches!(callee, "quote" | "quote_spanned" | "quote_into")
}

#[cfg(test)]
mod tests {
    use crate::rules_imports::stray::classify::{
        is_macro_def_open, is_quote_open, macro_callee, macro_seals,
    };

    #[test]
    fn callees_read() {
        assert_eq!(
            macro_callee("compile_error!(\"need x\");").as_deref(),
            Some("compile_error"),
            "bare callee reads"
        );
        assert_eq!(
            macro_callee("core::compile_error!(\"need x\");").as_deref(),
            Some("compile_error"),
            "pathed callee reads"
        );
        assert_eq!(
            macro_callee("fn first() {}"),
            None,
            "plain code stays silent"
        );
    }

    #[test]
    fn guards_never_seal() {
        assert!(
            !macro_seals("compile_error!(\"need x\");"),
            "guards stay silent"
        );
        assert!(macro_seals("bitflags! {"), "item macros still seal");
    }

    #[test]
    fn openers_classified() {
        assert!(
            is_macro_def_open("macro_rules! make {", 18),
            "definitions count"
        );
        assert!(
            is_macro_def_open("pub macro make {", 15),
            "macro items count"
        );
        assert!(
            !is_macro_def_open("bitflags! {", 10),
            "invocations stay plain"
        );
        assert!(
            !is_macro_def_open("if !ready {", 10),
            "negations stay silent"
        );
        assert!(
            !is_macro_def_open("if x != y {", 10),
            "comparisons stay silent"
        );
        assert!(
            !is_macro_def_open("fn first() {", 11),
            "plain blocks stay silent"
        );
        assert!(is_quote_open("quote! {", 7), "quote bodies count");
        assert!(
            is_quote_open("quote_spanned!(s =>", 14),
            "spanned bodies count"
        );
        assert!(
            !is_quote_open("wrap_client! {", 14),
            "plain calls stay silent"
        );
        assert!(!is_quote_open("if !(x) {", 8), "negated parens stay silent");
    }
}
