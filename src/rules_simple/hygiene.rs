//! Low-level hygiene detectors for error swallows and paths.

use crate::{
    lexer::{
        literal::strip_strings,
        marker::{cut_line_comment, is_doc_line},
        word::has_word,
    },
    rules_tests::range::brace_count,
    scan::{SourceFile, indexed_lines},
};

/// Scan state for doc blank-line checks in one file.
#[derive(Default)]
struct DocBlankState {
    /// Current brace depth before the pending line.
    depth: i32,
    /// Open frames as exempt struct/enum body plus depth before its opener.
    frames: Vec<(bool, i32)>,
    /// True when a struct/enum header without its opener was seen.
    pending: bool,
}

/// Collect findings for lines matching `pred` after blanking.
///
/// Iterates precomputed analysis lines with original lines zipped for
/// display, so multi-line raw-string interiors never scan as code.
pub fn line_hits(files: &[SourceFile], pred: fn(&str) -> bool) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file).filter_map(|(idx, (line, code))| {
                let stripped = strip_strings(code);
                let tail = cut_line_comment(&stripped);
                pred(tail).then(|| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
            })
        })
        .collect()
}

/// True if `code` holds a dot-ok error swallow outside strings.
#[must_use]
pub fn has_dot_ok(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        chars.get(i).is_some_and(|c| *c == '.')
            && chars.get(i.saturating_add(1)) == Some(&'o')
            && chars.get(i.saturating_add(2)) == Some(&'k')
            && chars.get(i.saturating_add(3)) == Some(&'(')
            && chars.get(i.saturating_add(4)) == Some(&')')
    })
}

/// Dot-ok findings across files, strings and line tails ignored.
pub fn dot_ok_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_dot_ok)
}

/// True if `marker` opens at `i` with a gap before it.
fn marker_at(code: &str, chars: &[char], i: usize, marker: &str) -> bool {
    code.get(i..).is_some_and(|rest| rest.starts_with(marker)) && gap_before_at(chars, i)
}

/// True if `code` holds a parent-module path outside strings.
#[must_use]
pub fn has_parent_path(code: &str) -> bool {
    if !has_word(code, "super") {
        return false;
    }
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        marker_at(code, &chars, i, "super")
            && chars.get(i.saturating_add(5)) == Some(&':')
            && chars.get(i.saturating_add(6)) == Some(&':')
    })
}

/// True if `c` continues an identifier.
const fn is_ident_char_at(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if the char before `i` ends any identifier run.
fn gap_before_at(chars: &[char], i: usize) -> bool {
    i == 0
        || chars
            .get(i.saturating_sub(1))
            .is_some_and(|p| !is_ident_char_at(*p))
}

/// Parent-module findings across files, strings and line tails ignored.
pub fn parent_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_parent_path)
}

/// True if a scoped visibility opens at char `i`.
fn opens_scope_at(code: &str, chars: &[char], i: usize) -> bool {
    if !code.get(i..).is_some_and(|rest| rest.starts_with("pub")) {
        return false;
    }
    if chars
        .get(i.saturating_add(3))
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        return false;
    }
    if i > 0
        && chars
            .get(i.saturating_sub(1))
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        return false;
    }
    let mut j = i.saturating_add(3);
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j = j.saturating_add(1);
    }
    chars.get(j).is_some_and(|c| *c == '(')
}

/// True if `code` holds scoped pub visibility outside strings and tails.
#[must_use]
pub fn has_scoped_pub(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| opens_scope_at(code, &chars, i))
}

/// Scoped-visibility findings across files, strings and line tails ignored.
pub fn scoped_pub_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_scoped_pub)
}

