//! Duplicate module-name detection across the workspace.
//!
//! Flags modules sharing any normalized word stem so every word stem stays
//! unique codebase-wide across `src/`, `tests/`, and `benches/`.

use std::collections::{BTreeMap, BTreeSet};

use crate::rules_simple::{
    layout::below_leaf,
    naming::{basename_stem, is_rust_file, split_words, stem::normalize_stem},
};

/// Normalized word stems of one path basename.
///
/// Lowercases, splits on `_` and `-`, and folds each word's plural, so
/// `track-row` and `track_rows` share word stems. Repeated stems collapse
/// since the set holds one entry per stem.
fn file_words(path: &str) -> BTreeSet<String> {
    let Some(stem) = basename_stem(path) else {
        return BTreeSet::new();
    };
    split_words(&stem)
        .iter()
        .map(|word| normalize_stem(word))
        .collect()
}

/// Single collision hit for `path`, if any word stem is shared.
///
/// Collects every other holder across all of `words`, then names the
/// smallest other path for determinism. Reports the smallest word stem
/// shared with that peer so output stays stable. Returns `None` when
/// silent.
fn collision_for(
    path: &str,
    words: &BTreeSet<String>,
    path_words: &BTreeMap<String, BTreeSet<String>>,
    by_word: &BTreeMap<String, BTreeSet<String>>,
) -> Option<String> {
    let mut peers: BTreeSet<&String> = BTreeSet::new();
    for word in words {
        let Some(holders) = by_word.get(word) else {
            continue;
        };
        if holders.len() <= 1 {
            continue;
        }
        peers.extend(holders.iter().filter(|holder| holder.as_str() != path));
    }
    let best_peer = peers.into_iter().next()?;
    let peer_words = path_words.get(best_peer)?;
    let best_word = words.intersection(peer_words).next()?;
    Some(format!(
        "{path}: stem '{best_word}' collides with '{best_peer}'"
    ))
}

/// Files sharing any normalized word stem, one hit per file, sorted.
///
/// Global workspace-wide map across all `src/`, `tests/`, and `benches/`
/// files found by `rg`; directories contribute nothing and no basenames are
/// exempt, so two `main.rs` files collide. A compound name such as
/// `queue_playback.rs` colliding via `queue` and `playback` reports only its
/// smallest peer path.
#[must_use]
pub fn duplicate_stems(rs_files: &[String]) -> Vec<String> {
    let mut unique: BTreeSet<&String> = BTreeSet::new();
    for path in rs_files {
        unique.extend([path]);
    }
    let mut path_words: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut by_word: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in unique {
        if below_leaf(path).is_none() {
            continue;
        }
        if !is_rust_file(path) {
            continue;
        }
        let words = file_words(path);
        if words.is_empty() {
            continue;
        }
        for word in &words {
            by_word
                .entry(word.clone())
                .or_default()
                .extend([path.clone()]);
        }
        path_words.entry(path.clone()).or_default().extend(words);
    }
    let mut hits = Vec::new();
    for (path, words) in &path_words {
        if let Some(hit) = collision_for(path, words, &path_words, &by_word) {
            hits.push(hit);
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::naming::collision::duplicate_stems;

    #[test]
    fn distinct_words_stay_silent() {
        let files = vec![
            "src/playback/engine.rs".to_owned(),
            "src/gallery/detail.rs".to_owned(),
            "src/verification/flow.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "distinct words stay silent"
        );
    }

    #[test]
    fn shared_words_collide() {
        let files = vec![
            "src/player/track_row.rs".to_owned(),
            "src/editor/track_transition.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "shared words collide");
        let files = vec![
            "src/player/queue_persistence.rs".to_owned(),
            "src/editor/settings_persistence.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "shared words collide");
        let files = vec![
            "src/playback/queue.rs".to_owned(),
            "src/playback/queue_manager.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "prefix shares collide");
        let files = vec![
            "src/rules_alias.rs".to_owned(),
            "src/rules_docs.rs".to_owned(),
        ];
        assert_eq!(duplicate_stems(&files).len(), 2, "family words collide");
    }

    #[test]
    fn compound_file_reports_single_collision() {
        let files = vec![
            "src/queue.rs".to_owned(),
            "src/playback.rs".to_owned(),
            "src/queue_playback.rs".to_owned(),
        ];
        let hits = duplicate_stems(&files);
        assert_eq!(hits.len(), 3, "compound file caps at one hit");
        let compound: Vec<&String> = hits
            .iter()
            .filter(|hit| hit.starts_with("src/queue_playback.rs:"))
            .collect();
        assert_eq!(compound.len(), 1, "compound file reports once");
        assert!(
            compound
                .first()
                .is_some_and(|hit| hit.contains("src/playback.rs")),
            "smallest peer wins"
        );
    }

    #[test]
    fn repeated_words_stay_single() {
        let files = vec!["src/foo_foo.rs".to_owned(), "src/foo.rs".to_owned()];
        assert_eq!(duplicate_stems(&files).len(), 2, "repeats stay single");
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
