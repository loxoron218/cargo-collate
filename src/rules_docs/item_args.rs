//! Call-argument item forwarding detection.
//!
//! Mirrors the `mutates_static` skip inside `must_use_candidate`: a call that
//! forwards a non-local path (a function item such as `str::trim`, or a known
//! file-local or imported name) as an argument counts as a side effect, so
//! the lint stays silent and the attribute is unnecessary. Macro interiors,
//! closures, literals, uppercase paths, and leading-reference arguments stay
//! excluded.

use std::collections::BTreeSet;

use crate::{
    lexer::word::has_word,
    rules_docs::{
        code_of, file_fn_names, fn_body, imported_binders, is_filler, keywords::is_rust_keyword,
    },
};

/// Byte index of the first non-whitespace character at or before `i`, if any.
fn prev_code_idx(text: &str, mut i: usize) -> Option<usize> {
    while i > 0 {
        i = i.saturating_sub(1);
        if text
            .as_bytes()
            .get(i)
            .is_none_or(|b| !b.is_ascii_whitespace())
        {
            return Some(i);
        }
    }
    None
}

/// Byte index of the `<` matching turbofish close at `i`, if any.
fn turbofish_open(text: &str, i: usize) -> Option<usize> {
    if text.as_bytes().get(i) != Some(&b'>') {
        return None;
    }
    let mut depth: i32 = 0;
    let mut k = i.saturating_add(1);
    while k > 0 {
        k = k.saturating_sub(1);
        if text.as_bytes().get(k) == Some(&b'>') {
            depth = depth.saturating_add(1);
        }
        if text.as_bytes().get(k) == Some(&b'<') {
            depth = depth.saturating_sub(1);
        }
        if depth <= 0 {
            return Some(k);
        }
    }
    None
}

/// True if `(` at `open` opens a macro invocation.
fn is_macro_call(text: &str, open: usize) -> bool {
    prev_code_idx(text, open).is_some_and(|j| text.as_bytes().get(j) == Some(&b'!'))
}

/// Callee ident before `(` at `open` in `text`, unless absent, if any.
fn callee_ident(text: &str, open: usize) -> Option<String> {
    let mut end = prev_code_idx(text, open)?;
    if text.as_bytes().get(end) == Some(&b'>') {
        end = prev_code_idx(text, turbofish_open(text, end)?)?;
    }
    let bytes = text.as_bytes();
    let mut start = end.saturating_add(1);
    while start > 0
        && bytes
            .get(start.saturating_sub(1))
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        start = start.saturating_sub(1);
    }
    let mut name = text.get(start..end.saturating_add(1)).unwrap_or("");
    name = name.strip_prefix("r#").unwrap_or(name);
    (!name.is_empty()).then_some(name.to_owned())
}

/// Brace depth after one character.
const fn step_bracket_depth(depth: i32, c: char) -> i32 {
    if c == '(' || c == '[' || c == '{' {
        depth.saturating_add(1)
    } else if c == ')' || c == ']' || c == '}' {
        depth.saturating_sub(1)
    } else {
        depth
    }
}

/// Byte index of the `)` matching the call open at `open`, if any.
fn call_close(text: &str, open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    for (k, c) in text.get(open..)?.char_indices() {
        depth = step_bracket_depth(depth, c);
        if depth <= 0 {
            return Some(open.saturating_add(k));
        }
    }
    None
}

/// Top-level comma-separated arguments between call parens.
fn call_args(text: &str, open: usize, close: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Some(inner) = text.get(open.saturating_add(1)..close) else {
        return out;
    };
    let mut depth: i32 = 0;
    let mut start = 0;
    for (k, c) in inner.char_indices() {
        depth = step_bracket_depth(depth, c);
        if c == ',' && depth <= 0 {
            out.push(inner.get(start..k).unwrap_or("").to_owned());
            start = k.saturating_add(1);
        }
    }
    out.push(inner.get(start..).unwrap_or("").to_owned());
    out
}

/// True if `arg` is a closure (never examined for forwarding).
fn is_closure_arg(arg: &str) -> bool {
    let mut rest = arg.trim_start();
    for kw in ["async", "move"] {
        if rest.starts_with(kw)
            && rest
                .get(kw.len()..)
                .is_some_and(|t| t.starts_with(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
        {
            rest = rest.get(kw.len()..).unwrap_or("").trim_start();
        }
    }
    rest.starts_with('|')
}

/// Lowercase last segment of a path-shaped `text`, if any.
///
/// Uppercase endings (types, variants, constants) stay excluded, as do shapes
/// holding other tokens.
fn path_last_segment(text: &str) -> Option<String> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !compact
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '<' | '>' | '#' | '[' | ']'))
    {
        return None;
    }
    let (_, tail) = compact.rsplit_once("::")?;
    let head = tail.split('<').next().unwrap_or("");
    let head = head.strip_prefix("r#").unwrap_or(head);
    let mut chars = head.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return None,
    }
    head.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        .then_some(head.to_owned())
}

