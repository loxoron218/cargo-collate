//! Tests for documented arguments and returns consistency.

use crate::{rules_annotations::args_returns::doc_args_returns, scan::builders::test_file};

#[test]
fn matching_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "/// * `y` - Other value.",
            "///",
            "/// # Returns",
            "///",
            "/// Result value.",
            "fn f(x: i32, y: String) -> i32 { x }",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "matching stays silent");
}

#[test]
fn missing_param_flags_fn() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "///",
            "/// # Returns",
            "///",
            "/// Value.",
            "fn f(x: i32, y: i32) -> i32 { x }",
        ],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "missing param flags once");
    assert!(
        hits.first().is_some_and(|h| h.contains(":9:")),
        "fn flagged"
    );
}

#[test]
fn extra_bullet_flags_line() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "/// * `z` - Unknown value.",
            "fn f(x: i32) {}",
        ],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "extra bullet flags once");
    assert!(
        hits.first().is_some_and(|h| h.contains(":5:")),
        "bullet flagged"
    );
}

#[test]
fn prose_only_flags_header() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// Takes input.",
            "fn f(x: i32) {}",
        ],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "prose-only flags header");
    assert!(
        hits.first().is_some_and(|h| h.contains(":2:")),
        "header flagged"
    );
}

#[test]
fn empty_unit_returns_flags_header() {
    let files = vec![test_file(
        "src/a.rs",
        &["/// Docs.", "/// # Returns", "fn f(x: i32) {}"],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "empty unit returns flags");
    assert!(
        hits.first().is_some_and(|h| h.contains(":2:")),
        "header flagged"
    );
}

#[test]
fn nothing_returns_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "///",
            "/// # Returns",
            "///",
            "/// Nothing; side effects only.",
            "fn f(x: i32) {}",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "nothing stays silent");
}

#[test]
fn empty_returns_flags_header() {
    let files = vec![test_file(
        "src/a.rs",
        &["/// Docs.", "/// # Returns", "fn f() -> i32 { 1 }"],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "empty returns flags");
    assert!(
        hits.first().is_some_and(|h| h.contains(":2:")),
        "header flagged"
    );
}

#[test]
fn no_sections_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["/// Docs.", "fn f(x: i32) -> i32 { x }"],
    )];
    assert!(doc_args_returns(&files).is_empty(), "no sections silent");
}

#[test]
fn single_section_only() {
    let args_only = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "fn f(x: i32) {}",
        ],
    )];
    assert!(doc_args_returns(&args_only).is_empty(), "args-only silent");
    let returns_only = vec![test_file(
        "src/b.rs",
        &[
            "/// Docs.",
            "/// # Returns",
            "///",
            "/// Value.",
            "fn f() -> i32 { 1 }",
        ],
    )];
    assert!(
        doc_args_returns(&returns_only).is_empty(),
        "returns-only silent"
    );
}

#[test]
fn exempt_silent() {
    let files = vec![
        test_file(
            "tests/x.rs",
            &["/// D.", "/// # Arguments", "fn f(x: i32) {}"],
        ),
        test_file(
            "src/a.rs",
            &["/// D.", "/// # Arguments", "#[test]", "fn t(x: i32) {}"],
        ),
        test_file("src/b.rs", &["/// D.", "/// # Arguments", "fn main() {}"]),
        test_file(
            "src/c.rs",
            &[
                "impl Display for S {",
                "/// D.",
                "/// # Arguments",
                "fn fmt(&self, f: &mut Formatter<'_>) -> Result { Ok(()) }",
                "}",
            ],
        ),
    ];
    assert!(doc_args_returns(&files).is_empty(), "exempt items silent");
}

#[test]
fn multiline_self_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Docs.",
            "/// # Arguments",
            "///",
            "/// * `x` - Input value.",
            "///",
            "/// # Returns",
            "///",
            "/// Value.",
            "pub fn f(",
            "&self,",
            "mut x: i32,",
            ") -> i32 { x }",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "multiline silent");
}

#[test]
fn array_params_multiline_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Feeds one sample through a delay line.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `delay` - Delay line, newest sample at index 0.",
            "/// * `max_peak` - Largest peak observed so far.",
            "/// * `sample` - Incoming sample.",
            "/// * `observe` - Pure observation returning the candidate peak.",
            "///",
            "/// # Returns",
            "///",
            "/// Updated maximum peak.",
            "pub fn push_detector<const N: usize>(",
            "    delay: &mut [f64; N],",
            "    max_peak: &mut f64,",
            "    sample: f64,",
            "    observe: impl Fn(&[f64; N]) -> f64,",
            ") {",
            "}",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "array sig silent");
}

#[test]
fn arrow_inside_params_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Runs the callback.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `cb` - Callback producing the value.",
            "/// * `x` - Seed value.",
            "fn f(cb: impl Fn(i32) -> i32, x: i32) -> i32 { cb(x) }",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "arrow params silent");
}

#[test]
fn macro_delegation_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Downloads one track.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `track_id` - Track identifier.",
            "/// * `cancel` - Optional cancellation flag.",
            "pub fn download_track(track_id: i32) -> PathBuf = download,",
            "cancel: Option<bool>",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "delegation silent");
}

#[test]
fn macro_untyped_params_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Sends a request.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `client` - HTTP client implementation.",
            "/// * `base_url` - API base URL.",
            "pub fn signed_get(client, base_url) -> Result {",
            "}",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "untyped silent");
}

#[test]
fn endash_bullets_match_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Builds a column.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `cover_size` – Cover art size in pixels.",
            "/// * `cache` — Shared cover art cache.",
            "fn f(cover_size: i32, cache: &Cache) {}",
        ],
    )];
    assert!(doc_args_returns(&files).is_empty(), "dashes silent");
}

#[test]
fn generic_params_missing_flags() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "/// Builds the queue section.",
            "///",
            "/// # Arguments",
            "///",
            "/// * `evt_receiver` - Receives events.",
            "/// * `cmd_sender` - Sends commands.",
            "/// * `tasks` - Shared tasks map.",
            "pub fn build_queue_section<S, R>(",
            "    evt_receiver: Receiver<DownloadEvent>,",
            "    cmd_sender: Sender<DownloadCommand>,",
            "    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,",
            "    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,",
            ") -> QueueSection",
            "where",
            "    S: BuildHasher + 'static,",
            "    R: BuildHasher + 'static,",
            "{",
            "}",
        ],
    )];
    let hits = doc_args_returns(&files);
    assert_eq!(hits.len(), 1, "missing generic param flags");
    assert!(
        hits.first().is_some_and(|h| h.contains("src/a.rs:8:")),
        "fn line flagged"
    );
}
