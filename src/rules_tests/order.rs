//! Module-order checks for top-level declarations.

use crate::{
    lexer::{
        visibility::{mod_name_tail, strip_pub_block},
        word::{boundary_after, has_word},
    },
    rules_tests::range::{brace_count, brace_delta, code_of},
    scan::SourceFile,
};

/// Whether a module declaration is inline or a file reference.
#[derive(PartialEq)]
enum ModKind {
    /// Braced inline module body.
    Inline,
    /// File module reference.
    File,
}

/// Top-level scan state for one file.
#[derive(Default)]
struct OrderState {
    /// Current brace nesting depth.
    depth: i32,
    /// True once a top-level code item was seen.
    seen_code: bool,
}

/// Strip leading attributes from `code`.
#[must_use]
pub fn strip_attrs(code: &str) -> String {
    let mut rest = code.trim_start().to_owned();
    while let Some(end) = attr_end(&rest) {
        rest = rest
            .get(end..)
            .map_or("", |tail| tail)
            .trim_start()
            .to_owned();
    }
    rest
}

/// End exclusive of the leading attribute, if any.
fn attr_end(rest: &str) -> Option<usize> {
    if !rest.starts_with("#[") {
        return None;
    }
    let mut depth: i32 = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth = depth.saturating_add(1),
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c == ']' && depth == 0 {
            return Some(i.saturating_add(1));
        }
    }
    None
}

/// Match a module head at the start of `code`; returns name plus remainder.
#[must_use]
pub fn match_mod_head(code: &str) -> Option<(String, &str)> {
    let trimmed = code.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    mod_name_tail(rest)
}

/// True if `code` declares a top-level item or macro call.
///
/// Complete item-head set from the Rust Reference
/// (`doc.rust-lang.org/reference/items.html`): `use`, `extern`,
/// `mod`, `fn`, `struct`, `enum`, `union`, `trait`, `impl`, `const`,
/// `static`, `type`, `macro` (plus `macro_rules!` and macro calls below).
fn is_code_item(code: &str) -> bool {
    let rest = strip_attrs(code);
    let r = strip_pub_block(&rest).unwrap_or(&rest);
    let r = strip_prefix_words(r, &["async", "unsafe"]);
    r.starts_with("macro_rules!")
        || [
            "use", "extern", "mod", "fn", "struct", "enum", "union", "trait", "impl", "const",
            "static", "type", "macro",
        ]
        .iter()
        .any(|kw| r.starts_with(kw) && boundary_after(r, kw.len()))
        || is_macro_call(r)
}

/// Strip leading keywords from `r`.
fn strip_prefix_words<'a>(mut r: &'a str, words: &[&str]) -> &'a str {
    while let Some(next) = words.iter().find_map(|kw| strip_one_word(r, kw)) {
        r = next;
    }
    r
}

/// Remainder after one leading keyword, if `r` opens with it.
fn strip_one_word<'a>(r: &'a str, kw: &str) -> Option<&'a str> {
    (r.starts_with(kw) && boundary_after(r, kw.len()))
        .then(|| r.get(kw.len()..).map_or("", |tail| tail.trim_start()))
}

/// True if `r` is a macro call.
#[must_use]
pub fn is_macro_call(r: &str) -> bool {
    let Some(bang) = r.find('!') else {
        return false;
    };
    let head = r.get(..bang).map_or("", |prefix| prefix).trim_end();
    !head.is_empty()
        && head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        && !head.ends_with(':')
}

/// Skip an inline block from `idx`; returns the index after it.
#[must_use]
pub fn skip_inline(lines: &[String], idx: usize) -> usize {
    let mut depth: i32 = 0;
    let mut started = false;
    let mut k = idx;
    while let Some(line) = lines.get(k) {
        let code = code_of(line);
        started |= code.contains('{');
        depth = depth
            .saturating_add(brace_count(&code, '{'))
            .saturating_sub(brace_count(&code, '}'));
        k = k.saturating_add(1);
        if started && depth <= 0 {
            break;
        }
    }
    k
}

/// Classify the module at `idx` from its remainder `rest` and following lines.
fn mod_kind(lines: &[String], idx: usize, rest: &str) -> ModKind {
    let brace = rest.find('{');
    let semi = rest.find(';');
    if brace.is_some_and(|b| semi.is_none_or(|s| b < s)) {
        return ModKind::Inline;
    }
    if semi.is_some() {
        return ModKind::File;
    }
    kind_from_following(lines, idx)
}

/// True if the module at `idx` is an inline braced body.
#[must_use]
pub fn is_inline_mod(lines: &[String], idx: usize, rest: &str) -> bool {
    mod_kind(lines, idx, rest) == ModKind::Inline
}

/// Inline when the opener precedes the terminator on one line.
fn order_kind(brace: Option<usize>, semi: Option<usize>) -> ModKind {
    if brace < semi {
        ModKind::Inline
    } else {
        ModKind::File
    }
}

