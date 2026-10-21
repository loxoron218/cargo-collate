//! Arrow-in-generics and macro-delegation edge cases.

use crate::{rules_annotations::args_returns::doc_args_returns, scan::builders::test_file};

#[test]
fn arrow_inside_generics_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Runs the boxed callback.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `cb` - Callback producing the lookup table.",
            "/// * `x` - Seed value.",
            "fn f(cb: Box<dyn Fn() -> HashMap<String, Vec<i32>>>, x: i32) -> i32 { x }",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "arrow generics silent");
}

#[test]
fn arrow_inside_generics_missing_flags() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Runs the boxed callback.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `cb` - Callback producing the lookup table.",
            "fn f(cb: Box<dyn Fn() -> HashMap<String, Vec<i32>>>, x: i32) -> i32 { x }",
        ],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "missing param flags");
    assert!(
        hits.first().is_some_and(|h| h.contains("src/a.rs:6:")),
        "fn line flagged"
    );
}

#[test]
fn macro_delegation_with_arrow_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Downloads one track.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `track_id` - Track identifier.",
            "/// * `cancel` - Optional cancellation flag.",
            "pub fn download_track(track_id: i32, mk: Box<dyn Fn() -> i32>) -> PathBuf = download,",
            "cancel: Option<bool>",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "delegation silent");
}
