//! Doc blank-line detectors for `///` openers.

use crate::{
    lexer::{
        literal::strip_strings,
        marker::{cut_line_comment, is_doc_line},
        word::has_word,
    },
    rules_harness::range::brace_count,
    scan::SourceFile,
};

/// Scan state for doc blank-line checks in one file.
#[derive(Default)]
struct DocBlankState {
    /// Current brace depth before the pending line.
    depth: i32,
    /// Open frames as exempt struct/enum body plus depth before its opener.
    frames: Vec<(bool, i32)>,
    /// True when a struct/enum header without its opener was seen.
    pending: bool,
}

/// True if stripped `code` declares a struct, enum, or union item.
///
/// Unions share struct-like field bodies, so their docs follow the same
/// blank-line rules. Reference: Rust Reference items
/// (`doc.rust-lang.org/reference/items.html`).
fn is_struct_enum_head(code: &str) -> bool {
    has_word(code, "struct") || has_word(code, "enum") || has_word(code, "union")
}

/// True if stripped `code` opens another item that cancels a pending header.
///
/// Complete item-head set (struct/enum handled separately via
/// [`is_struct_enum_head`]): `fn`, `impl`, `mod`, `trait`, `type`, `const`,
/// `static`, `use`, `macro`, plus `union` and `extern` which also open items.
/// Reference: Rust Reference items
/// (`doc.rust-lang.org/reference/items.html`).
fn is_other_item_head(code: &str) -> bool {
    [
        "fn", "impl", "mod", "trait", "type", "const", "static", "use", "macro", "union", "extern",
    ]
    .iter()
    .any(|kw| has_word(code, kw))
}

/// True if the scan sits inside a struct/enum body.
///
/// Any enclosing frame counts, so field docs inside struct-variant bodies of
/// enums stay exempt even though the variant brace itself is not a struct or
/// enum head. Closed frames pop off the stack, so only open bodies exempt.
fn inside_struct_enum(state: &DocBlankState) -> bool {
    state
        .frames
        .iter()
        .any(|(exempt, entry)| *exempt && state.depth > *entry)
}

/// True if the doc line at `idx` is a block opener missing its blank line.
///
/// Continuations after another doc line, openers at the start of file,
/// openers directly after an empty line, and openers starting a brace or
/// paren block stay allowed. The brace arm is required because `rustfmt`
/// forbids the blank line there; the paren arm covers item-generating macro
/// invocations such as `signed_request!(` whose docs route into the output.
fn is_flagged_opener(lines: &[String], idx: usize) -> bool {
    let Some(line) = lines.get(idx) else {
        return false;
    };
    if !is_doc_line(&strip_strings(line)) {
        return false;
    }
    let Some(prev_idx) = idx.checked_sub(1) else {
        return false;
    };
    let Some(prev) = lines.get(prev_idx) else {
        return false;
    };
    if is_doc_line(&strip_strings(prev)) {
        return false;
    }
    if prev.trim().is_empty() {
        return false;
    }
    !is_block_opener(prev)
}

/// True if stripped `prev` opens a brace or paren block for the next line.
fn is_block_opener(prev: &str) -> bool {
    let stripped = strip_strings(prev);
    let opened = cut_line_comment(&stripped).trim_end();
    opened.ends_with('{') || opened.ends_with('(')
}

/// Push one brace frame; returns the depth after opening.
fn push_frame(state: &mut DocBlankState, exempt: bool) {
    state.frames.push((exempt, state.depth));
    state.depth = state.depth.saturating_add(1);
}

/// Drop the innermost brace frame, if any.
fn pop_one_frame(state: &mut DocBlankState) {
    if let Some(next) = state.frames.len().checked_sub(1) {
        state.frames.truncate(next);
    }
}

/// Close `closes` braces, dropping frames whose opener left the depth.
fn pop_frames(state: &mut DocBlankState, closes: i32) {
    let mut remaining = closes;
    while remaining > 0 {
        state.depth = state.depth.saturating_sub(1);
        while state
            .frames
            .last()
            .is_some_and(|(_, entry)| *entry >= state.depth)
        {
            pop_one_frame(state);
        }
        remaining = remaining.saturating_sub(1);
    }
}

