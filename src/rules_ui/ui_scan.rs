//! Byte-level scan for declarative UI and CSS references.
//!
//! Keeps slash-pair and block-comment handling beside string blanking so
//! code references stay visible while comment text stays silent.

use std::ops::ControlFlow::{self, Break, Continue};

use crate::rules_ui::ui_ext::{boundary_before, has_ui_ext_at};

/// Scanner state for one line across strings and comments.
struct LineScan {
    /// True inside a double-quoted or byte-string literal.
    in_double: bool,
    /// Hash count while inside a raw string, if any.
    raw_hashes: Option<usize>,
    /// Open block depth outside strings.
    block_depth: i32,
}

/// Raw-string opener at `idx`, if any.
///
/// Accepts byte and plain forms with a word boundary before the opener so
/// trailing identifier letters never open; raw identifiers stay excluded
/// since no quote follows the hashes.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate opener index.
///
/// # Returns
///
/// * `Option<(usize, usize)>` - Hash count and content start past the quote.
fn raw_opener_at(bytes: &[u8], idx: usize) -> Option<(usize, usize)> {
    if !boundary_before(bytes, idx) {
        return None;
    }
    let mut cursor = idx;
    if bytes.get(cursor) == Some(&b'b') && bytes.get(cursor.saturating_add(1)) == Some(&b'r') {
        cursor = cursor.saturating_add(1);
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor = cursor.saturating_add(1);
    let mut hashes: usize = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes = hashes.saturating_add(1);
        cursor = cursor.saturating_add(1);
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    Some((hashes, cursor.saturating_add(1)))
}

/// End index past the raw-string closer opening at `idx`, if any.
///
/// A closer is a quote followed by exactly `hashes` markers, mirroring the
/// opener counted by [`raw_opener_at`].
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate closer index.
/// * `hashes` - Hash count of the opener.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the closer.
fn raw_closer_end_at(bytes: &[u8], idx: usize, hashes: usize) -> Option<usize> {
    if bytes.get(idx) != Some(&b'"') {
        return None;
    }
    let mut cursor = idx.saturating_add(1);
    let mut seen = 0;
    while seen < hashes {
        if bytes.get(cursor) != Some(&b'#') {
            return None;
        }
        seen = seen.saturating_add(1);
        cursor = cursor.saturating_add(1);
    }
    Some(cursor)
}

/// True if a byte-string opener sits at `idx`.
///
/// Requires a word boundary before the opener so identifier tails never open.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate opener index.
///
/// # Returns
///
/// * `bool` - True for byte quotes outside identifiers.
fn is_byte_opener_at(bytes: &[u8], idx: usize) -> bool {
    boundary_before(bytes, idx)
        && bytes.get(idx) == Some(&b'b')
        && bytes.get(idx.saturating_add(1)) == Some(&b'"')
}

/// Resume index after handling a raw closer at `idx`, if any.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `Option<usize>` - Resume index past the closer.
fn raw_resume_at(bytes: &[u8], idx: usize, state: &mut LineScan) -> Option<usize> {
    let hashes = state.raw_hashes?;
    let end = raw_closer_end_at(bytes, idx, hashes)?;
    state.raw_hashes = None;
    Some(end)
}

/// Advance past a double-quoted interior at `idx`.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `usize` - Resume index for the next step.
fn step_double(bytes: &[u8], idx: usize, state: &mut LineScan) -> usize {
    if bytes.get(idx) == Some(&b'\\') {
        return idx.saturating_add(2);
    }
    if bytes.get(idx) == Some(&b'"') {
        state.in_double = false;
        return idx.saturating_add(1);
    }
    idx.saturating_add(1)
}

/// Advance past block-comment text at `idx`.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `usize` - Resume index for the next step.
fn step_block(bytes: &[u8], idx: usize, state: &mut LineScan) -> usize {
    if bytes.get(idx) == Some(&b'/') && bytes.get(idx.saturating_add(1)) == Some(&b'*') {
        state.block_depth = state.block_depth.saturating_add(1);
        return idx.saturating_add(2);
    }
    if bytes.get(idx) == Some(&b'*') && bytes.get(idx.saturating_add(1)) == Some(&b'/') {
        state.block_depth = state.block_depth.saturating_sub(1);
        return idx.saturating_add(2);
    }
    idx.saturating_add(1)
}

/// Resume index after handling normal code at `idx`, if any.
///
/// Opens strings, raw strings, and block comments; a slash-pair opener ends
/// the line so trailing comments never count as references.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `Option<usize>` - Resume index, or `None` when the line comment ends code.
fn step_code(bytes: &[u8], idx: usize, state: &mut LineScan) -> Option<usize> {
    if bytes.get(idx) == Some(&b'/') && bytes.get(idx.saturating_add(1)) == Some(&b'/') {
        return None;
    }
    if bytes.get(idx) == Some(&b'/') && bytes.get(idx.saturating_add(1)) == Some(&b'*') {
        state.block_depth = state.block_depth.saturating_add(1);
        return Some(idx.saturating_add(2));
    }
    if let Some((hashes, content)) = raw_opener_at(bytes, idx) {
        state.raw_hashes = Some(hashes);
        return Some(content);
    }
    if bytes.get(idx) == Some(&b'"') {
        state.in_double = true;
        return Some(idx.saturating_add(1));
    }
    if is_byte_opener_at(bytes, idx) {
        state.in_double = true;
        return Some(idx.saturating_add(2));
    }
    Some(idx.saturating_add(1))
}

/// Next step while inside a raw string.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `ControlFlow<bool, usize>` - Hit or resume index.
fn scan_raw_step(bytes: &[u8], idx: usize, state: &mut LineScan) -> ControlFlow<bool, usize> {
    if has_ui_ext_at(bytes, idx) {
        return Break(true);
    }
    if let Some(end) = raw_resume_at(bytes, idx, state) {
        return Continue(end);
    }
    Continue(idx.saturating_add(1))
}

/// Next step while inside a double-quoted literal.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `ControlFlow<bool, usize>` - Hit or resume index.
fn scan_double_step(bytes: &[u8], idx: usize, state: &mut LineScan) -> ControlFlow<bool, usize> {
    if has_ui_ext_at(bytes, idx) {
        return Break(true);
    }
    Continue(step_double(bytes, idx, state))
}

/// Next step in normal code with comment and string handling.
///
/// Extensions never hit here; only string-literal interiors count, so bare
/// identifiers such as a `ui` method stay silent.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `ControlFlow<bool, usize>` - Line end or resume index.
fn scan_code_step(bytes: &[u8], idx: usize, state: &mut LineScan) -> ControlFlow<bool, usize> {
    step_code(bytes, idx, state).map_or(Break(false), Continue)
}

/// Next scan step for one index: hit, line end, or resume index.
///
/// Dispatches on raw-string, double-quoted, block-comment, and code states
/// so the driver loop stays flat for the nesting lint.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Current index.
/// * `state` - Mutable scan state.
///
/// # Returns
///
/// * `ControlFlow<bool, usize>` - Hit, line end, or resume index.
fn scan_one(bytes: &[u8], idx: usize, state: &mut LineScan) -> ControlFlow<bool, usize> {
    if state.raw_hashes.is_some() {
        return scan_raw_step(bytes, idx, state);
    }
    if state.in_double {
        return scan_double_step(bytes, idx, state);
    }
    if state.block_depth > 0 {
        return Continue(step_block(bytes, idx, state));
    }
    scan_code_step(bytes, idx, state)
}

/// True if `line` references a denied declarative extension in a string literal.
///
/// Scans string-literal interiors only: double-quoted, byte-string, and raw
/// contents count while bare code, slash-pair tails, and block interiors stay
/// silent, so `self.ui(` methods and documentation examples never flag.
/// Matching is ASCII case-insensitive with a trailing identifier boundary so
/// `view.uix` stays silent.
///
/// # Arguments
///
/// * `line` - Original source line with strings intact.
///
/// # Returns
///
/// * `bool` - True when `.ui`, `.blp`, `.xml`, or `.css` appears in a string.
#[must_use]
pub fn has_ui_reference(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut state = LineScan {
        in_double: false,
        raw_hashes: None,
        block_depth: 0,
    };
    let mut idx = 0;
    while idx < bytes.len() {
        match scan_one(bytes, idx, &mut state) {
            Break(hit) => return hit,
            Continue(next) => idx = next,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::ui_scan::has_ui_reference;

    #[test]
    fn extensions_flagged_in_code() {
        for line in [
            "let s = \"window.ui\";",
            "let s = \"view.blp\";",
            "let s = \"data.xml\";",
            "let s = \"style.css\";",
            "let s = include_str!(\"window.ui\");",
            "let p = \"a/WINDOW.UI\";",
        ] {
            assert!(has_ui_reference(line), "{line} flags");
        }
    }

    #[test]
    fn clean_code_stays_silent() {
        for line in [
            "let btn = Button::builder().build();",
            "btn.add_css_class(\"circular\");",
            "let x = 1;",
            "let s = \"view.uix\";",
            "let s = \"data.xml2\";",
            "let s = \"style.cssx\";",
        ] {
            assert!(!has_ui_reference(line), "{line} stays silent");
        }
    }

    #[test]
    fn comments_stay_silent() {
        for line in [
            "// window.ui",
            "let x = 1; // window.ui",
            "/* window.ui */",
            "let x = 1; /* window.ui */",
        ] {
            assert!(!has_ui_reference(line), "{line} stays silent");
        }
    }

    #[test]
    fn bare_idents_stay_silent() {
        for line in [
            "terminal.draw(|f| self.ui(f, messages))?;",
            "let x = foo.ui;",
            "let y = self.css;",
            "let z = widgets.xml();",
            "let w = theme.blp;",
        ] {
            assert!(!has_ui_reference(line), "{line} stays silent");
        }
    }

    #[test]
    fn strings_count_as_code() {
        for line in [
            "let s = r#\"window.ui\"#;",
            "let s = b\"style.css\";",
            "let s = \"a\";",
        ] {
            let flagged = line.contains("window.ui") || line.contains("style.css");
            assert_eq!(has_ui_reference(line), flagged, "{line} routes");
        }
    }
}
