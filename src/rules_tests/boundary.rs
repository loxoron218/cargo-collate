//! Boundary checks for error types in test versus library code.
//!
//! Three focused lints replace the former whole-file exemption:
//! public-API leaks, wrong-location use, and missing error context.

use std::collections::HashSet;

use crate::{
    gating::gated_stems,
    lexer::{
        visibility::{find_mod_semi, strip_pub_block},
        word::has_word,
    },
    rules_simple::is_real_cfg_line,
    rules_tests::{
        order::strip_attrs,
        range::{anyhow_gap, code_of, in_ranges, test_ranges},
    },
    scan::SourceFile,
};

/// True if `path` is exempt from the error-type rule by location.
///
/// Only integration tests, binary entries, and inline test modules count;
/// `benches/`, `examples/`, and `build.rs` stay treated as library code.
fn anyhow_exempt_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.ends_with("src/main.rs")
        || path.contains("/src/bin/")
        || path.starts_with("src/bin/")
}

/// True if `path` is a binary entry that must attach error context.
///
/// Integration tests are exempt from the context rule because `bail!` and
/// `ensure!` without context stay idiomatic there.
fn is_binary_entry(path: &str) -> bool {
    path.ends_with("src/main.rs") || path.contains("/src/bin/") || path.starts_with("src/bin/")
}

/// True if the whole file is a parent-gated file module.
fn whole_file_gated(lines: &[String]) -> bool {
    lines
        .iter()
        .enumerate()
        .any(|(i, line)| is_real_cfg_line(line) && gated_file_mod(lines, i).is_some())
}

/// File-module declaration within six lines after a marker at `i`, if any.
fn gated_file_mod(lines: &[String], i: usize) -> Option<String> {
    let mut j = i.saturating_add(1);
    while lines.get(j).is_some_and(|line| anyhow_gap(line)) {
        j = j.saturating_add(1);
        if j > i.saturating_add(6) {
            break;
        }
    }
    (j < lines.len()
        && lines.get(j).is_some_and(|line| has_word(line, "mod"))
        && lines
            .get(j)
            .is_some_and(|line| line.trim_end().ends_with(';')))
    .then(|| {
        let line = lines.get(j)?;
        find_mod_semi(line)
    })
    .flatten()
}

/// True if `file` is gated behind a cfg-test file-module declaration.
fn is_stem_gated(file: &SourceFile, stems: &HashSet<String>) -> bool {
    let stem = file.path.rsplit('/').next().unwrap_or(&file.path);
    stems.contains(stem.strip_suffix(".rs").unwrap_or(stem))
}

/// Non-test line ranges for `file`.
fn live_ranges(file: &SourceFile) -> Vec<(usize, usize)> {
    test_ranges(&file.lines, anyhow_gap)
}

/// True if `file` needs no boundary scan at all.
fn is_fully_exempt(file: &SourceFile, stems: &HashSet<String>) -> bool {
    anyhow_exempt_path(&file.path) || is_stem_gated(file, stems) || whole_file_gated(&file.lines)
}

/// True if stripped `code` declares a `pub` item.
fn is_pub_item(code: &str) -> bool {
    let stripped = strip_attrs(code);
    strip_pub_block(stripped.trim_start()).is_some()
}

/// True if `c` continues an ASCII identifier.
const fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if `name` matches at `start` with identifier boundaries.
fn name_at(chars: &[char], name: &[char], start: usize) -> bool {
    let end = start.saturating_add(name.len());
    if chars
        .get(start..end)
        .is_none_or(|body| body.iter().zip(name.iter()).any(|(a, b)| a != b))
    {
        return false;
    }
    let before_ok = start == 0
        || chars
            .get(start.saturating_sub(1))
            .is_some_and(|p| !is_ident_char(*p));
    let after_ok = chars.get(end).is_none_or(|c| !is_ident_char(*c));
    before_ok && after_ok
}

/// True if `code` calls macro `name` with a `!`.
fn has_bang_macro(code: &str, name: &str) -> bool {
    if !has_word(code, name) {
        return false;
    }
    let chars: Vec<char> = code.chars().collect();
    let want: Vec<char> = name.chars().collect();
    if want.is_empty() || chars.len() < want.len() {
        return false;
    }
    let last = chars.len().saturating_sub(want.len());
    (0..=last)
        .any(|i| name_at(&chars, &want, i) && bang_after(&chars, i.saturating_add(want.len())))
}

/// True if a macro bang opens after optional whitespace at `from`.
fn bang_after(chars: &[char], from: usize) -> bool {
    let mut j = from;
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j = j.saturating_add(1);
    }
    chars.get(j).is_some_and(|c| *c == '!')
}

/// True if stripped `code` mentions the `anyhow` error type.
fn mentions_anyhow(code: &str) -> bool {
    has_word(code, "anyhow") || has_bang_macro(code, "bail") || has_bang_macro(code, "ensure")
}

/// True if `code` holds a `?` error propagation.
///
/// A `?` directly followed by identifier characters stays excluded since it
/// opens a bound or macro sigil rather than propagating an error.
fn has_bare_question(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars.get(i).is_none_or(|c| *c != '?') {
            i = i.saturating_add(1);
            continue;
        }
        if question_opens_ident(&chars, i) {
            i = skip_ident(&chars, i.saturating_add(1));
            continue;
        }
        return true;
    }
    false
}