/// Whether the first opener on `code` belongs to a struct/enum body.
fn first_opener_exempt(state: &mut DocBlankState, code: &str, opens: i32) -> bool {
    if opens <= 0 {
        return false;
    }
    if is_struct_enum_head(code) {
        state.pending = false;
        return true;
    }
    if state.pending && !is_other_item_head(code) {
        state.pending = false;
        return true;
    }
    if is_other_item_head(code) {
        state.pending = false;
    }
    false
}

/// Track a struct/enum header without its opener on `code`.
///
/// Only a `{` line (resolved by `first_opener_exempt`) or a `;` line may
/// resolve a pending header. Where-clause contents such as `'static`
/// lifetimes or `fn` bounds never cancel it, since keyword matching cannot
/// tell them apart from new item heads.
fn track_pending(state: &mut DocBlankState, code: &str, opens: i32) {
    if code.contains(';') {
        state.pending = false;
        return;
    }
    if opens > 0 {
        return;
    }
    if is_struct_enum_head(code) {
        state.pending = true;
    }
}

/// Advance brace depth and struct/enum frames from one stripped line.
fn track_braces(state: &mut DocBlankState, stripped: &str) {
    let code = cut_line_comment(stripped);
    let opens = brace_count(code, '{');
    let closes = brace_count(code, '}');
    if opens <= 0 {
        track_pending(state, code, opens);
    } else {
        let exempt_first = first_opener_exempt(state, code, opens);
        let mut remaining = opens;
        let mut first = true;
        while remaining > 0 {
            push_frame(state, first && exempt_first);
            first = false;
            remaining = remaining.saturating_sub(1);
        }
        if code.contains(';') {
            state.pending = false;
        }
    }
    pop_frames(state, closes);
}

/// Doc blank-line findings for one file.
fn check_file_doc_blank(file: &SourceFile) -> Vec<String> {
    let mut state = DocBlankState::default();
    let mut out = Vec::new();
    for (idx, (line, code)) in file.lines.iter().zip(file.stripped.iter()).enumerate() {
        let stripped = strip_strings(code);
        if is_doc_line(&stripped)
            && is_flagged_opener(&file.stripped, idx)
            && !inside_struct_enum(&state)
        {
            out.push(format!("{}:{}:{line}", file.path, idx.saturating_add(1)));
        }
        track_braces(&mut state, &stripped);
    }
    out
}