/// True if `code` holds a path attribute outside strings.
///
/// Only `path` as the first token after the opener counts; arguments
/// named `path` inside other attributes stay excluded.
pub fn has_path_attr(code: &str) -> bool {
    let mut from = 0;
    while let Some(tail) = code.get(from..) {
        let Some(rel) = tail.find("#[") else {
            return false;
        };
        let mut j = from.saturating_add(rel).saturating_add(2);
        while code.as_bytes().get(j).is_some_and(u8::is_ascii_whitespace) {
            j = j.saturating_add(1);
        }
        if code.get(j..).is_some_and(|rest| rest.starts_with("path"))
            && code
                .as_bytes()
                .get(j.saturating_add(4))
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
        {
            return true;
        }
        from = j.saturating_add(1);
    }
    false
}

/// True if `code` holds an include macro call outside strings.
#[must_use]
pub fn has_include_macro(code: &str) -> bool {
    if !has_word(code, "include") {
        return false;
    }
    let chars: Vec<char> = code.chars().collect();
    (0..chars.len()).any(|i| {
        marker_at(code, &chars, i, "include") && chars.get(i.saturating_add(7)) == Some(&'!')
    })
}

/// True if `code` holds either path marker outside strings.
fn has_path_include(code: &str) -> bool {
    has_path_attr(code) || has_include_macro(code)
}

/// Path-attribute and include-macro findings across files.
pub fn path_include_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_path_include)
}

/// True if `code` holds low-level logging outside strings.
#[must_use]
pub fn has_low_log(code: &str) -> bool {
    for marker in ["trace", "debug"] {
        if !has_word(code, marker) {
            continue;
        }
        let chars: Vec<char> = code.chars().collect();
        let found = (0..chars.len()).any(|i| {
            marker_at(code, &chars, i, marker)
                && chars.get(i.saturating_add(marker.len())) == Some(&'!')
        });
        if found {
            return true;
        }
    }
    false
}

/// Low-level logging findings across files, strings and tails ignored.
pub fn low_log_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_low_log)
}

/// True if stripped `code` declares a struct, enum, or union item.
///
/// Unions share struct-like field bodies, so their docs follow the same
/// blank-line rules. Reference: Rust Reference items
/// (`doc.rust-lang.org/reference/items.html`).
fn is_struct_enum_head(code: &str) -> bool {
    has_word(code, "struct") || has_word(code, "enum") || has_word(code, "union")
}

/// True if stripped `code` opens another item that cancels a pending header.
///
/// Complete item-head set (struct/enum handled separately via
/// [`is_struct_enum_head`]): `fn`, `impl`, `mod`, `trait`, `type`, `const`,
/// `static`, `use`, `macro`, plus `union` and `extern` which also open items.
/// Reference: Rust Reference items
/// (`doc.rust-lang.org/reference/items.html`).
fn is_other_item_head(code: &str) -> bool {
    [
        "fn", "impl", "mod", "trait", "type", "const", "static", "use", "macro", "union", "extern",
    ]
    .iter()
    .any(|kw| has_word(code, kw))
}

/// True if the scan sits inside a struct/enum body.
///
/// Any enclosing frame counts, so field docs inside struct-variant bodies of
/// enums stay exempt even though the variant brace itself is not a struct or
/// enum head. Closed frames pop off the stack, so only open bodies exempt.
fn inside_struct_enum(state: &DocBlankState) -> bool {
    state
        .frames
        .iter()
        .any(|(exempt, entry)| *exempt && state.depth > *entry)
}

/// True if the doc line at `idx` is a block opener missing its blank line.
///
/// Continuations after another doc line, openers at the start of file,
/// openers directly after an empty line, and openers starting a brace or
/// paren block stay allowed. The brace arm is required because `rustfmt`
/// forbids the blank line there; the paren arm covers item-generating macro
/// invocations such as `signed_request!(` whose docs route into the output.
fn is_flagged_opener(lines: &[String], idx: usize) -> bool {
    let Some(line) = lines.get(idx) else {
        return false;
    };
    if !is_doc_line(&strip_strings(line)) {
        return false;
    }
    let Some(prev_idx) = idx.checked_sub(1) else {
        return false;
    };
    let Some(prev) = lines.get(prev_idx) else {
        return false;
    };
    if is_doc_line(&strip_strings(prev)) {
        return false;
    }
    if prev.trim().is_empty() {
        return false;
    }
    !is_block_opener(prev)
}