/// True if `arg` forwards a non-local path as a call argument.
fn is_item_arg(arg: &str, known: &BTreeSet<String>) -> bool {
    if is_closure_arg(arg) {
        return false;
    }
    let text = arg.trim();
    if let Some(rest) = text.strip_prefix('&') {
        let rest = rest.trim_start();
        let Some(tail) = rest.strip_prefix("mut") else {
            return false;
        };
        if tail
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            return false;
        }
        return is_item_arg(tail, known);
    }
    if text.contains("::") {
        return path_last_segment(text).is_some();
    }
    let head = text.strip_prefix("r#").unwrap_or(text);
    let mut chars = head.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if !head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    known.contains(head)
}

/// Byte index just past the balanced group opened at `open`, if any.
fn group_close(text: &str, open: usize) -> Option<usize> {
    call_close(text, open).map(|k| k.saturating_add(1))
}

/// Resume index after the construct opened at `open`, or `None` on forwarding.
///
/// Unparseable shapes (non-call parens, keywords, unbalanced groups) resume
/// past the paren so scanning continues; only a classified call with an item
/// argument reports forwarding.
fn scan_call(body: &str, open: usize, known: &BTreeSet<String>) -> Option<usize> {
    if is_macro_call(body, open) {
        return Some(group_close(body, open).unwrap_or_else(|| open.saturating_add(1)));
    }
    let Some(name) = callee_ident(body, open) else {
        return Some(open.saturating_add(1));
    };
    if is_rust_keyword(&name) {
        return Some(open.saturating_add(1));
    }
    let Some(close) = call_close(body, open) else {
        return Some(open.saturating_add(1));
    };
    if call_args(body, open, close)
        .iter()
        .any(|arg| is_item_arg(arg, known))
    {
        return None;
    }
    Some(close.saturating_add(1))
}

/// True if the `fn` under the attribute at `attr_idx` forwards a non-local
/// path as a call argument anywhere in its body.
#[must_use]
pub fn body_forwards_item(lines: &[String], attr_idx: usize) -> bool {
    let known: BTreeSet<String> = file_fn_names(lines)
        .union(&imported_binders(lines))
        .cloned()
        .collect();
    let Some(fn_idx) = fn_body_start(lines, attr_idx) else {
        return false;
    };
    let body = fn_body(lines, fn_idx);
    let mut i = 0;
    while i < body.len() {
        let Some(rel) = body.get(i..).and_then(|t| t.find('(')) else {
            break;
        };
        let Some(next) = scan_call(&body, i.saturating_add(rel), &known) else {
            return true;
        };
        i = next;
    }
    false
}

/// Index of the `fn` line after the attribute at `attr_idx`, if any.
fn fn_body_start(lines: &[String], attr_idx: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(attr_idx.saturating_add(1))
        .take(20)
        .filter(|(_, line)| !is_filler(code_of(line).trim()))
        .find(|(_, line)| has_word(&code_of(line), "fn"))
        .map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use crate::{rules_docs::item_args::body_forwards_item, scan::support::test_file};

    #[test]
    fn forwards_path_flagged() {
        let file = test_file(
            "src/a.rs",
            &[
                "fn helper(x: i32) -> i32 { x }",
                "#[must_use]",
                "pub fn f(a: &str) -> Vec<String> {",
                "    a.split(',').map(str::trim).collect()",
                "}",
            ],
        );
        assert!(
            body_forwards_item(&file.lines, 1),
            "associated function forwarding flags"
        );
        let bare = test_file(
            "src/b.rs",
            &[
                "fn helper(x: i32) -> i32 { x }",
                "#[must_use]",
                "pub fn g(v: Vec<i32>) -> Vec<i32> {",
                "    v.into_iter().map(helper).collect()",
                "}",
            ],
        );
        assert!(
            body_forwards_item(&bare.lines, 1),
            "bare helper forwarding flags"
        );
    }

    #[test]
    fn non_forwarding_silent() {
        let file = test_file(
            "src/a.rs",
            &[
                "#[must_use]",
                "pub fn f(v: Vec<i32>) -> usize {",
                "    v.into_iter().filter(|x| *x > 1).count()",
                "}",
            ],
        );
        assert!(
            !body_forwards_item(&file.lines, 0),
            "closures and literals stay silent"
        );
        let upper = test_file(
            "src/b.rs",
            &[
                "#[must_use]",
                "pub fn g() -> u64 {",
                "    foo(u32::MAX)",
                "}",
            ],
        );
        assert!(
            !body_forwards_item(&upper.lines, 0),
            "uppercase constants stay silent"
        );
        let shared = test_file(
            "src/c.rs",
            &[
                "#[must_use]",
                "pub fn h(x: &str) -> bool {",
                "    starts_with(x)",
                "}",
            ],
        );
        assert!(
            !body_forwards_item(&shared.lines, 0),
            "local bindings stay silent"
        );
    }

    #[test]
    fn non_call_parens_silent() {
        let file = test_file(
            "src/a.rs",
            &[
                "#[must_use]",
                "pub fn f(lines: &[String]) -> Vec<(usize, String)> {",
                "    let mut out = Vec::new();",
                "    for (idx, line) in lines.iter().enumerate() {",
                "        out.push(line.to_owned());",
                "    }",
                "    out",
                "}",
            ],
        );
        assert!(
            !body_forwards_item(&file.lines, 0),
            "control-flow and type parens stay silent"
        );
    }
}
