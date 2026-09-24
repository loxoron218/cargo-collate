//! Cargo manifest helpers for parent-index checks.

use std::{fs::read_to_string, path::Path};

/// Match `path` value at `i`; returns the value and resume index.
fn path_value_at(text: &str, chars: &[char], i: usize) -> Option<(String, usize)> {
    if text.get(i..).is_none_or(|rest| !rest.starts_with("path"))
        || !boundary_before(chars, i)
        || !boundary_after(text, i.saturating_add(4))
    {
        return None;
    }
    let mut j = skip_spaces(chars, i.saturating_add(4));
    if chars.get(j) != Some(&'=') {
        return None;
    }
    j = skip_spaces(chars, j.saturating_add(1));
    if chars.get(j) != Some(&'"') {
        return None;
    }
    let start = j.saturating_add(1);
    let mut end = start;
    while chars.get(end).is_some_and(|c| *c != '"') {
        end = end.saturating_add(1);
    }
    let value: String = chars
        .get(start..end)
        .map_or_else(String::new, |part| part.iter().collect());
    (end < chars.len() && Path::new(&value).extension().is_some_and(|ext| ext == "rs"))
        .then(|| (value, end.saturating_add(1)))
}

/// True if the char before `i` ends any identifier.
fn boundary_before(chars: &[char], i: usize) -> bool {
    i == 0
        || chars
            .get(i.saturating_sub(1))
            .is_some_and(|p| !p.is_ascii_alphanumeric() && *p != '_')
}

/// True if `text` ends or continues with a non-identifier char.
fn boundary_after(text: &str, len: usize) -> bool {
    text.len() == len
        || text.get(len..).is_some_and(|tail| {
            tail.chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
        })
}

/// Skip ASCII spaces and tabs from `i`.
fn skip_spaces(chars: &[char], mut j: usize) -> usize {
    while chars.get(j).is_some_and(|c| *c == ' ' || *c == '\t') {
        j = j.saturating_add(1);
    }
    j
}

/// Extract manifest path values from `Cargo.toml` text.
fn cargo_path_values(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((value, next)) = path_value_at(text, &chars, i) {
            out.push(value);
            i = next;
        } else {
            i = i.saturating_add(1);
        }
    }
    out
}

/// Parent dir of `path`, or empty when at the top level.
///
/// `src/a.rs` maps to `src`, `a.rs` maps to empty, and `a/b/c.rs` maps to
/// `a/b`.
fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// Nearest non-empty ancestor of `path` containing a `Cargo.toml`, if any.
///
/// Skips the repo root itself so syntactic fallbacks stay deterministic in
/// tests; arbitrary layouts (`fuzz/`, `xtask/`, workspace-excluded crates)
/// resolve here when their manifests exist on disk.
fn manifest_ancestor(path: &str) -> Option<&str> {
    let mut dir = parent_of(path);
    while !dir.is_empty() {
        let manifest = format!("{dir}/Cargo.toml");
        if Path::new(&manifest).exists() {
            return Some(dir);
        }
        dir = parent_of(dir);
    }
    None
}

/// Syntactic package dir via legacy leaf markers, if any.
///
/// Keeps fake test paths and manifest-less trees deterministic without
/// filesystem access.
fn marker_package(path: &str) -> Option<&str> {
    for marker in ["/src/", "/tests/", "/benches/", "/examples/"] {
        if let Some((prefix, _)) = path.split_once(marker) {
            return Some(prefix);
        }
    }
    None
}

/// Package dir holding the manifest for one source path.
///
/// Prefers the nearest ancestor `Cargo.toml` so arbitrary layouts resolve
/// without a hardcoded leaf list; falls back to legacy leaf markers for
/// manifest-less trees, else empty denoting the repo-root package.
#[must_use]
pub fn package_dir_for(path: &str) -> &str {
    if let Some(dir) = manifest_ancestor(path) {
        return dir;
    }
    marker_package(path).unwrap_or("")
}

/// Manifest paths covering root plus each package dir in `rs_files`.
#[must_use]
pub fn manifest_paths_for(rs_files: &[String]) -> Vec<String> {
    let mut dirs: Vec<&str> = vec![""];
    for path in rs_files {
        let dir = package_dir_for(path);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs.into_iter()
        .map(|dir| {
            if dir.is_empty() {
                "Cargo.toml".to_owned()
            } else {
                format!("{dir}/Cargo.toml")
            }
        })
        .collect()
}

/// Manifest roots prefixed with their package dir.
#[must_use]
pub fn prefixed_manifest_roots(manifest: &str, text: &str) -> Vec<String> {
    let dir = manifest
        .strip_suffix("/Cargo.toml")
        .unwrap_or_default()
        .trim_end_matches('/');
    cargo_path_values(text)
        .into_iter()
        .map(|value| {
            if dir.is_empty() {
                value
            } else {
                format!("{dir}/{value}")
            }
        })
        .collect()
}

/// Cargo target roots across all covering manifests.
fn all_cargo_roots(rs_files: &[String]) -> Vec<String> {
    let mut roots = Vec::new();
    for manifest in manifest_paths_for(rs_files) {
        if let Ok(text) = read_to_string(&manifest) {
            roots.extend(prefixed_manifest_roots(&manifest, &text));
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

/// Parent-index violations for stray nested indexes.
#[must_use]
pub fn stray_mod_rs(rs_files: &[String]) -> Vec<String> {
    let mut found: Vec<&String> = rs_files.iter().filter(|f| f.ends_with("/mod.rs")).collect();
    found.sort();
    let roots = all_cargo_roots(rs_files);
    found
        .into_iter()
        .filter(|f| roots.binary_search(f).is_err())
        .cloned()
        .collect()
}
