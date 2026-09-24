//! Module-depth checks for shallow nesting.
//!
//! `CODING_STANDARDS.md` caps the sub-folder depth below a scan leaf at two
//! (such as `src/ui/gallery/`), so deeper trees stay flagged. Paths outside
//! the classic leaves measure from their first folder instead, so every
//! scanned `.rs` file counts, including `docs/`, `fuzz/`, `examples/`, and
//! other non-standard roots.

/// Scan leaves measured from; the outermost match wins.
///
/// Complete Cargo auto-discovered target directories
/// (`doc.rust-lang.org/cargo/guide/project-layout.html`): `src`
/// (lib/bins), `tests` (integration tests), `benches` (benchmarks).
/// All other roots (`examples/`, `fuzz/`, `docs/`, `xtask/`, …) fall back to
/// first-folder measurement via [`below_first_folder`], so no leaf is missing.
const SCAN_LEAVES: [&str; 3] = ["src", "tests", "benches"];

/// Maximum sub-folder depth below a scan leaf, per `CODING_STANDARDS.md`.
pub const MAX_FOLDER_DEPTH: usize = 2;

/// Remainder of `path` below its outermost scan leaf, if any.
///
/// Matches both leaf-relative paths (`src/a.rs`) and package-prefixed ones
/// (`member/src/a.rs`, `tools/name/src/a.rs`). When several leaves match, the
/// leftmost marker wins, so a nested crate root such as
/// `tests/w/dep/src/a.rs` measures from `tests/`, not from the inner `src/`.
/// The leftmost marker leaves the longest remainder, which keeps the
/// comparison slicing-free.
#[must_use]
pub fn below_leaf(path: &str) -> Option<&str> {
    for leaf in SCAN_LEAVES {
        let mut head = String::new();
        head.push_str(leaf);
        head.push('/');
        if let Some(rest) = path.strip_prefix(head.as_str()) {
            return Some(rest);
        }
    }
    let mut best: Option<&str> = None;
    for leaf in SCAN_LEAVES {
        let mut marker = String::new();
        marker.push('/');
        marker.push_str(leaf);
        marker.push('/');
        if let Some((_, rest)) = path.split_once(marker.as_str())
            && best.is_none_or(|prev| rest.len() > prev.len())
        {
            best = Some(rest);
        }
    }
    best
}

/// Remainder of `path` below its first folder.
///
/// Fallback for paths outside every scan leaf, so arbitrary roots (`docs/`,
/// `fuzz/`, …) still measure instead of staying silent. Files directly in
/// the first folder map to their file name.
fn below_first_folder(path: &str) -> &str {
    if let Some((_, rel)) = path.split_once('/') {
        rel
    } else {
        path
    }
}

/// Depth of the file parent dir below its scan leaf, if under one.
///
/// A file directly under the leaf sits at depth zero, so
/// `src/ui/gallery/item.rs` measures two.
#[must_use]
pub fn depth_below_leaf(path: &str) -> Option<usize> {
    if path.is_empty() {
        return None;
    }
    let rest = below_leaf(path).unwrap_or_else(|| below_first_folder(path));
    let Some((parent, _)) = rest.rsplit_once('/') else {
        return Some(0);
    };
    if parent.is_empty() {
        return Some(0);
    }
    Some(parent.split('/').count())
}

/// Files nested deeper than the limit, one hit per file, sorted.
///
/// Every scanned `.rs` file counts: classic leaves measure from their
/// outermost leaf while other roots measure from their first folder.
#[must_use]
pub fn deep_nesting(rs_files: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = rs_files
        .iter()
        .filter_map(|path| {
            let depth = depth_below_leaf(path)?;
            (depth > MAX_FOLDER_DEPTH)
                .then(|| format!("{path}: depth {depth} (max {MAX_FOLDER_DEPTH})"))
        })
        .collect();
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::layout::{MAX_FOLDER_DEPTH, deep_nesting, depth_below_leaf};

    #[test]
    fn depth_limit_matches_standard() {
        assert_eq!(MAX_FOLDER_DEPTH, 2);
    }

    #[test]
    fn shallow_paths_stay_silent() {
        assert_eq!(depth_below_leaf("src/app.rs"), Some(0));
        assert_eq!(depth_below_leaf("src/ui/item.rs"), Some(1));
        assert_eq!(depth_below_leaf("src/ui/gallery/item.rs"), Some(2));
        assert_eq!(depth_below_leaf("tests/fixture.rs"), Some(0));
        let files = vec!["src/app.rs".to_owned(), "src/ui/gallery/item.rs".to_owned()];
        assert!(deep_nesting(&files).is_empty(), "shallow files stay silent");
    }

    #[test]
    fn deep_paths_flagged() {
        assert_eq!(depth_below_leaf("src/ui/gallery/detail/item.rs"), Some(3));
        let files = vec![
            "src/ui/gallery/detail/item.rs".to_owned(),
            "src/app.rs".to_owned(),
            "src/ui/gallery/detail/item.rs".to_owned(),
        ];
        assert_eq!(
            deep_nesting(&files),
            vec!["src/ui/gallery/detail/item.rs: depth 3 (max 2)".to_owned()]
        );
    }

    #[test]
    fn member_and_tool_prefixes_resolve() {
        assert_eq!(depth_below_leaf("member/src/a/b/item.rs"), Some(2));
        assert_eq!(depth_below_leaf("member/src/a/b/c/item.rs"), Some(3));
        assert_eq!(
            depth_below_leaf("collate/src/rules_simple/layout.rs"),
            Some(1)
        );
    }

    #[test]
    fn nested_crate_roots_measure_from_outer_leaf() {
        assert_eq!(depth_below_leaf("tests/w/dep/src/a/b/c/item.rs"), Some(6));
        assert_eq!(
            depth_below_leaf(
                "tests/workspace_test/module_style/pass/dep/src/with_mod/inner/stuff/most.rs"
            ),
            Some(8)
        );
        assert_eq!(depth_below_leaf("tests/src/foo.rs"), Some(1));
        let files = vec!["tests/w/dep/src/a/b/c/item.rs".to_owned()];
        assert_eq!(
            deep_nesting(&files),
            vec!["tests/w/dep/src/a/b/c/item.rs: depth 6 (max 2)".to_owned()]
        );
    }

    #[test]
    fn all_leaves_share_limit() {
        let files = vec![
            "benches/a/b/c/item.rs".to_owned(),
            "tests/a/b/c/item.rs".to_owned(),
            "fuzz/a/b/c/item.rs".to_owned(),
        ];
        assert_eq!(deep_nesting(&files).len(), 3);
    }

    #[test]
    fn arbitrary_roots_flagged() {
        assert_eq!(depth_below_leaf("docs/a/b/c/item.rs"), Some(3));
        assert_eq!(depth_below_leaf("fuzz/fuzz_targets/parse.rs"), Some(1));
        assert_eq!(depth_below_leaf("examples/a/b/c/item.rs"), Some(3));
        let files = vec!["docs/a/b/c/item.rs".to_owned()];
        assert_eq!(
            deep_nesting(&files),
            vec!["docs/a/b/c/item.rs: depth 3 (max 2)".to_owned()]
        );
    }
}
