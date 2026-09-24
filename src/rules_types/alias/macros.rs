//! Macro-interior masking for type aliases.
//!
//! `macro_rules!` definitions and `name!(...)`/`[...]`/`{...}`
//! invocations span to their balanced close across lines; mentions
//! inside never resolve syntactically, so discovery skips them while
//! verdicts veto (defining file) or skip (elsewhere) them. Definition
//! fragments without interpolation score as ordinary sites instead.

use crate::{rules_tests::range::code_of, rules_types::alias::head::skip_quoted};

/// True if `b` is ASCII whitespace without crossing lines.
const fn is_span_ws(b: u8) -> bool {
    b.is_ascii_whitespace() && b != b'\n'
}

/// True if `b` can continue an ASCII identifier.
const fn is_scan_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Byte index after skipping `skip` bytes backwards from `i`.
fn skip_back_while(bytes: &[u8], mut i: usize, skip: impl Fn(u8) -> bool) -> usize {
    while i > 0 && bytes.get(i.saturating_sub(1)).is_some_and(|b| skip(*b)) {
        i = i.saturating_sub(1);
    }
    i
}

/// True if the identifier run ending at `end` spells `word`.
fn word_ends_at(bytes: &[u8], end: usize, word: &str) -> bool {
    let start = skip_back_while(bytes, end, is_scan_ident);
    bytes.get(start..end) == Some(word.as_bytes())
}

/// True if the group opening at `j` is a `macro_rules!` definition body.
///
/// Only the body brace after `macro_rules! name` qualifies; ordinary
/// `name!(` invocations return false here.
fn is_def_body_open(bytes: &[u8], j: usize) -> bool {
    let after_ws = skip_back_while(bytes, j, is_span_ws);
    let run_start = skip_back_while(bytes, after_ws, is_scan_ident);
    if run_start >= after_ws {
        return false;
    }
    let before_name = skip_back_while(bytes, run_start, is_span_ws);
    let Some(bang) = before_name.checked_sub(1) else {
        return false;
    };
    if bytes.get(bang).copied() != Some(b'!') {
        return false;
    }
    word_ends_at(bytes, bang, "macro_rules")
}

/// True if the group opening at `j` belongs to a macro.
///
/// Direct invocations (`vec![`, `foo!(`, `bar! {`) put `!` right before
/// the bracket; `macro_rules! name {` hides behind its name, so one
/// identifier run may intervene — but only for `macro_rules`. Negation
/// (`if !ready {`), comparisons, closures, and ordinary blocks never
/// qualify, and openers stay same-line by stopping at newlines.
fn is_macro_open(bytes: &[u8], j: usize) -> bool {
    let after_ws = skip_back_while(bytes, j, is_span_ws);
    let run_start = skip_back_while(bytes, after_ws, is_scan_ident);
    if run_start < after_ws {
        let before_name = skip_back_while(bytes, run_start, is_span_ws);
        let Some(bang) = before_name.checked_sub(1) else {
            return false;
        };
        return bytes.get(bang).copied() == Some(b'!') && word_ends_at(bytes, bang, "macro_rules");
    }
    let Some(bang) = after_ws.checked_sub(1) else {
        return false;
    };
    if bytes.get(bang).copied() != Some(b'!') {
        return false;
    }
    let Some(prev) = bang.checked_sub(1) else {
        return false;
    };
    bytes.get(prev).is_some_and(|b| is_scan_ident(*b))
}

/// Closer matching `opener`, if any.
const fn group_closer(opener: u8) -> Option<u8> {
    match opener {
        b'(' => Some(b')'),
        b'[' => Some(b']'),
        b'{' => Some(b'}'),
        _ => None,
    }
}

/// Pop the top frame on a matching closer, recording macro spans.
fn close_macro_frame(
    byte: u8,
    j: usize,
    stack: &mut Vec<(u8, bool, usize)>,
    spans: &mut Vec<(usize, usize)>,
) {
    if stack.last().is_some_and(|frame| frame.0 == byte)
        && let Some((_, macro_group, open)) = stack.pop()
        && macro_group
    {
        spans.push((open, j.saturating_add(1)));
    }
}

