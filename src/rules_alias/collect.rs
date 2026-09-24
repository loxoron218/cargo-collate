//! `use` statement collection and import expansion.
//!
//! Joins multiline `use` statements, expands brace groups, and records
//! binders alongside aliased entries.

use std::collections::BTreeSet;

use crate::{
    lexer::{
        literal::strip_strings, marker::cut_line_comment, visibility::strip_pub_block,
        word::boundary_after,
    },
    rules_alias::import_alias::{original_name, split_alias, strip_raw},
    rules_self_import::expand::expand,
};

/// True if `code` (string-stripped) opens a `use` statement.
fn is_use_start(code: &str) -> bool {
    let trimmed = code.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    rest.starts_with("use") && boundary_after(rest, 3)
}

/// Collected `use` statements as start line, end line, and joined code.
#[must_use]
pub fn collect_use_stmts(lines: &[String]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (idx, line) in lines.iter().enumerate() {
        let code = cut_line_comment(&strip_strings(line)).to_owned();
        if pending.is_some() {
            extend_pending(&mut pending, &mut out, idx, &code);
            continue;
        }
        if !is_use_start(&code) {
            continue;
        }
        start_pending(&mut pending, &mut out, idx, &code);
    }
    out
}

/// Append `code` to a pending statement, flushing on `;`.
fn extend_pending(
    pending: &mut Option<(usize, String)>,
    out: &mut Vec<(usize, usize, String)>,
    idx: usize,
    code: &str,
) {
    let Some((start, mut buf)) = pending.take() else {
        return;
    };
    buf.push('\n');
    buf.push_str(code);
    if !code.contains(';') {
        *pending = Some((start, buf));
        return;
    }
    out.push((start, idx.saturating_add(1), buf));
}

/// Start a pending statement or push it when complete on one line.
fn start_pending(
    pending: &mut Option<(usize, String)>,
    out: &mut Vec<(usize, usize, String)>,
    idx: usize,
    code: &str,
) {
    let start = idx.saturating_add(1);
    if code.contains(';') {
        out.push((start, start, code.to_owned()));
        return;
    }
    *pending = Some((start, code.to_owned()));
}

/// Body after the `use` keyword up to `;`, if parseable.
#[must_use]
pub fn use_body(stmt: &str) -> Option<String> {
    let trimmed = stmt.trim_start();
    let rest = strip_pub_block(trimmed).unwrap_or(trimmed);
    let after = rest.strip_prefix("use")?;
    if after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let body = after.split(';').next().unwrap_or_default();
    Some(body.trim().to_owned())
}

/// All binders plus aliased entries as original, alias, and statement index.
#[must_use]
pub fn collect_imports(
    stmts: &[(usize, usize, String)],
) -> (BTreeSet<String>, Vec<(String, String, usize)>) {
    let mut binders = BTreeSet::new();
    let mut aliased = Vec::new();
    for (stmt_idx, (_, _, buf)) in stmts.iter().enumerate() {
        collect_stmt_imports(stmt_idx, buf, &mut binders, &mut aliased);
    }
    (binders, aliased)
}

/// Collect imports for one `use` statement body.
fn collect_stmt_imports(
    stmt_idx: usize,
    buf: &str,
    binders: &mut BTreeSet<String>,
    aliased: &mut Vec<(String, String, usize)>,
) {
    let Some(body) = use_body(buf) else {
        return;
    };
    for expanded in expand(&body) {
        collect_one_import(&expanded, stmt_idx, binders, aliased);
    }
}

/// Collect one expanded path into binder or aliased sets.
fn collect_one_import(
    expanded: &str,
    stmt_idx: usize,
    binders: &mut BTreeSet<String>,
    aliased: &mut Vec<(String, String, usize)>,
) {
    let (path_part, alias_opt) = split_alias(expanded);
    let Some(orig) = original_name(&path_part) else {
        return;
    };
    let Some(alias_raw) = alias_opt else {
        binders.extend([orig]);
        return;
    };
    let alias = strip_raw(alias_raw.trim()).trim().to_owned();
    if alias.is_empty() || alias.contains('*') || alias.contains(':') {
        return;
    }
    binders.extend([alias.clone()]);
    aliased.push((orig, alias, stmt_idx));
}
