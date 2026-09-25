//! Import-group separation for `use` blocks.
//!
//! Parent index for the capability. The `origin` submodule resolves package
//! names and local modules while `rank` classifies single imports; `attr`
//! attaches leading attributes and detects `cfg` sub-blocks staying inside
//! their `std`/external/`crate` group like `mixed_mod_groups` attaches `mod`
//! attributes.

pub mod import_attr;
pub mod origin;
pub mod rank;
pub mod stray;

#[cfg(test)]
mod import_cases;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    rules_imports::{
        import_attr::{attached_start, has_detached_attr_gap, is_cfg_gated},
        origin::{locals_for, package_names, top_modules_by_package},
        rank::{classify, is_use_start},
    },
    rules_simple::manifest::package_dir_for,
    scan::SourceFile,
};

/// One joined `use` statement with its group rank.
struct UseStmt {
    /// Zero-based line of the first attached attribute or the `use` itself.
    attr_start: usize,
    /// Zero-based line where the `use` keyword sits.
    start: usize,
    /// Zero-based line where the statement ends.
    end: usize,
    /// Group rank from `std` to `crate`.
    group: u8,
    /// True when gated by `#[cfg(...)]`; sorts after plain in one group.
    cfg_gated: bool,
    /// True when one consolidated block mixes several groups.
    mixed: bool,
}

/// Mixed import groups across every `use` block in all files.
///
/// Groups stay ordered `std`, external, self-package, `crate`; inside one
/// group unconditional imports come first, then `#[cfg(...)]`-gated imports
/// (all predicates lumped). Sub-blocks stay separated by blank lines with no
/// blanks inside one `(group, cfg)` sub-block.
#[must_use]
pub fn mixed_import_groups(files: &[SourceFile]) -> Vec<String> {
    let modules = top_modules_by_package(files);
    let names = package_names(files);
    files
        .iter()
        .flat_map(|file| check_file(file, &modules, &names))
        .collect()
}

/// Findings for one scanned file with per-package lookups.
fn check_file(
    file: &SourceFile,
    modules: &BTreeMap<(String, String), BTreeSet<String>>,
    names: &BTreeMap<String, Option<String>>,
) -> Vec<String> {
    let pkg = package_dir_for(&file.path).to_owned();
    let pkg_name = names.get(&pkg).cloned().flatten();
    let fallback = BTreeSet::new();
    let locals = locals_for(modules, &pkg, &file.path, &fallback);
    group_findings(file, pkg_name.as_deref(), locals)
}

/// Findings for one file with a resolved package name and locals.
fn group_findings(
    file: &SourceFile,
    pkg_name: Option<&str>,
    locals: &BTreeSet<String>,
) -> Vec<String> {
    let stmts = collect_uses(file, pkg_name, locals);
    let mut out = Vec::new();
    for stmt in &stmts {
        if stmt.mixed {
            push_hit(&mut out, &file.path, &file.lines, stmt.start);
        }
        if has_detached_attr_gap(file, stmt.attr_start)
            && !out.iter().any(|hit| {
                hit.starts_with(&format!("{}:{}:", file.path, stmt.start.saturating_add(1)))
            })
        {
            push_hit(&mut out, &file.path, &file.lines, stmt.start);
        }
    }
    let mut prev: Option<&UseStmt> = None;
    let mut peak: (u8, bool) = (0, false);
    for stmt in &stmts {
        let Some(before) = prev else {
            peak = (stmt.group, stmt.cfg_gated);
            prev = Some(stmt);
            continue;
        };
        if !same_run(&file.lines, before.end.saturating_add(1), stmt.attr_start) {
            peak = (stmt.group, stmt.cfg_gated);
            prev = Some(stmt);
            continue;
        }
        if pair_hit(&file.lines, before, stmt, peak) {
            push_hit(&mut out, &file.path, &file.lines, stmt.start);
        }
        peak = peak.max((stmt.group, stmt.cfg_gated));
        prev = Some(stmt);
    }
    out
}

/// True when an adjacent pair in one run needs a finding.
fn pair_hit(lines: &[String], before: &UseStmt, stmt: &UseStmt, peak: (u8, bool)) -> bool {
    let blank = gap_has_blank(lines, before.end.saturating_add(1), stmt.attr_start);
    let ordered = (stmt.group, stmt.cfg_gated) >= peak;
    let separated = ((stmt.group, stmt.cfg_gated) == (before.group, before.cfg_gated)) != blank;
    !ordered || !separated
}

/// Push one `path:line:content` finding for the statement at `start`.
pub fn push_hit(out: &mut Vec<String>, path: &str, lines: &[String], start: usize) {
    if let Some(line) = lines.get(start) {
        out.push(format!("{path}:{}:{line}", start.saturating_add(1)));
    }
}

/// Joined `use` statements of one file with group ranks.
fn collect_uses(
    file: &SourceFile,
    pkg_name: Option<&str>,
    locals: &BTreeSet<String>,
) -> Vec<UseStmt> {
    let lines = &file.lines;
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < lines.len() {
        let Some(line) = lines.get(idx) else {
            break;
        };
        if !is_use_start(cut_line_comment(&strip_strings(line))) {
            idx = idx.saturating_add(1);
            continue;
        }
        let start = idx;
        let mut joined = cut_line_comment(&strip_strings(line)).to_owned();
        let end = stmt_end(lines, start, &mut joined);
        if !joined.contains(';') {
            break;
        }
        let (group, mixed) = classify(&joined, pkg_name, locals);
        let attr_start = attached_start(file, start);
        let cfg_gated = is_cfg_gated(file, attr_start, start);
        out.push(UseStmt {
            attr_start,
            start,
            end,
            group,
            cfg_gated,
            mixed,
        });
        idx = end.saturating_add(1);
    }
    out
}

/// End line of the statement starting at `start`, joining into `joined`.
fn stmt_end(lines: &[String], start: usize, joined: &mut String) -> usize {
    let mut end = start;
    while !joined.contains(';') {
        let Some(next) = lines.get(end.saturating_add(1)) else {
            break;
        };
        end = end.saturating_add(1);
        joined.push('\n');
        joined.push_str(cut_line_comment(&strip_strings(next)));
    }
    end
}

/// True for lines ignored between two `use` statements.
fn is_gap_filler(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// True when the gap holds at least one blank line.
fn gap_has_blank(lines: &[String], from: usize, to: usize) -> bool {
    (from..to).any(|i| lines.get(i).is_some_and(|line| line.trim().is_empty()))
}

/// True when only filler lines sit between two statements.
fn same_run(lines: &[String], from: usize, to: usize) -> bool {
    (from..to).all(|i| {
        lines.get(i).is_some_and(|line| {
            let stripped = strip_strings(line);
            let code = cut_line_comment(&stripped);
            is_gap_filler(code)
        })
    })
}
