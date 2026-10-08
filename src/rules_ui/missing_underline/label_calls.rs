//! Label-bearing call spans for underline checks.
//!
//! Finds `.label(`-style method calls plus mnemonic-associated constructors
//! (`Button::new(`, `Label::new(`, `with_mnemonic(`) in blanked code, mapping
//! each to its argument line range so literal scans stay position-aware:
//! format placeholders, URLs, and `snake_case` strings outside label
//! arguments never count.

use crate::rules_ui::{
    missing_labels::labels_trigger::{builder_type_at, cursor_after_gap},
    missing_underline::{
        mnemonic_text::has_mnemonic_literal, mnemonic_widgets::is_mnemonic_widget,
    },
    ui_ext::is_ident_byte,
};

/// Method names carrying user-visible label text.
const LABEL_METHODS: [&str; 4] = ["label", "title", "accept_label", "set_label"];

/// Associated constructors carrying user-visible label text.
const ASSOC_CTORS: [&str; 3] = ["new", "with_label", "with_mnemonic"];

/// Label-bearing call span in code lines, with mnemonic association.
///
/// Start and end are zero-based line offsets into the scanned code slice;
/// both point at the same line for single-line calls.
#[derive(Debug, Clone, Copy)]
pub struct LabelCall {
    /// Zero-based start line offset holding the call opener.
    pub start: usize,
    /// Zero-based end line offset holding the matching closer.
    pub end: usize,
    /// True when the callee is mnemonic by construction.
    pub assoc_mnemonic: bool,
}

/// Opener of `.name(` at `idx` for one method spelling, if any.
///
/// Requires the leading dot, a trailing identifier boundary, and an opener
/// past gaps, so `.labels(` and bare mentions stay silent.
///
/// # Arguments
///
/// * `bytes` - Code bytes under scan.
/// * `idx` - Candidate dot index.
/// * `method` - Expected method spelling.
///
/// # Returns
///
/// * `Option<usize>` - Byte index of the argument opener, if it opens here.
fn method_call_open(bytes: &[u8], idx: usize, method: &str) -> Option<usize> {
    if bytes.get(idx) != Some(&b'.') {
        return None;
    }
    let start = idx.saturating_add(1);
    let end = start.saturating_add(method.len());
    if bytes.get(start..end) != Some(method.as_bytes()) {
        return None;
    }
    if bytes.get(end).is_some_and(|next| is_ident_byte(*next)) {
        return None;
    }
    let open = cursor_after_gap(bytes, end);
    (bytes.get(open) == Some(&b'(')).then_some(open)
}

/// Opener of `.name(` at `idx` for any label method, if any.
///
/// # Arguments
///
/// * `bytes` - Code bytes under scan.
/// * `idx` - Candidate dot index.
///
/// # Returns
///
/// * `Option<usize>` - Byte index of the argument opener, if it opens here.
fn method_open_at(bytes: &[u8], idx: usize) -> Option<usize> {
    LABEL_METHODS
        .iter()
        .find_map(|method| method_call_open(bytes, idx, method))
}

/// True if the associated callee at `sep` is mnemonic by construction.
///
/// `with_mnemonic` counts for any receiver; `new` and `with_label` count
/// only on mnemonic widget basenames, so `Box::new` and dialog constructors
/// stay out.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `sep` - Byte index of the first `:` in `::`.
/// * `ctor` - Associated name after `::`.
///
/// # Returns
///
/// * `bool` - True for mnemonic-associated callees.
fn assoc_is_mnemonic(body: &str, sep: usize, ctor: &str) -> bool {
    if ctor == "with_mnemonic" {
        return true;
    }
    builder_type_at(body, sep).is_some_and(is_mnemonic_widget)
}