/// Resume index after scanning the byte at `j`, tracking macro frames.
///
/// Openers push their closer with the macro flag; matching closers pop.
/// With definitions only, invocations track as ordinary groups so nested
/// brackets still balance without covering anything.
fn step_macro(
    bytes: &[u8],
    j: usize,
    stack: &mut Vec<(u8, bool, usize)>,
    spans: &mut Vec<(usize, usize)>,
    defs_only: bool,
) -> usize {
    if let Some(past) = skip_quoted(bytes, j) {
        return past;
    }
    let byte = bytes.get(j).copied().unwrap_or(0);
    if let Some(closer) = group_closer(byte) {
        let flagged = if defs_only {
            is_def_body_open(bytes, j)
        } else {
            is_macro_open(bytes, j)
        };
        stack.push((closer, flagged, j));
    } else {
        close_macro_frame(byte, j, stack, spans);
    }
    j.saturating_add(1)
}

/// Global spans of macro groups across joined stripped lines.
///
/// Frames track every bracket with its macro flag, so nested ordinary
/// groups never close a macro early. Unterminated macro frames run to end
/// of file, keeping the lint silent rather than misreading truncated
/// groups as code.
fn macro_spans(joined: &str, defs_only: bool) -> Vec<(usize, usize)> {
    let bytes = joined.as_bytes();
    let mut spans = Vec::new();
    let mut stack: Vec<(u8, bool, usize)> = Vec::new();
    let mut j = 0;
    while j < bytes.len() {
        j = step_macro(bytes, j, &mut stack, &mut spans, defs_only);
    }
    for (_, macro_group, open) in stack {
        if macro_group {
            spans.push((open, bytes.len()));
        }
    }
    spans
}

/// Ranges of `spans` clipped to the line at `start` with length `len`.
fn clip_line_ranges(spans: &[(usize, usize)], start: usize, len: usize) -> Vec<(usize, usize)> {
    let end = start.saturating_add(len);
    let mut ranges = Vec::new();
    for (open, close) in spans {
        let from = (*open).max(start);
        let to = (*close).min(end);
        if from < to {
            ranges.push((from.saturating_sub(start), to.saturating_sub(start)));
        }
    }
    ranges
}

/// Per-line byte ranges inside the selected macro spans.
///
/// Joins stripped lines with their byte offsets, then clips global spans
/// back to per-line ranges.
fn covers(lines: &[String], defs_only: bool) -> Vec<Vec<(usize, usize)>> {
    let mut stripped = Vec::with_capacity(lines.len());
    for line in lines {
        stripped.push(code_of(line));
    }
    let mut joined = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for text in &stripped {
        starts.push(joined.len());
        joined.push_str(text);
        joined.push('\n');
    }
    let spans = macro_spans(&joined, defs_only);
    let mut cover = Vec::with_capacity(lines.len());
    for (line_no, text) in stripped.iter().enumerate() {
        let start = starts.get(line_no).copied().unwrap_or(0);
        cover.push(clip_line_ranges(&spans, start, text.len()));
    }
    cover
}

/// Per-line byte ranges inside macro spans, in stripped-line coords.
///
/// `macro_rules!` definitions and `name!(…)`/`[…]`/`{…}` invocations span
/// to their balanced close across lines; unterminated groups run to end
/// of file. Ordinary blocks, negation, and comparisons never open spans.
#[must_use]
pub fn macro_cover(lines: &[String]) -> Vec<Vec<(usize, usize)>> {
    covers(lines, false)
}

/// Per-line byte ranges inside `macro_rules!` definition bodies only.
///
/// Invocations track as ordinary groups for balancing without covering
/// anything, so callers can tell template text apart from expanded code.
#[must_use]
pub fn macro_def_cover(lines: &[String]) -> Vec<Vec<(usize, usize)>> {
    covers(lines, true)
}

/// True if `tail` after `$` is a `$crate` path with identifier boundary.
///
/// Only the deterministic crate path expands literally; any other
/// `$fragment` keeps the veto since expansion could reshape the type.
fn is_crate_rooted(tail: &str) -> bool {
    tail.starts_with("crate")
        && tail.get("crate".len()..).is_none_or(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
        })
}

/// True if the `$` at `k` opens a deterministic `$crate` path.
fn dollar_is_clean(text: &str, k: usize) -> bool {
    let tail = text.get(k.saturating_add(1)..).unwrap_or("");
    is_crate_rooted(tail)
}

