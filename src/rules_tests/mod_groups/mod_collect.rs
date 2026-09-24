//! File-module collection with group ranks.
//!
//! Scans top-level file declarations and attaches leading attributes to
//! classify each statement as macro-use, ordinary, or cfg-test.

use crate::{
    lexer::word::has_word,
    rules_simple::is_real_cfg_line,
    rules_tests::{
        mod_groups::{
            CFG_TEST_GROUP, MACRO_USE_GROUP, ORDINARY_GROUP,
            attr::{
                attr_block_start_ending_at, block_has_cfg_test, is_pure_attr_text, is_pure_block,
                resolve_trailing_attr_start, trailing_attr_mod_pos,
            },
        },
        order::{is_inline_mod, match_mod_head, skip_inline, strip_attrs},
        range::{brace_delta, code_of},
    },
    scan::SourceFile,
};

/// Collection state for file-module statements in one file.
#[derive(Default)]
struct ModCollectState {
    /// Current brace nesting depth.
    depth: i32,
    /// Collected file-module statements.
    stmts: Vec<ModStmt>,
}

/// One top-level file-module statement with its attached attributes.
#[derive(Debug, Clone, Copy)]
pub struct ModStmt {
    /// Zero-based line of the first attached attribute or the mod itself.
    pub attr_start: usize,
    /// Zero-based line of the `mod` declaration.
    pub mod_idx: usize,
    /// Zero-based line holding the terminating `;`.
    pub end: usize,
    /// Group rank from macro-use to cfg-test.
    pub group: u8,
}

/// Top-level file-module statements of one file with group ranks.
#[must_use]
pub fn collect_file_mods(file: &SourceFile) -> Vec<ModStmt> {
    let mut state = ModCollectState::default();
    let mut idx = 0;
    while idx < file.lines.len() {
        idx = collect_step(file, idx, &mut state);
    }
    state.stmts
}

/// Advance file-mod collection by one line; returns the next index.
fn collect_step(file: &SourceFile, idx: usize, state: &mut ModCollectState) -> usize {
    let Some(stripped_line) = file.stripped.get(idx) else {
        return idx.saturating_add(1);
    };
    let code = code_of(stripped_line);
    if state.depth != 0 {
        state.depth = state.depth.saturating_add(brace_delta(&code)).max(0);
        return idx.saturating_add(1);
    }
    let cleaned = strip_attrs(&code);
    if let Some((_, rest)) = match_mod_head(&cleaned) {
        return push_file_mod(file, idx, rest, attached_start(file, idx), state);
    }
    if let Some((attr_start, rest)) = match_trailing_mod(file, idx, &code) {
        return push_file_mod(file, idx, &rest, attr_start, state);
    }
    state.depth = state.depth.saturating_add(brace_delta(&code)).max(0);
    idx.saturating_add(1)
}

/// Match a trailing-attribute mod whose `#[...]` closes on its own line.
///
/// Returns the attribute-block start plus the owned remainder after the mod
/// name for shapes like `...)] mod foo;` where the opener sits above.
fn match_trailing_mod(file: &SourceFile, idx: usize, code: &str) -> Option<(usize, String)> {
    let mod_pos = trailing_attr_mod_pos(code)?;
    let suffix = code.get(mod_pos..)?.trim_start();
    let (_, rest) = match_mod_head(suffix)?;
    let seed = trailing_block_start(file, idx)?;
    let attr_start = attached_from(file, seed);
    (attr_start < idx).then(|| (attr_start, rest.to_owned()))
}

/// Push one top-level file mod or skip inline; returns the next index.
fn push_file_mod(
    file: &SourceFile,
    idx: usize,
    rest: &str,
    attr_start: usize,
    state: &mut ModCollectState,
) -> usize {
    if is_inline_mod(&file.stripped, idx, rest) {
        return skip_inline(&file.stripped, idx);
    }
    let end = file_mod_end(&file.stripped, idx, rest);
    let group = mod_group_for(file, attr_start, idx);
    state.stmts.push(ModStmt {
        attr_start,
        mod_idx: idx,
        end,
        group,
    });
    track_file_mod_depth(file, idx, state);
    end.saturating_add(1)
}

