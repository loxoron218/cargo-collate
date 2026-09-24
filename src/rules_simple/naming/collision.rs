//! Duplicate module-name detection across the workspace.
//!
//! Flags modules sharing a full normalized file stem so every module name
//! stays unique codebase-wide across `src/`, `tests/`, and `benches/`.

use std::collections::{BTreeMap, BTreeSet};

use crate::rules_simple::{
    layout::below_leaf,
    naming::{basename_stem, is_rust_file, split_words, stem::normalize_stem},
};

/// Normalized full-stem key of one file basename, if any.
///
/// Lowercases, splits on `_` and `-`, folds each word's plural, and rejoins
/// with `_`, so `track-row` and `track_row` share a key while `track_row`
/// and `track_transition` do not.
fn module_key(path: &str) -> Option<String> {
    let stem = basename_stem(path)?;
    let mut words = Vec::new();
    for word in split_words(&stem) {
        words.push(normalize_stem(&word));
    }
    (!words.is_empty()).then(|| words.join("_"))
}

/// Smallest other path in `paths` distinct from `path`, if any.
///
/// Keeps output deterministic with one culprit per hit.
fn first_other(paths: &BTreeSet<String>, path: &str) -> Option<String> {
    paths.iter().find(|other| other.as_str() != path).cloned()
}

/// Push hits for one stem shared by several paths.
///
/// One hit per file names the smallest other holder for determinism.
fn collisions_for(stem: &str, paths: &BTreeSet<String>, hits: &mut Vec<String>) {
    if paths.len() <= 1 {
        return;
    }
    for path in paths {
        if let Some(other) = first_other(paths, path) {
            hits.push(format!("{path}: stem '{stem}' collides with '{other}'"));
        }
    }
}

/// Files sharing a full normalized file-name stem, one hit per file, sorted.
///
/// Global workspace-wide map across all `src/`, `tests/`, and `benches/`
/// files found by `rg`; directories contribute nothing and no basenames are
/// exempt, so two `main.rs` files collide.
#[must_use]
pub fn duplicate_stems(rs_files: &[String]) -> Vec<String> {
    let mut unique: BTreeSet<&String> = BTreeSet::new();
    for path in rs_files {
        unique.extend([path]);
    }
    let mut by_stem: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in unique {
        if below_leaf(path).is_none() {
            continue;
        }
        if !is_rust_file(path) {
            continue;
        }
        if let Some(key) = module_key(path) {
            by_stem.entry(key).or_default().extend([path.clone()]);
        }
    }
    let mut hits = Vec::new();
    for (stem, paths) in &by_stem {
        collisions_for(stem, paths, &mut hits);
    }
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::naming::collision::duplicate_stems;

    #[test]
    fn distinct_names_stay_silent() {
        let files = vec![
            "src/player/track_row.rs".to_owned(),
            "src/editor/track_transition.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "distinct stems stay silent"
        );
        let files = vec![
            "src/player/queue_persistence.rs".to_owned(),
            "src/editor/settings_persistence.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "distinct stems stay silent"
        );
        let files = vec![
            "src/playback/queue.rs".to_owned(),
            "src/playback/queue_manager.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "prefix shares stay silent"
        );
        let files = vec![
            "src/rules_alias.rs".to_owned(),
            "src/rules_docs.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "distinct family stems stay silent"
        );
    }

    #[test]
    fn same_names_collide() {
        let files = vec![
            "src/playback/queue.rs".to_owned(),
            "src/ui/player/queue.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "same stems collide");
        let files = vec![
            "src/importer/fetch.rs".to_owned(),
            "src/exporter/fetch.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "same stems collide");
        let files = vec![
            "src/importer/field.rs".to_owned(),
            "src/exporter/field.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "same stems collide");
    }

    #[test]
    fn separators_and_case_fold() {
        let files = vec![
            "src/player/track_row.rs".to_owned(),
            "src/editor/track-row.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "separators fold");
        let files = vec![
            "src/gallery/Album.rs".to_owned(),
            "src/editor/album.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "case folds");
    }

    #[test]
    fn plural_forms_collide() {
        let files = vec![
            "src/gallery/album.rs".to_owned(),
            "src/editor/albums.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "singular plural collide");
        let files = vec![
            "src/docs/guide.rs".to_owned(),
            "src/types/guides.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "singular plural collide");
    }

    #[test]
    fn cross_leaf_names_collide() {
        let files = vec![
            "src/sample/conv.rs".to_owned(),
            "tests/sample/conv.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "leaves share scope");
    }

    #[test]
    fn duplicate_roots_collide_without_exemptions() {
        let files = vec![
            "member-a/src/main.rs".to_owned(),
            "member-b/src/main.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "roots collide");
    }
}