/// True if the definition range holding `pos` carries no interpolation.
///
/// Every `$` must open `$crate` (the deterministic crate path); any other
/// `$fragment` keeps the veto since expansion could reshape the type.
#[must_use]
pub fn def_range_clean(code: &str, ranges: &[(usize, usize)], pos: usize) -> bool {
    let Some((start, end)) = ranges
        .iter()
        .find(|(from, to)| *from <= pos && pos < *to)
        .copied()
    else {
        return false;
    };
    let text = code.get(start..end).unwrap_or("");
    let bytes = text.as_bytes();
    let mut k = 0;
    while k < bytes.len() {
        if bytes.get(k).copied() == Some(b'$') && !dollar_is_clean(text, k) {
            return false;
        }
        k = k.saturating_add(1);
    }
    true
}

/// True if byte `pos` on line `li` sits inside a macro range of `cover`.
#[must_use]
pub fn cover_hits(cover: &[Vec<(usize, usize)>], li: usize, pos: usize) -> bool {
    cover.get(li).is_some_and(|ranges| {
        ranges
            .iter()
            .any(|(start, end)| *start <= pos && pos < *end)
    })
}

#[cfg(test)]
mod tests {
    use crate::rules_types::alias::macros::{
        cover_hits, def_range_clean, macro_cover, macro_def_cover,
    };

    fn owned(lines: &[&str]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn macro_invocation_lines_covered() {
        let lines = owned(&[
            "fn f() -> Vec<u32> {",
            "    let xs = vec![",
            "        Token,",
            "    ];",
            "    xs",
            "}",
        ]);
        let cover = macro_cover(&lines);
        assert!(
            cover_hits(&cover, 1, 17),
            "the opening bracket sits inside the span"
        );
        assert!(cover_hits(&cover, 2, 8), "continuation lines stay covered");
        assert!(cover_hits(&cover, 3, 0), "the closing line stays covered");
        assert!(
            !cover_hits(&cover, 0, 4) && !cover_hits(&cover, 4, 4) && !cover_hits(&cover, 5, 0),
            "ordinary lines stay bare"
        );
    }

    #[test]
    fn negation_blocks_stay_bare() {
        let lines = owned(&[
            "fn f(ready: bool) {",
            "    if !ready {",
            "        let x = 1;",
            "    }",
            "    let done = a != b;",
            "    let f = |x| { x };",
            "    let s = Foo { x: 1 };",
            "}",
        ]);
        let cover = macro_cover(&lines);
        assert!(
            cover.iter().all(Vec::is_empty),
            "negation, comparison, closures, and struct literals never open spans"
        );
    }

    #[test]
    fn macro_rules_body_covered() {
        let lines = owned(&["macro_rules! m {", "    ($t:ty) => { Token; },", "}"]);
        let cover = macro_cover(&lines);
        assert!(
            cover_hits(&cover, 0, 15),
            "the definition brace opens the span"
        );
        assert!(cover_hits(&cover, 1, 8), "matcher lines stay covered");
        assert!(cover_hits(&cover, 2, 0), "the closing brace stays covered");
    }

    #[test]
    fn def_cover_ignores_invocations() {
        let lines = owned(&[
            "macro_rules! m {",
            "    () => { Token; },",
            "}",
            "fn f() { m!(); }",
        ]);
        let def = macro_def_cover(&lines);
        assert!(cover_hits(&def, 1, 14), "template lines stay covered");
        assert!(
            !cover_hits(&def, 3, 11),
            "invocations never count as definitions"
        );
    }

    #[test]
    fn def_range_cleanliness() {
        let code = "    shared: Rc<RefCell<T>>,";
        let ranges = vec![(0, code.len())];
        assert!(
            def_range_clean(code, &ranges, 14),
            "interpolation-free fragments score"
        );
        let dirty = "fn f(t: $ty) {}";
        let dirty_ranges = vec![(0, dirty.len())];
        assert!(
            !def_range_clean(dirty, &dirty_ranges, 9),
            "matcher variables keep the veto"
        );
        let rooted = "fn f() -> $crate::Error {}";
        let rooted_ranges = vec![(0, rooted.len())];
        assert!(
            def_range_clean(rooted, &rooted_ranges, 20),
            "deterministic crate paths score"
        );
    }
}
