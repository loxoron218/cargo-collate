//! Package names and local modules per package dir.
//!
//! Reads each covering manifest once for the own-crate name and derives
//! top-level `src` modules from the scanned file list.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::read_to_string,
};

use crate::{rules_simple::manifest::package_dir_for, scan::SourceFile};

/// Top-level module names per package dir and source root.
#[must_use]
pub fn top_modules_by_package(
    files: &[SourceFile],
) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut out: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let pkg = package_dir_for(&file.path).to_owned();
        if let Some((root, rel)) = root_relative(&pkg, &file.path)
            && let Some(top) = top_segment(&rel)
        {
            record_top(&mut out, (pkg, root), top);
        }
    }
    out
}

/// Record one top module in its package-root set.
fn record_top(
    out: &mut BTreeMap<(String, String), BTreeSet<String>>,
    key: (String, String),
    top: String,
) {
    let entry = out.entry(key).or_default();
    if !entry.contains(&top) {
        entry.extend([top]);
    }
}

/// Local modules for one file path: its own source root, if any.
#[must_use]
pub fn locals_for<'a>(
    modules: &'a BTreeMap<(String, String), BTreeSet<String>>,
    pkg: &str,
    path: &str,
    fallback: &'a BTreeSet<String>,
) -> &'a BTreeSet<String> {
    let Some((root, _)) = root_relative(pkg, path) else {
        return fallback;
    };
    modules.get(&(pkg.to_owned(), root)).unwrap_or(fallback)
}

/// Source root and relative path for one file path, if scanned.
///
/// The first component under the package dir is the root (`src`, `tests`,
/// `fuzz`, `xtask`, …), so arbitrary layouts resolve without a hardcoded
/// list. Files directly in the package dir map to an empty root.
fn root_relative(pkg: &str, path: &str) -> Option<(String, String)> {
    let rest = if pkg.is_empty() {
        path
    } else {
        path.strip_prefix(pkg)?.strip_prefix('/')?
    };
    if let Some((root, rel)) = rest.split_once('/') {
        Some((root.to_owned(), rel.to_owned()))
    } else {
        Some((String::new(), rest.to_owned()))
    }
}

/// Top module name for a source-relative path, if it declares one.
fn top_segment(rel: &str) -> Option<String> {
    let head = rel.split('/').next().unwrap_or("");
    if head.contains('.') {
        let stem = head.strip_suffix(".rs").unwrap_or(head);
        if stem == "main" || stem == "lib" || stem == "mod" {
            return None;
        }
        return (!stem.is_empty()).then_some(stem.to_owned());
    }
    (!head.is_empty()).then_some(head.to_owned())
}

/// Own crate names with hyphens mapped per package dir in `files`.
#[must_use]
pub fn package_names(files: &[SourceFile]) -> BTreeMap<String, Option<String>> {
    let mut pkgs: Vec<String> = files
        .iter()
        .map(|file| package_dir_for(&file.path).to_owned())
        .collect();
    pkgs.sort();
    pkgs.dedup();
    pkgs.into_iter()
        .map(|pkg| {
            let name = package_name(&pkg);
            (pkg, name)
        })
        .collect()
}

/// Own crate name with hyphens mapped for a package dir.
fn package_name(pkg: &str) -> Option<String> {
    let manifest = if pkg.is_empty() {
        "Cargo.toml".to_owned()
    } else {
        format!("{pkg}/Cargo.toml")
    };
    let Ok(text) = read_to_string(manifest) else {
        return None;
    };
    name_in_manifest(&text)
}

/// Crate name from manifest text with hyphens mapped to underscores.
fn name_in_manifest(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(name) = name_value(trimmed) {
            return Some(name);
        }
    }
    None
}

/// Value of a `name = "..."` line with hyphens mapped, if any.
fn name_value(trimmed: &str) -> Option<String> {
    let (key, rest) = trimmed.split_once('=')?;
    if key.trim() != "name" {
        return None;
    }
    let raw = rest.split('#').next().unwrap_or("").trim();
    let inner = unquoted(raw, '"').or_else(|| unquoted(raw, '\''))?;
    (!inner.is_empty()).then_some(inner.replace('-', "_"))
}

/// Contents of a quoted manifest value for `quote`, if well-formed.
fn unquoted(raw: &str, quote: char) -> Option<&str> {
    let tail = raw.strip_prefix(quote)?;
    tail.strip_suffix(quote)
}

#[cfg(test)]
mod tests {
    use crate::rules_imports::origin::{name_in_manifest, root_relative, top_segment};

    #[test]
    fn manifest_names_parsed() {
        assert_eq!(
            name_in_manifest("[package]\nname = \"my-crate\"\n"),
            Some("my_crate".to_owned()),
            "hyphens map"
        );
        assert_eq!(
            name_in_manifest("[package]\nname = \"plain\"\n"),
            Some("plain".to_owned()),
            "plain names pass"
        );
        assert_eq!(
            name_in_manifest("[dependencies]\nname = \"other\"\n"),
            None,
            "wrong section stays silent"
        );
        assert_eq!(name_in_manifest("not toml"), None, "garbage stays silent");
    }

    #[test]
    fn segments_resolved() {
        assert_eq!(
            top_segment("gating.rs"),
            Some("gating".to_owned()),
            "file stems map"
        );
        assert_eq!(
            top_segment("lexer/word.rs"),
            Some("lexer".to_owned()),
            "dirs map to top"
        );
        assert_eq!(top_segment("main.rs"), None, "roots declare nothing");
        assert_eq!(
            root_relative("", "tests/api.rs"),
            Some(("tests".to_owned(), "api.rs".to_owned())),
            "integration roots map"
        );
        assert_eq!(
            root_relative("", "src/a.rs"),
            Some(("src".to_owned(), "a.rs".to_owned())),
            "src paths map"
        );
        assert_eq!(
            root_relative("", "fuzz/fuzz_targets/parse.rs"),
            Some(("fuzz".to_owned(), "fuzz_targets/parse.rs".to_owned())),
            "arbitrary roots map"
        );
        assert_eq!(
            root_relative("fuzz", "fuzz/fuzz_targets/parse.rs"),
            Some(("fuzz_targets".to_owned(), "parse.rs".to_owned())),
            "package roots map"
        );
    }
}