/// True if the `?` at `q` is directly followed by an identifier character.
///
/// Such a `?` never denotes the postfix try operator: it opens a `?Sized`
/// or `?const` bound, or a macro sigil such as tracing's `?field`.
fn question_opens_ident(chars: &[char], q: usize) -> bool {
    chars
        .get(q.saturating_add(1))
        .is_some_and(|c| is_ident_char(*c))
}

/// Index after the identifier starting at `start`.
fn skip_ident(chars: &[char], start: usize) -> usize {
    let mut j = start;
    while chars.get(j).is_some_and(|c| is_ident_char(*c)) {
        j = j.saturating_add(1);
    }
    j
}

/// True if `text` attaches context through the `Context` trait.
///
/// Complete anyhow context API: `.context()` and `.with_context()`.
fn has_context_call(text: &str) -> bool {
    text.contains(".context(") || text.contains(".with_context(")
}

/// True if `code` closes a scan statement.
fn closes_statement(code: &str) -> bool {
    code.contains(';') || code.contains('{') || code.contains('}')
}

/// Findings for one statement group without context.
///
/// Each group entry pairs its line index with that line's blanked code, so
/// `?` inside multi-line raw strings never attributes to a hit.
fn bare_group_hits(file: &SourceFile, group: &[(usize, String)], joined: &str) -> Vec<String> {
    if !has_bare_question(joined) || has_context_call(joined) {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|(idx, code)| {
            let line = file.lines.get(*idx)?;
            has_bare_question(code)
                .then(|| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        })
        .collect()
}

/// True if `file` uses `anyhow` outside test ranges, so context applies.
///
/// Binaries propagating only typed errors never need `.context()`, which
/// requires anyhow's `Context` trait in scope.
fn uses_anyhow(file: &SourceFile, ranges: &[(usize, usize)]) -> bool {
    file.stripped.iter().enumerate().any(|(idx, line)| {
        if in_ranges(ranges, idx) {
            return false;
        }
        let code = code_of(line);
        has_word(&code, "anyhow") || has_word(&code, "Context")
    })
}

/// Missing-context findings for one binary entry file.
///
/// Analysis runs on precomputed blanked lines so `?` inside multi-line raw
/// strings never flags; display keeps original lines.
fn missing_context_in_file(file: &SourceFile, ranges: &[(usize, usize)]) -> Vec<String> {
    if !uses_anyhow(file, ranges) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut group: Vec<(usize, String)> = Vec::new();
    let mut joined = String::new();
    for (idx, line) in file.stripped.iter().enumerate() {
        if in_ranges(ranges, idx) {
            continue;
        }
        let code = code_of(line);
        if code.trim().is_empty() {
            continue;
        }
        joined.push_str(&code);
        joined.push('\n');
        group.push((idx, code.clone()));
        if closes_statement(&code) {
            out.extend(bare_group_hits(file, &group, &joined));
            group.clear();
            joined.clear();
        }
    }
    out.extend(bare_group_hits(file, &group, &joined));
    out
}

/// True if stripped `code` leaks `anyhow` through a `pub` item.
fn is_leak_line(code: &str) -> bool {
    has_word(code, "anyhow") && is_pub_item(code)
}

/// Findings in `file` outside `ranges` where `pred` holds on stripped code.
fn file_line_hits(
    file: &SourceFile,
    ranges: &[(usize, usize)],
    pred: fn(&str) -> bool,
) -> Vec<String> {
    file.lines
        .iter()
        .enumerate()
        .filter(|(idx, line)| !in_ranges(ranges, *idx) && pred(&code_of(line)))
        .map(|(idx, line)| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        .collect()
}

/// Filtered `anyhow` findings; `leak_only` selects the `pub`-item predicate.
fn anyhow_filtered(files: &[SourceFile], leak_only: bool) -> Vec<String> {
    let stems: HashSet<String> = gated_stems(files);
    let pred: fn(&str) -> bool = if leak_only {
        is_leak_line
    } else {
        mentions_anyhow
    };
    files
        .iter()
        .filter(|file| !is_fully_exempt(file, &stems))
        .flat_map(|file| file_line_hits(file, &live_ranges(file), pred))
        .collect()
}

/// `anyhow::Error` exposed through a `pub` item outside allowed locations.
///
/// Binary entries and test code stay exempt; everything else must use typed
/// `thiserror` enums instead of leaking `anyhow` across module boundaries.
#[must_use]
pub fn anyhow_leak(files: &[SourceFile]) -> Vec<String> {
    anyhow_filtered(files, true)
}

/// `anyhow` use outside tests and binary entries.
///
/// Only `tests/`, `src/main.rs`, `src/bin/`, and inline `#[cfg(test)]`
/// modules may mention `anyhow`; library code must use typed errors.
#[must_use]
pub fn anyhow_site(files: &[SourceFile]) -> Vec<String> {
    anyhow_filtered(files, false)
}

/// Bare `?` without `.context()` or `.with_context()` in binary entries.
///
/// Every fallible propagation in `src/main.rs` and `src/bin/` files that use
/// `anyhow` must attach context so errors stay actionable. Binaries without
/// `anyhow` and test modules stay exempt; errors built inline with an anyhow
/// message macro (`anyhow!`, `bail!`, `ensure!`) must carry `.context()` like
/// any other propagation.
#[must_use]
pub fn missing_context(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .filter(|file| is_binary_entry(&file.path))
        .flat_map(|file| {
            let ranges = live_ranges(file);
            missing_context_in_file(file, &ranges)
        })
        .collect()
}
