//! Capability grouping and global stem uniqueness.
//!
//! Parent index for the capability. Shared path helpers live here while
//! `stem` folds singulars and plurals, `grouping` flags generic directories,
//! and `collision` flags shared file-name words.

pub mod collision;
pub mod grouping;
pub mod stem;

use std::path::Path;

/// Words of one file or directory stem split on separators.
///
/// Splits on `_` and `-`, lowercases, and drops empties so `track-row`
/// and `track_row` map alike.
pub fn split_words(stem: &str) -> Vec<String> {
    stem.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// True if `path` names a Rust source file, case-insensitively.
///
/// Uses `Path` so `FOO.RS` counts alongside `foo.rs`.
#[must_use]
pub fn is_rust_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
}

/// File stem of `path` without its `.rs` suffix, if any.
///
/// Uses only the basename so grouping directories without a parent index
/// contribute no stems.
#[must_use]
pub fn basename_stem(path: &str) -> Option<String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = Path::new(file).file_stem()?.to_str()?;
    (!stem.is_empty()).then(|| stem.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::naming::{collision::duplicate_stems, grouping::generic_dirs};

    #[test]
    fn clean_names_stay_silent() {
        let files = vec![
            "src/playback/engine.rs".to_owned(),
            "src/gallery/detail.rs".to_owned(),
            "tests/verification_flow.rs".to_owned(),
        ];
        assert!(
            duplicate_stems(&files).is_empty(),
            "distinct words stay silent"
        );
        assert!(
            generic_dirs(&files).is_empty(),
            "capability dirs stay silent"
        );
    }

    #[test]
    fn grouping_dirs_add_no_stems() {
        let files = vec![
            "tests/verification/flow.rs".to_owned(),
            "tests/confirmation/flow.rs".to_owned(),
        ];
        assert_eq!(
            duplicate_stems(&files).len(),
            2,
            "basenames collide even though dirs differ"
        );
        let dirs = vec!["tests/verification/flow.rs".to_owned()];
        assert!(generic_dirs(&dirs).is_empty(), "grouping dirs stay silent");
    }

    #[test]
    fn non_leaf_roots_stay_silent() {
        let files = vec![
            "docs/models/idea.rs".to_owned(),
            "fuzz/utils/fuzz.rs".to_owned(),
            "src/models/a.rs".to_owned(),
        ];
        assert_eq!(generic_dirs(&files).len(), 1, "only leaves count");
        assert!(duplicate_stems(&files).is_empty(), "no shared words here");
    }
}
