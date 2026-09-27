//! First-occurrence import alias detection.
//!
//! Flags `use … as …` when the same original name is imported again later in
//! the same scope; the first import stays plain and subsequent colliding
//! imports carry the alias. Top-level imports group file-wide while imports
//! inside blocks group per statement, approximating scopes without parsing
//! them. Configuration-gated imports, `thiserror::Error` (owned by its own
//! lint), and names colliding with file-local items stay skipped.

#[cfg(test)]
mod ordering;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    rules_alias::{
        alias_line,
        collect::{collect_use_stmts, use_body},
        format_hits,
        import_alias::{is_prelude, original_name, split_alias, strip_raw},
        local_scope::collect_locals,
        thiserror::is_thiserror_error,
    },
    rules_self_import::expand::expand,
    scan::SourceFile,
};

/// One expanded import in source order.
struct Entry {
    /// Original imported name.
    orig: String,
    /// Alias when the import uses `as`, else none.
    alias: Option<String>,
    /// One-based first line of the enclosing `use` statement.
    start: usize,
    /// One-based last line of the enclosing `use` statement.
    end: usize,
    /// Indent width of the statement start line.
    depth: usize,
    /// Index of the enclosing `use` statement in source order.
    stmt: usize,
}

/// Alias slot claim for first-occurrence ordering.
enum Slot {
    /// Anonymous or malformed import claiming no slot.
    Skip,
    /// Plain or same-name import claiming the first slot.
    Plain,
    /// Aliased import carrying this alias.
    Aliased(String),
}

/// Import aliases on the first occurrence of a name in the same scope.
///
/// Reports one finding for the first aliased import when the same original
/// name appears again later in its scope, including grouped and `pub use`
/// forms.
pub fn first_occurrence_alias(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_scope_aliases).collect()
}

/// Findings for one file.
fn check_scope_aliases(file: &SourceFile) -> Vec<String> {
    let locals = collect_locals(&file.stripped);
    let stmts = collect_use_stmts(&file.lines);
    mark_flagged(file, &collect_ordered(&file.lines, &stmts), &locals)
}

/// Expanded imports in source order for every unconditional `use` statement.
///
/// Configuration-gated statements stay skipped since their order spans
/// different compilation worlds.
fn collect_ordered(lines: &[String], stmts: &[(usize, usize, String)]) -> Vec<Entry> {
    let mut ordered = Vec::new();
    for (stmt, (start, end, buf)) in stmts.iter().enumerate() {
        collect_stmt_entries(lines, stmt, *start, *end, buf, &mut ordered);
    }
    ordered
}

/// Push expanded imports for one `use` statement body.
///
/// Nested statements record their indent depth so collisions only apply
/// within the same scope approximation.
fn collect_stmt_entries(
    lines: &[String],
    stmt: usize,
    start: usize,
    end: usize,
    buf: &str,
    ordered: &mut Vec<Entry>,
) {
    if !is_ungated(lines, start) {
        return;
    }
    let Some(body) = use_body(buf) else {
        return;
    };
    let depth = indent_depth(lines, start);
    for expanded in expand(&body) {
        push_entry(&expanded, start, end, depth, stmt, ordered);
    }
}

/// Indent width in bytes of the statement start line.
///
/// Top-level imports sit at column zero while imports inside blocks share
/// their scope only with neighbors at the same depth.
fn indent_depth(lines: &[String], start: usize) -> usize {
    lines
        .get(start.saturating_sub(1))
        .map_or(0, |line| line.len().saturating_sub(line.trim_start().len()))
}

/// True if no attribute directly precedes the statement.
///
/// Configuration-gated imports live in different compilation worlds, so
/// source order across gates says nothing about collisions.
fn is_ungated(lines: &[String], start: usize) -> bool {
    if start <= 1 {
        return true;
    }
    lines
        .get(start.saturating_sub(2))
        .is_none_or(|line| !line.trim_start().starts_with("#["))
}

/// Push one expanded path, skipping prelude origins and anonymous imports.
///
/// `thiserror::Error` stays skipped since its collisions belong to the
/// dedicated lint, which pins it plain while this lint orders the rest.
fn push_entry(
    expanded: &str,
    start: usize,
    end: usize,
    depth: usize,
    stmt: usize,
    ordered: &mut Vec<Entry>,
) {
    let (path_part, alias_opt) = split_alias(expanded);
    if is_thiserror_error(&path_part) {
        return;
    }
    let Some(orig) = original_name(&path_part) else {
        return;
    };
    if is_prelude(&orig) {
        return;
    }
    let Some(alias_raw) = alias_opt else {
        ordered.push(Entry {
            orig,
            alias: None,
            start,
            end,
            depth,
            stmt,
        });
        return;
    };
    match clean_alias(&alias_raw, &orig) {
        Slot::Skip => {}
        Slot::Plain => ordered.push(Entry {
            orig,
            alias: None,
            start,
            end,
            depth,
            stmt,
        }),
        Slot::Aliased(alias) => ordered.push(Entry {
            orig,
            alias: Some(alias),
            start,
            end,
            depth,
            stmt,
        }),
    }
}

/// Slot claim for one alias, distinguishing anonymous imports.
///
/// Same-name aliases count as plain claims so only the other lint flags
/// them, while `as _` binds nothing and stays skipped.
fn clean_alias(raw: &str, orig: &str) -> Slot {
    let name = strip_raw(raw.trim()).trim().to_owned();
    if name == "_" || name.is_empty() || name.contains('*') || name.contains(':') {
        return Slot::Skip;
    }
    if name == orig {
        return Slot::Plain;
    }
    Slot::Aliased(name)
}

/// Flagged alias lines where the first import of a name is aliased.
///
/// Top-level entries group file-wide by name while nested entries group by
/// statement, approximating scopes without parsing blocks. Groups whose name
/// collides with a file-local item stay skipped since their alias is
/// required.
fn mark_flagged(file: &SourceFile, ordered: &[Entry], locals: &BTreeSet<String>) -> Vec<String> {
    let mut grouped: BTreeMap<(&str, Option<usize>), Vec<&Entry>> = BTreeMap::new();
    for entry in ordered {
        grouped
            .entry((entry.orig.as_str(), entry_scope(entry)))
            .or_default()
            .push(entry);
    }
    let mut flagged = BTreeSet::new();
    for entries in grouped.values() {
        if entries.len() < 2 {
            continue;
        }
        let Some(first) = entries.first().copied() else {
            continue;
        };
        if locals.contains(first.orig.as_str()) {
            continue;
        }
        let Some(alias) = first.alias.as_ref() else {
            continue;
        };
        flagged.extend([alias_line(&file.lines, first.start, first.end, alias)]);
    }
    format_hits(file, flagged)
}

/// Statement scope for grouping: top-level entries share one scope per name
/// while nested entries scope to their own statement.
const fn entry_scope(entry: &Entry) -> Option<usize> {
    if entry.depth == 0 {
        None
    } else {
        Some(entry.stmt)
    }
}
