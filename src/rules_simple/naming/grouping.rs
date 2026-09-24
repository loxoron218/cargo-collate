//! Generic directory detection for capability grouping.
//!
//! Flags files under layer/role or scaffolding directories so modules group
//! by capability/domain instead.

use crate::rules_simple::{layout::below_leaf, naming::stem::normalize_stem};

/// Normalized generic directory stems, already singular.
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

/// True if one directory component uses a generic name.
///
/// Compares the whole component normalized so `models` and `Model` both hit
/// while `rules_types` stays silent as a capability name.
fn component_is_generic(component: &str) -> bool {
    let norm = normalize_stem(component);
    FORBIDDEN_NORMS.contains(&norm.as_str())
}

/// First generic directory component below the scan leaf, if any.
///
/// Compares whole components normalized so `models` and `model` both hit.
fn generic_component(path: &str) -> Option<String> {
    let rel = below_leaf(path)?;
    let (parent, _) = rel.rsplit_once('/')?;
    parent
        .split('/')
        .find(|component| component_is_generic(component))
        .map(str::to_owned)
}

/// Files under a generic directory, one hit per file, sorted.
///
/// Only `src/`, `tests/`, and `benches/` leaves count, including
/// package-prefixed paths like `collate/src/utils/x.rs`.
#[must_use]
pub fn generic_dirs(rs_files: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = rs_files
        .iter()
        .filter_map(|path| {
            generic_component(path).map(|dir| {
                format!("{path}: generic directory '{dir}' (use capability/domain name)")
            })
        })
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
}
