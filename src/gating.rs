//! Shared cfg-test gating helpers for rule modules.

use std::{collections::HashSet, hash::BuildHasher};

use crate::{
    lexer::visibility::match_file_mod_decl, rules_simple::is_real_cfg_line, scan::SourceFile,
};

/// Module stem declared within three lines after a cfg-test marker at `i`.
#[must_use]
pub fn gated_stem(lines: &[String], i: usize) -> Option<String> {
    for next in lines.iter().skip(i.saturating_add(1)).take(3) {
        if let Some(stem) = match_file_mod_decl(next) {
            return Some(stem);
        }
        if next.trim().is_empty() || next.trim_start().starts_with('#') {
            continue;
        }
        if next.contains('{') {
            return None;
        }
    }
    None
}

/// Stems gated in one file lines.
fn stems_in_file(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_real_cfg_line(line))
        .filter_map(|(i, _)| gated_stem(lines, i))
        .collect()
}

/// Stems of file modules gated behind a cfg-test marker in any files.
#[must_use]
pub fn gated_stems(files: &[SourceFile]) -> HashSet<String> {
    files
        .iter()
        .flat_map(|file| stems_in_file(&file.lines))
        .collect()
}

/// True if `file` is a file module gated behind a parent `#[cfg(test)]` declaration.
#[must_use]
pub fn is_stem_gated<S>(file: &SourceFile, gated: &HashSet<String, S>) -> bool
where
    S: BuildHasher,
{
    let stem = file.path.rsplit('/').next().unwrap_or(&file.path);
    gated.contains(stem.strip_suffix(".rs").unwrap_or(stem))
}

/// Stems restricted to source-tree files for self-import checks.
#[must_use]
pub fn src_gated_stems(files: &[SourceFile]) -> HashSet<String> {
    files
        .iter()
        .filter(|file| file.path.contains("/src/") || file.path.starts_with("src/"))
        .flat_map(|file| stems_in_file(&file.lines))
        .collect()
}
