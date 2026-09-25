//! Grouping checks for collected file modules.
//!
//! Mirrors `mixed_import_groups`: ordered ranks with blank separation and no
//! blanks inside a group.

use crate::{
    rules_tests::{
        mod_groups::{
            attr::{block_has_cfg_test, block_has_macro_use, has_detached_block},
            mod_collect::{ModStmt, collect_file_mods},
        },
        range::code_of,
    },
    scan::SourceFile,
};

/// Module-group findings for one file.
#[must_use]
pub fn check_file(file: &SourceFile) -> Vec<String> {
    let stmts = collect_file_mods(file);
    let mut flagged: Vec<usize> = Vec::new();
    for (pos, stmt) in stmts.iter().enumerate() {
        if has_detached_attr_gap(file, stmt) && !flagged.contains(&pos) {
            flagged.push(pos);
        }
    }
    flag_group_pairs(file, &stmts, &mut flagged);
    flagged.sort_unstable();
    flagged.dedup();
    flagged
        .into_iter()
        .filter_map(|pos| {
            let stmt = stmts.get(pos)?;
            let line = file.lines.get(stmt.mod_idx)?;
            Some(format!(
                "{}:{}:{line}",
                file.path,
                stmt.mod_idx.saturating_add(1)
            ))
        })
        .collect()
}

/// Flag ordered and separated pair violations in `stmts`.
fn flag_group_pairs(file: &SourceFile, stmts: &[ModStmt], flagged: &mut Vec<usize>) {
    let mut prev: Option<&ModStmt> = None;
    let mut peak: u8 = 0;
    for (pos, stmt) in stmts.iter().enumerate() {
        let Some(before) = prev else {
            peak = stmt.group;
            prev = Some(stmt);
            continue;
        };
        if !same_mod_run(file, before.end.saturating_add(1), stmt.attr_start) {
            peak = stmt.group;
            prev = Some(stmt);
            continue;
        }
        if pair_mod_hit(file, before, stmt, peak) && !flagged.contains(&pos) {
            flagged.push(pos);
        }
        peak = peak.max(stmt.group);
        prev = Some(stmt);
    }
}

/// True for lines ignored between two file-module statements.
fn is_mod_gap_filler(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// True when the gap holds at least one blank line.
fn mod_gap_has_blank(file: &SourceFile, from: usize, to: usize) -> bool {
    let mut idx = from;
    while idx < to {
        if file
            .lines
            .get(idx)
            .is_some_and(|line| line.trim().is_empty())
        {
            return true;
        }
        idx = idx.saturating_add(1);
    }
    false
}

/// True when only filler lines sit between two statements.
fn same_mod_run(file: &SourceFile, from: usize, to: usize) -> bool {
    let mut idx = from;
    while idx < to {
        let filler = file
            .stripped
            .get(idx)
            .is_some_and(|line| is_mod_gap_filler(&code_of(line)));
        if !filler {
            return false;
        }
        idx = idx.saturating_add(1);
    }
    true
}

/// True when an adjacent pair in one run needs a finding.
fn pair_mod_hit(file: &SourceFile, before: &ModStmt, stmt: &ModStmt, peak: u8) -> bool {
    let blank = mod_gap_has_blank(file, before.end.saturating_add(1), stmt.attr_start);
    let ordered = stmt.group >= peak;
    let separated = (stmt.group == before.group) != blank;
    !ordered || !separated
}

/// True when a macro-use or cfg-test attribute sits above across a blank.
///
/// Walks whole attribute blocks so multi-line detached attributes count like
/// single-line ones.
fn has_detached_attr_gap(file: &SourceFile, stmt: &ModStmt) -> bool {
    has_detached_block(file, stmt.attr_start, |file, start, end| {
        block_has_cfg_test(file, start, end) || block_has_macro_use(file, start, end)
    })
}
