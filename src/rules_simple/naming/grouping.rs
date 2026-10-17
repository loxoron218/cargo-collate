//! Generic name detection for capability grouping.
//!
//! Flags generic directory components and file stems so modules group by
//! capability/domain instead.

use crate::rules_simple::{
    layout::below_leaf,
    naming::{basename_stem, split_words, stem::normalize_stem},
};

/// Normalized generic name words, already singular.
///
/// Extensible: add the normalized singular here when a new generic name
/// appears. Covers layer/role names (`model`, `handler`, `type`), shared
/// scaffolding (`common`, `shared`, `helper`, `util`, `utility`, `support`,
/// `base`, `core`, `generic`, `global`, `misc`, `script`, `tool`, `example`,
/// `doc`, `spec`) and test scaffolding (`test`, `testing`, `bench`,
/// `benchmark`, `mock`, `fixture`, `stub`, `fake`, `double`).
const FORBIDDEN_NORMS: [&str; 28] = [
    "base",
    "bench",
    "benchmark",
    "common",
    "core",
    "doc",
    "double",
    "example",
    "fake",
    "fixture",
    "generic",
    "global",
    "handler",
    "helper",
    "misc",
    "mock",
    "model",
    "script",
    "shared",
    "spec",
    "stub",
    "support",
    "test",
    "testing",
    "tool",
    "type",
    "util",
    "utility",
];

/// First generic word in one directory or file name, if any.
///
/// Splits on `_` and `-` and folds singulars/plurals per word so `models`,
/// `service_types`, and `TYPES` all hit while `rules_simple` stays silent.
fn generic_word_in(name: &str) -> Option<String> {
    split_words(name).into_iter().find_map(|word| {
        let norm = normalize_stem(&word);
        FORBIDDEN_NORMS.contains(&norm.as_str()).then_some(norm)
    })
}

/// First generic directory word below the scan leaf, if any.
///
/// Compares each component per word so `models` and `rules_types` both hit.
fn generic_component(path: &str) -> Option<String> {
    let rel = below_leaf(path)?;
    let (parent, _) = rel.rsplit_once('/')?;
    parent.split('/').find_map(generic_word_in)
}

/// Generic word in the file stem, if any.
///
/// Catches `types.rs` as well as compounds like `service_types.rs`.
fn generic_file_word(path: &str) -> Option<String> {
    let rel = below_leaf(path)?;
    generic_word_in(&basename_stem(rel)?)
}

/// Generic directory or file-name hit for one path, if any.
fn generic_hit(path: &str) -> Option<String> {
    if let Some(word) = generic_component(path) {
        return Some(format!(
            "{path}: generic directory '{word}' (use capability/domain name)"
        ));
    }
    generic_file_word(path)
        .map(|word| format!("{path}: generic file name '{word}' (use capability/domain name)"))
}

/// Files with a generic directory or file name, one hit per file, sorted.
///
/// Only `src/`, `tests/`, and `benches/` leaves count, including
/// package-prefixed paths like `collate/src/utils/x.rs`. Top-level `docs/`
/// holds skill docs and stays silent.
#[must_use]
pub fn generic_dirs(rs_files: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = rs_files
        .iter()
        .filter_map(|path| generic_hit(path))
        .collect();
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::naming::grouping::generic_dirs;

    #[test]
    fn generic_dirs_flagged() {
        let files = vec![
            "src/models/album.rs".to_owned(),
            "src/utils/format.rs".to_owned(),
            "tests/handlers/flow.rs".to_owned(),
            "benches/types/load.rs".to_owned(),
            "collate/src/common/x.rs".to_owned(),
            "src/helpers/parse.rs".to_owned(),
            "src/shared/cache.rs".to_owned(),
        ];
        assert_eq!(generic_dirs(&files).len(), 7, "generic dirs flag");
        let clean = vec!["src/playback/engine.rs".to_owned()];
        assert!(
            generic_dirs(&clean).is_empty(),
            "capability dirs stay silent"
        );
    }

    #[test]
    fn scaffolding_dirs_flagged() {
        let files = vec![
            "src/test/flow.rs".to_owned(),
            "src/tests/flow.rs".to_owned(),
            "src/testing/flow.rs".to_owned(),
            "tests/support/flow.rs".to_owned(),
            "tests/supports/flow.rs".to_owned(),
            "benches/bench/flow.rs".to_owned(),
            "benches/benches/flow.rs".to_owned(),
            "src/benchmark/flow.rs".to_owned(),
            "src/benchmarks/flow.rs".to_owned(),
            "src/mocks/flow.rs".to_owned(),
            "src/mock/flow.rs".to_owned(),
            "tests/fixtures/flow.rs".to_owned(),
            "tests/fixture/flow.rs".to_owned(),
            "src/stubs/flow.rs".to_owned(),
            "src/fakes/flow.rs".to_owned(),
            "src/doubles/flow.rs".to_owned(),
            "src/examples/flow.rs".to_owned(),
            "src/docs/flow.rs".to_owned(),
            "src/specs/flow.rs".to_owned(),
            "src/tools/flow.rs".to_owned(),
            "src/scripts/flow.rs".to_owned(),
            "src/generics/flow.rs".to_owned(),
            "src/globals/flow.rs".to_owned(),
            "src/utilities/flow.rs".to_owned(),
            "collate/src/support/x.rs".to_owned(),
        ];
        assert_eq!(generic_dirs(&files).len(), 25, "scaffolding dirs flag");
        let clean = vec![
            "src/playback/engine.rs".to_owned(),
            "tests/verification/flow.rs".to_owned(),
        ];
        assert!(
            generic_dirs(&clean).is_empty(),
            "capability and grouping dirs stay silent"
        );
    }

    #[test]
    fn compound_and_file_names_flagged() {
        let files = vec![
            "src/types.rs".to_owned(),
            "src/rules_types.rs".to_owned(),
            "src/api/service_types.rs".to_owned(),
            "src/rules_types/load.rs".to_owned(),
            "src/Service-Types/load.rs".to_owned(),
        ];
        assert_eq!(generic_dirs(&files).len(), 5, "types stems flag");
        let clean = vec![
            "src/playback/engine.rs".to_owned(),
            "src/rules_simple/engine.rs".to_owned(),
        ];
        assert!(
            generic_dirs(&clean).is_empty(),
            "capability compounds stay silent"
        );
    }
}
