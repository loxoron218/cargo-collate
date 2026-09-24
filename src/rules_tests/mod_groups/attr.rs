//! Multi-line attribute-block helpers for file modules.
//!
//! Attributes like `#[cfg(any(...))]` span several lines, so attachment walks
//! resolve whole bracket-balanced blocks instead of single lines.

use crate::{
    lexer::word::{has_word, word_match_at},
    rules_simple::is_real_cfg_line,
    rules_tests::{
        order::{match_mod_head, strip_attrs},
        range::{brace_count, code_of},
    },
    scan::SourceFile,
};

/// Maximum lines scanned backwards for one attribute block.
const MAX_ATTR_BLOCK_LINES: usize = 16;

/// Start line of the attribute block ending at `end`, if any.
#[must_use]
pub fn attr_block_start_ending_at(file: &SourceFile, end: usize) -> Option<usize> {
    let mut need: i32 = 0;
    let low = end.saturating_sub(MAX_ATTR_BLOCK_LINES);
    let mut idx = end;
    loop {
        let line = file.stripped.get(idx)?;
        let code = code_of(line);
        need = need
            .saturating_add(brace_count(&code, ']'))
            .saturating_sub(brace_count(&code, '['));
        if need < 0 {
            return None;
        }
        if need == 0 {
            return code.trim_start().starts_with('#').then_some(idx);
        }
        if idx == low {
            return None;
        }
        idx = idx.checked_sub(1)?;
    }
}

/// Byte index of a `mod` head trailing an attribute on the same line, if any.
///
/// Matches shapes like `...)] mod foo;` where the `#[...]` opened above; the
/// same-line prefix must hold a closing bracket, otherwise the direct
/// single-line match owns the case.
#[must_use]
pub fn trailing_attr_mod_pos(code: &str) -> Option<usize> {
    for (i, _) in code.match_indices("mod") {
        if !word_match_at(code, "mod", i) {
            continue;
        }
        if code.get(..i).is_some_and(|prefix| prefix.contains(']')) {
            return Some(i);
        }
    }
    None
}

/// Start line of a trailing attribute closing before `mod_pos` on `idx`.
///
/// Balances the same-line prefix plus contiguous lines above, succeeding only
/// on a `#` opener with balanced brackets and no blank or comment gap; lines
/// holding `;` abort the walk since attributes never contain them.
#[must_use]
pub fn resolve_trailing_attr_start(file: &SourceFile, idx: usize, mod_pos: usize) -> Option<usize> {
    let line = file.stripped.get(idx)?;
    let code = code_of(line);
    let prefix = code.get(..mod_pos)?;
    let mut need = brace_count(prefix, ']').saturating_sub(brace_count(prefix, '['));
    if need <= 0 {
        return None;
    }
    let low = idx.saturating_sub(MAX_ATTR_BLOCK_LINES);
    let mut cursor = idx.checked_sub(1)?;
    loop {
        let above = file.stripped.get(cursor)?;
        let above_code = code_of(above);
        if above_code.trim().is_empty() || above_code.contains(';') {
            return None;
        }
        need = need
            .saturating_add(brace_count(&above_code, ']'))
            .saturating_sub(brace_count(&above_code, '['));
        if need < 0 {
            return None;
        }
        if need == 0 {
            return above_code.trim_start().starts_with('#').then_some(cursor);
        }
        if cursor == low {
            return None;
        }
        cursor = cursor.checked_sub(1)?;
    }
}

/// True if attribute-block text holds no module declaration.
#[must_use]
pub fn is_pure_attr_text(text: &str) -> bool {
    if match_mod_head(&strip_attrs(text)).is_some() {
        return false;
    }
    let Some(suffix) = text
        .rfind(']')
        .and_then(|pos| text.get(pos.saturating_add(1)..))
    else {
        return true;
    };
    match_mod_head(suffix.trim_start()).is_none()
}

/// Joined lines `start..=end`, blanked code or originals, if all present.
fn join_block_lines(file: &SourceFile, start: usize, end: usize, stripped: bool) -> Option<String> {
    let mut joined = String::new();
    for idx in start..=end {
        let line = if stripped {
            file.stripped.get(idx)?
        } else {
            file.lines.get(idx)?
        };
        if stripped {
            joined.push_str(&code_of(line));
        } else {
            joined.push_str(line);
        }
        joined.push('\n');
    }
    Some(joined)
}

/// True if lines `start..=end` form attribute text without a module declaration.
#[must_use]
pub fn is_pure_block(file: &SourceFile, start: usize, end: usize) -> bool {
    join_block_lines(file, start, end, true).is_some_and(|joined| is_pure_attr_text(&joined))
}

/// True if the block at `start..=end` holds a `cfg(test)` marker.
#[must_use]
pub fn block_has_cfg_test(file: &SourceFile, start: usize, end: usize) -> bool {
    join_block_lines(file, start, end, false).is_some_and(|joined| is_real_cfg_line(&joined))
}

/// True if the block at `start..=end` mentions `macro_use`.
#[must_use]
pub fn block_has_macro_use(file: &SourceFile, start: usize, end: usize) -> bool {
    join_block_lines(file, start, end, true)
        .is_some_and(|joined| joined.contains("#[") && has_word(&joined, "macro_use"))
}