/// Classify from up to three following lines when undecided on one line.
fn kind_from_following(lines: &[String], idx: usize) -> ModKind {
    for line in lines.iter().skip(idx.saturating_add(1)).take(3) {
        let code = code_of(line);
        if code.trim().is_empty() || code.trim().starts_with('#') {
            continue;
        }
        if code.contains('{') && !code.contains(';') {
            return ModKind::Inline;
        }
        if code.contains('{') && code.contains(';') {
            return order_kind(code.find('{'), code.find(';'));
        }
        if code.contains(';') {
            return ModKind::File;
        }
        break;
    }
    ModKind::File
}

/// Module declarations after code, and empty inline test modules.
pub fn module_order(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_order).collect()
}

/// Module-order findings for one file.
fn check_file_order(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    let mut state = OrderState::default();
    let mut idx = 0;
    while idx < file.lines.len() {
        idx = order_step(file, idx, &mut state, &mut out);
    }
    out
}

/// Advance the module-order scan by one line; returns the next index.
fn order_step(
    file: &SourceFile,
    idx: usize,
    state: &mut OrderState,
    out: &mut Vec<String>,
) -> usize {
    let Some(line) = file.stripped.get(idx) else {
        return idx.saturating_add(1);
    };
    let code = code_of(line);
    if state.depth != 0 {
        state.depth = state.depth.saturating_add(brace_delta(&code)).max(0);
        return idx.saturating_add(1);
    }
    if let Some((name, rest)) = match_mod_head(&strip_attrs(&code)) {
        return step_top_mod(file, idx, &name, rest, state, out);
    }
    if is_filler(&code) {
        state.depth = state.depth.saturating_add(brace_delta(&code)).max(0);
        return idx.saturating_add(1);
    }
    state.seen_code |= is_code_item(&code);
    state.depth = state.depth.saturating_add(brace_delta(&code)).max(0);
    idx.saturating_add(1)
}

/// True for blank, attribute-only, or lone-bracket lines.
fn is_filler(code: &str) -> bool {
    let s = code.trim();
    s.is_empty() || s.starts_with('#') || s == "{" || s == "}" || s == ";"
}

/// Advance past a top-level module declaration.
fn step_top_mod(
    file: &SourceFile,
    idx: usize,
    name: &str,
    rest: &str,
    state: &mut OrderState,
    out: &mut Vec<String>,
) -> usize {
    if name == "tests" && mod_kind(&file.stripped, idx, rest) == ModKind::Inline {
        let end = skip_inline(&file.stripped, idx);
        if !tests_body_has_tests(file, idx, end)
            && let Some(line) = file.lines.get(idx)
        {
            out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
        }
        return end;
    }
    if name == "tests" {
        return idx.saturating_add(1);
    }
    if state.seen_code
        && let Some(line) = file.lines.get(idx)
    {
        out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
    }
    if mod_kind(&file.stripped, idx, rest) == ModKind::Inline {
        return skip_inline(&file.stripped, idx);
    }
    if let Some(line) = file.stripped.get(idx) {
        state.depth = state
            .depth
            .saturating_add(brace_delta(&code_of(line)))
            .max(0);
    }
    idx.saturating_add(1)
}

/// True if the inline test body holds a test attribute.
fn tests_body_has_tests(file: &SourceFile, start: usize, end: usize) -> bool {
    let body = file.stripped.get(start..end).map_or(String::new(), |part| {
        part.iter()
            .map(|line| code_of(line))
            .collect::<Vec<_>>()
            .join("\n")
    });
    has_test_attr(&body)
}

/// True if `body` holds a test attribute.
///
/// Covers runner attributes (`#[tokio::test]` and other `::test` paths via
/// [`has_plain_test_attr`] + `::test` in [`crate::rules_docs::missing_docs`],
/// `#[rstest]`, parameterized `#[case]` / `#[test_case]`, `#[wasm_bindgen_test]`)
/// plus plain `#[test]`. `#[bench]` stays handled by callers that need it.
fn has_test_attr(body: &str) -> bool {
    [
        "tokio::test",
        "rstest",
        "case",
        "test_case",
        "wasm_bindgen_test",
        "parametrized",
    ]
    .iter()
    .any(|m| has_word(body, m))
        || has_plain_test_attr(body)
}

/// True if `body` holds a plain test attribute.
fn has_plain_test_attr(body: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    (0..chars.len()).any(|i| chars.get(i).is_some_and(|c| *c == '#') && test_attr_at(&chars, i))
}

/// True if a test attribute opens at `i`.
fn test_attr_at(chars: &[char], i: usize) -> bool {
    let mut j = skip_ws(chars, i.saturating_add(1));
    if chars.get(j) != Some(&'[') {
        return false;
    }
    j = skip_ws(chars, j.saturating_add(1));
    if chars
        .get(j..)
        .is_none_or(|rest| !rest.starts_with(&['t', 'e', 's', 't']))
    {
        return false;
    }
    j.saturating_add(4) >= chars.len()
        || chars
            .get(j.saturating_add(4))
            .is_none_or(|c| !is_test_ident_char(*c))
}

/// Skip whitespace from `j`.
fn skip_ws(chars: &[char], mut j: usize) -> usize {
    while chars
        .get(j)
        .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
    {
        j = j.saturating_add(1);
    }
    j
}

/// True if `c` continues an identifier in test-attribute position.
const fn is_test_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