/// Opener of `::ctor(` at `idx` for one constructor spelling, if any.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `bytes` - Code bytes under scan.
/// * `idx` - Candidate index of the first `:`.
/// * `ctor` - Expected associated spelling.
///
/// # Returns
///
/// * `Option<(usize, bool)>` - Opener index with mnemonic flag on matches.
fn assoc_call_open(body: &str, bytes: &[u8], idx: usize, ctor: &str) -> Option<(usize, bool)> {
    let name_start = idx.saturating_add(2);
    let end = name_start.saturating_add(ctor.len());
    if bytes.get(name_start..end) != Some(ctor.as_bytes()) {
        return None;
    }
    if bytes.get(end).is_some_and(|next| is_ident_byte(*next)) {
        return None;
    }
    let open = cursor_after_gap(bytes, end);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    assoc_is_mnemonic(body, idx, ctor).then_some((open, true))
}

/// Opener of a mnemonic-associated `::ctor(` at `idx`, if any.
///
/// Non-mnemonic receivers (`Box::new`, dialog constructors) stay silent by
/// construction, so only mnemonic-associated spans enter the result.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `bytes` - Code bytes under scan.
/// * `idx` - Candidate index of the first `:`.
///
/// # Returns
///
/// * `Option<(usize, bool)>` - Opener index with mnemonic flag on matches.
fn assoc_open_at(body: &str, bytes: &[u8], idx: usize) -> Option<(usize, bool)> {
    if bytes.get(idx) != Some(&b':') || bytes.get(idx.saturating_add(1)) != Some(&b':') {
        return None;
    }
    ASSOC_CTORS
        .iter()
        .find_map(|ctor| assoc_call_open(body, bytes, idx, ctor))
}

/// Paren depth change of `byte`.
///
/// # Arguments
///
/// * `byte` - Byte under scan.
///
/// # Returns
///
/// * `i32` - Plus one for openers, minus one for closers, else zero.
const fn paren_delta(byte: u8) -> i32 {
    if byte == b'(' {
        1
    } else if byte == b')' {
        -1
    } else {
        0
    }
}

/// End line of the call opened at `open`, if it closes in `text`.
///
/// Counts lines from `start_line` while balancing parens; blanked code holds
/// no string interiors, so every paren counts except rare block-comment
/// text.
///
/// # Arguments
///
/// * `text` - Joined code text with strings blanked and tails cut.
/// * `open` - Byte index of the argument opener.
/// * `start_line` - Zero-based line offset holding `open`.
///
/// # Returns
///
/// * `Option<usize>` - Zero-based end line offset of the closer, if any.
fn match_end_line(text: &str, open: usize, start_line: usize) -> Option<usize> {
    let tail = text.get(open.saturating_add(1)..)?;
    let mut depth: i32 = 1;
    let mut line = start_line;
    for byte in tail.bytes() {
        depth = depth.saturating_add(paren_delta(byte));
        if byte == b'\n' {
            line = line.saturating_add(1);
        }
        if depth <= 0 {
            return Some(line);
        }
    }
    None
}

/// Label-bearing call opening at `idx`, if any.
///
/// Method calls record no mnemonic association; associated calls only return
/// for mnemonic receivers, so the result already encodes widget scope.
///
/// # Arguments
///
/// * `text` - Joined code text with strings blanked and tails cut.
/// * `bytes` - Code bytes under scan.
/// * `idx` - Candidate byte index.
/// * `line` - Zero-based line offset holding `idx`.
///
/// # Returns
///
/// * `Option<LabelCall>` - Call span when a label call opens here.
fn call_at(text: &str, bytes: &[u8], idx: usize, line: usize) -> Option<LabelCall> {
    if let Some(open) = method_open_at(bytes, idx) {
        return match_end_line(text, open, line).map(|end| LabelCall {
            start: line,
            end,
            assoc_mnemonic: false,
        });
    }
    let (open, mnemonic) = assoc_open_at(text, bytes, idx)?;
    match_end_line(text, open, line).map(|end| LabelCall {
        start: line,
        end,
        assoc_mnemonic: mnemonic,
    })
}