/// Doc `///` openers without a preceding blank line.
///
/// Only the first line of a consecutive doc block is checked; continuations,
/// quadruple `////` lines, field docs anywhere inside struct/enum bodies
/// (including struct-variant bodies), and docs opening a brace or paren
/// block stay exempt. A doc line at the start of file is allowed.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn doc_blank_hits(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_doc_blank).collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::hygiene::comment_blank::doc_blank_hits, scan::builders::test_file};

    #[test]
    fn doc_blank_flags_opener_after_code() {
        let files = vec![test_file(
            "src/a.rs",
            &["fn helper() {}", "/// Loads an item."],
        )];
        assert_eq!(doc_blank_hits(&files).len(), 1);
        let clean = vec![test_file(
            "src/a.rs",
            &["fn helper() {}", "", "/// Loads an item."],
        )];
        assert!(
            doc_blank_hits(&clean).is_empty(),
            "opener after blank stays silent"
        );
    }

    #[test]
    fn doc_blank_edge_shapes() {
        let bof = vec![test_file("src/a.rs", &["/// First line."])];
        assert!(doc_blank_hits(&bof).is_empty(), "BOF stays silent");
        let cont = vec![test_file(
            "src/a.rs",
            &["fn f() {}", "", "/// First.", "/// Second."],
        )];
        assert!(
            doc_blank_hits(&cont).is_empty(),
            "continuations stay silent"
        );
        let quad = vec![test_file("src/a.rs", &["fn f() {}", "//// Rule"])];
        assert!(doc_blank_hits(&quad).is_empty(), "quadruple stays silent");
        let trailing = vec![test_file("src/a.rs", &["let x = 1; /// trailing"])];
        assert!(
            doc_blank_hits(&trailing).is_empty(),
            "trailing stays silent"
        );
        let in_string = vec![test_file("src/a.rs", &[r#"let s = "/// not doc";"#])];
        assert!(
            doc_blank_hits(&in_string).is_empty(),
            "string contents stay silent"
        );
        let ws_blank = vec![test_file("src/a.rs", &["fn f() {}", "   ", "/// Doc."])];
        assert!(
            doc_blank_hits(&ws_blank).is_empty(),
            "whitespace counts as blank"
        );
    }

    #[test]
    fn doc_blank_paren_opener_exempt() {
        let wrapped = vec![test_file(
            "src/a.rs",
            &[
                "signed_request!(",
                "    /// Sends a signed GET request.",
                "    pub fn f() {}",
                ");",
            ],
        )];
        assert!(
            doc_blank_hits(&wrapped).is_empty(),
            "paren openers stay silent"
        );
        let after_close = vec![test_file(
            "src/a.rs",
            &["let x = foo(", "    bar,", ");", "/// Docs without blank."],
        )];
        assert_eq!(
            doc_blank_hits(&after_close).len(),
            1,
            "openers after close still flag"
        );
        let paren_string = vec![test_file("src/a.rs", &[r#"let s = "(";"#, "/// Docs."])];
        assert_eq!(
            doc_blank_hits(&paren_string).len(),
            1,
            "parens in strings stay inert"
        );
    }

    #[test]
    fn doc_blank_struct_enum_exempt() {
        let fields = vec![test_file(
            "src/a.rs",
            &[
                "struct OrderState {",
                "    /// Current depth.",
                "    depth: i32,",
                "    /// Seen flag.",
                "    seen_code: bool,",
                "}",
            ],
        )];
        assert!(doc_blank_hits(&fields).is_empty(), "struct fields exempt");
        let variants = vec![test_file(
            "src/a.rs",
            &[
                "enum Kind {",
                "    /// First variant.",
                "    First,",
                "    /// Second variant.",
                "    Second,",
                "}",
            ],
        )];
        assert!(doc_blank_hits(&variants).is_empty(), "enum variants exempt");
        let block_open = vec![test_file(
            "src/a.rs",
            &["impl Foo {", "    /// Method docs.", "    fn f() {}", "}"],
        )];
        assert!(
            doc_blank_hits(&block_open).is_empty(),
            "block openers stay silent"
        );
        let nested_variant = vec![test_file(
            "src/a.rs",
            &[
                "pub enum Event {",
                "    Start {",
                "        /// Album to analyze.",
                "        album_id: AlbumId,",
                "        /// Track file paths.",
                "        tracks: Vec<PathBuf>,",
                "    },",
                "}",
            ],
        )];
        assert!(
            doc_blank_hits(&nested_variant).is_empty(),
            "struct-variant fields exempt"
        );
        let split_header = vec![test_file(
            "src/a.rs",
            &[
                "pub struct SortListBuilder<T: SortListBounds, F, U>",
                "where",
                "    T::Criteria: 'static,",
                "{",
                "    /// The `ListBox` being built.",
                "    list_box: ListBox,",
                "    /// Tracks the current order.",
                "    order_map: u32,",
                "}",
            ],
        )];
        assert!(
            doc_blank_hits(&split_header).is_empty(),
            "where-clause lifetimes keep the header pending"
        );
        let method = vec![test_file(
            "src/a.rs",
            &[
                "impl Foo {",
                "    fn g() {",
                "    }",
                "    /// Method docs.",
                "    fn f() {}",
                "}",
            ],
        )];
        assert_eq!(doc_blank_hits(&method).len(), 1, "impl docs still flag");
    }
}