/// Track brace depth after a file-mod declaration at `idx`.
fn track_file_mod_depth(file: &SourceFile, idx: usize, state: &mut ModCollectState) {
    let Some(line) = file.stripped.get(idx) else {
        return;
    };
    state.depth = state
        .depth
        .saturating_add(brace_delta(&code_of(line)))
        .max(0);
}

/// Start line of a multi-line attribute closing on the mod line itself, if any.
fn trailing_block_start(file: &SourceFile, mod_idx: usize) -> Option<usize> {
    let line = file.stripped.get(mod_idx)?;
    let code = code_of(line);
    let mod_pos = trailing_attr_mod_pos(&code)?;
    let block_start = resolve_trailing_attr_start(file, mod_idx, mod_pos)?;
    let mut joined = String::new();
    for line_idx in block_start..mod_idx {
        joined.push_str(&code_of(file.stripped.get(line_idx)?));
        joined.push('\n');
    }
    joined.push_str(code.get(..mod_pos).unwrap_or(""));
    is_pure_attr_text(&joined).then_some(block_start)
}

/// First line of the contiguous attribute blocks directly above `mod_idx`.
///
/// Blocks may span several lines (e.g. multi-line `#[cfg(any(...))]`), so each
/// step resolves the whole bracket-balanced block ending at the previous line.
/// A block closing on the mod line itself (e.g. `...)] mod foo;`) seeds the
/// walk before lines above are considered.
fn attached_start(file: &SourceFile, mod_idx: usize) -> usize {
    attached_from(file, trailing_block_start(file, mod_idx).unwrap_or(mod_idx))
}

/// Walk contiguous pure attribute blocks above `start`.
fn attached_from(file: &SourceFile, mut start: usize) -> usize {
    while let Some(prev) = start.checked_sub(1) {
        let Some(line) = file.stripped.get(prev) else {
            break;
        };
        if code_of(line).trim().is_empty() {
            break;
        }
        let Some(block_start) = attr_block_start_ending_at(file, prev) else {
            break;
        };
        if !is_pure_block(file, block_start, prev) {
            break;
        }
        start = block_start;
    }
    start
}

/// True if attribute `code` mentions `macro_use`.
fn is_macro_use_attr(code: &str) -> bool {
    code.contains("#[") && has_word(code, "macro_use")
}

/// True if the attribute prefix before `mod` mentions `macro_use`.
fn prefix_has_macro_use(code: &str) -> bool {
    let cleaned = strip_attrs(code);
    let trimmed = code.trim_start();
    let prefix_len = trimmed.len().saturating_sub(cleaned.len());
    let prefix = trimmed.get(..prefix_len).unwrap_or("");
    is_macro_use_attr(prefix)
}

/// Group rank for the file mod at `mod_idx` with attached block at `attr_start`.
///
/// The joined fallback also catches a `cfg(test)` marker split across the
/// lines of one multi-line attribute block.
fn mod_group_for(file: &SourceFile, attr_start: usize, mod_idx: usize) -> u8 {
    for idx in attr_start..=mod_idx {
        if file
            .lines
            .get(idx)
            .is_some_and(|line| is_real_cfg_line(line))
        {
            return CFG_TEST_GROUP;
        }
    }
    if block_has_cfg_test(file, attr_start, mod_idx) {
        return CFG_TEST_GROUP;
    }
    for idx in attr_start..mod_idx {
        if file
            .stripped
            .get(idx)
            .is_some_and(|line| is_macro_use_attr(&code_of(line)))
        {
            return MACRO_USE_GROUP;
        }
    }
    if file
        .stripped
        .get(mod_idx)
        .is_some_and(|line| prefix_has_macro_use(&code_of(line)))
    {
        return MACRO_USE_GROUP;
    }
    ORDINARY_GROUP
}

/// Terminating line of the file mod at `idx` with remainder `rest`.
fn file_mod_end(lines: &[String], idx: usize, rest: &str) -> usize {
    if rest.contains(';') {
        return idx;
    }
    let mut next = idx.saturating_add(1);
    let limit = idx.saturating_add(3);
    while next <= limit {
        let Some(line) = lines.get(next) else {
            break;
        };
        let code = code_of(line);
        if code.trim().is_empty() || code.trim_start().starts_with('#') {
            next = next.saturating_add(1);
            continue;
        }
        if code.contains(';') {
            return next;
        }
        break;
    }
    idx
}