/// True if stripped `prev` opens a brace or paren block for the next line.
fn is_block_opener(prev: &str) -> bool {
    let stripped = strip_strings(prev);
    let opened = cut_line_comment(&stripped).trim_end();
    opened.ends_with('{') || opened.ends_with('(')
}

/// Push one brace frame; returns the depth after opening.
fn push_frame(state: &mut DocBlankState, exempt: bool) {
    state.frames.push((exempt, state.depth));
    state.depth = state.depth.saturating_add(1);
}

/// Drop the innermost brace frame, if any.
fn pop_one_frame(state: &mut DocBlankState) {
    if let Some(next) = state.frames.len().checked_sub(1) {
        state.frames.truncate(next);
    }
}

/// Close `closes` braces, dropping frames whose opener left the depth.
fn pop_frames(state: &mut DocBlankState, closes: i32) {
    let mut remaining = closes;
    while remaining > 0 {
        state.depth = state.depth.saturating_sub(1);
        while state
            .frames
            .last()
            .is_some_and(|(_, entry)| *entry >= state.depth)
        {
            pop_one_frame(state);
        }
        remaining = remaining.saturating_sub(1);
    }
}

/// Whether the first opener on `code` belongs to a struct/enum body.
fn first_opener_exempt(state: &mut DocBlankState, code: &str, opens: i32) -> bool {
    if opens <= 0 {
        return false;
    }
    if is_struct_enum_head(code) {
        state.pending = false;
        return true;
    }
    if state.pending && !is_other_item_head(code) {
        state.pending = false;
        return true;
    }
    if is_other_item_head(code) {
        state.pending = false;
    }
    false
}

/// Track a struct/enum header without its opener on `code`.
///
/// Only a `{` line (resolved by `first_opener_exempt`) or a `;` line may
/// resolve a pending header. Where-clause contents such as `'static`
/// lifetimes or `fn` bounds never cancel it, since keyword matching cannot
/// tell them apart from new item heads.
fn track_pending(state: &mut DocBlankState, code: &str, opens: i32) {
    if code.contains(';') {
        state.pending = false;
        return;
    }
    if opens > 0 {
        return;
    }
    if is_struct_enum_head(code) {
        state.pending = true;
    }
}

/// Advance brace depth and struct/enum frames from one stripped line.
fn track_braces(state: &mut DocBlankState, stripped: &str) {
    let code = cut_line_comment(stripped);
    let opens = brace_count(code, '{');
    let closes = brace_count(code, '}');
    if opens <= 0 {
        track_pending(state, code, opens);
    } else {
        let exempt_first = first_opener_exempt(state, code, opens);
        let mut remaining = opens;
        let mut first = true;
        while remaining > 0 {
            push_frame(state, first && exempt_first);
            first = false;
            remaining = remaining.saturating_sub(1);
        }
        if code.contains(';') {
            state.pending = false;
        }
    }
    pop_frames(state, closes);
}

/// Doc blank-line findings for one file.
fn check_file_doc_blank(file: &SourceFile) -> Vec<String> {
    let mut state = DocBlankState::default();
    let mut out = Vec::new();
    for (idx, (line, code)) in file.lines.iter().zip(file.stripped.iter()).enumerate() {
        let stripped = strip_strings(code);
        if is_doc_line(&stripped)
            && is_flagged_opener(&file.stripped, idx)
            && !inside_struct_enum(&state)
        {
            out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
        }
        track_braces(&mut state, &stripped);
    }
    out
}

/// Doc `///` openers without a preceding blank line.
///
/// Only the first line of a consecutive doc block is checked; continuations,
/// quadruple `////` lines, field docs anywhere inside struct/enum bodies
/// (including struct-variant bodies), and docs opening a brace or paren
/// block stay exempt. A doc line at the start of file is allowed.
pub fn doc_blank_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_doc_blank).collect()
}