/// Label-bearing call spans in code lines.
///
/// # Arguments
///
/// * `code_lines` - Blanked `fn` body lines with tails cut.
///
/// # Returns
///
/// * `Vec<LabelCall>` - Spans in scan order.
#[must_use]
pub fn label_calls(code_lines: &[String]) -> Vec<LabelCall> {
    let text = code_lines.join("\n");
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut idx = 0;
    let mut line = 0;
    while idx < bytes.len() {
        if let Some(call) = call_at(&text, bytes, idx, line) {
            out.push(call);
        }
        line = line.saturating_add(usize::from(bytes.get(idx) == Some(&b'\n')));
        idx = idx.saturating_add(1);
    }
    out
}

/// True if the original lines of `call` hold a mnemonic marker.
///
/// # Arguments
///
/// * `orig_lines` - Original `fn` body lines with strings intact.
/// * `call` - Label-bearing call span to inspect.
///
/// # Returns
///
/// * `bool` - True when `_<alphanumeric>` appears in the span.
#[must_use]
pub fn call_has_literal(orig_lines: &[String], call: &LabelCall) -> bool {
    let end = call.end.saturating_add(1);
    orig_lines
        .get(call.start..end)
        .is_some_and(|slice| has_mnemonic_literal(&slice.join("\n")))
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::missing_underline::label_calls::{call_has_literal, label_calls};

    #[test]
    fn method_spans_record() {
        let code = vec!["let b = Button::builder().label(\"_Open\").build();".to_owned()];
        let calls = label_calls(&code);
        assert_eq!(calls.len(), 1, "label call records once");
        assert!(
            !calls.first().is_some_and(|call| call.assoc_mnemonic),
            "method calls carry no association"
        );
        let orig = vec!["let b = Button::builder().label(\"_Open\").build();".to_owned()];
        assert!(
            calls
                .first()
                .is_some_and(|call| call_has_literal(&orig, call)),
            "mnemonic arg counts"
        );
    }

    #[test]
    fn assoc_spans_carry_mnemonic() {
        let code = vec!["let b = Button::new(\"_Open\");".to_owned()];
        let calls = label_calls(&code);
        assert_eq!(calls.len(), 1, "ctor call records once");
        assert!(
            calls.first().is_some_and(|call| call.assoc_mnemonic),
            "mnemonic receiver associates"
        );
        let boxed = vec!["let b = Box::new(x, y);".to_owned()];
        assert!(
            label_calls(&boxed).is_empty(),
            "non-mnemonic receivers stay silent"
        );
    }

    #[test]
    fn multiline_spans_resolve() {
        let code = vec![
            "let b = Button::builder()".to_owned(),
            "    .label(".to_owned(),
            "        \"_Open\",".to_owned(),
            "    )".to_owned(),
            "    .build();".to_owned(),
        ];
        let calls = label_calls(&code);
        assert_eq!(calls.len(), 1, "multiline call records once");
        assert!(
            calls.first().is_some_and(|call| call.end == 3),
            "span reaches the closer line"
        );
        let orig = vec![
            "let b = Button::builder()".to_owned(),
            "    .label(".to_owned(),
            "        \"_Open\",".to_owned(),
            "    )".to_owned(),
            "    .build();".to_owned(),
        ];
        assert!(
            calls
                .first()
                .is_some_and(|call| call_has_literal(&orig, call)),
            "multiline literal counts"
        );
    }

    #[test]
    fn plain_args_stay_silent() {
        let code = vec!["let b = Button::builder().label(\"Open\").build();".to_owned()];
        let calls = label_calls(&code);
        assert_eq!(calls.len(), 1, "plain call still records");
        let orig = vec!["let b = Button::builder().label(\"Open\").build();".to_owned()];
        assert!(
            calls
                .first()
                .is_none_or(|call| !call_has_literal(&orig, call)),
            "plain args stay silent"
        );
    }
}
