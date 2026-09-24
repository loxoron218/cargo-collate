//! Raw-string and quoted-literal blanking.

use crate::{lexer::marker::skip_block_comment, rules_tests::range::quoted_close};

/// Span of a raw string.
///
/// `end` holds the resume index past the closer when it sits on the opened
/// line; `None` carries the opener hash count into the next line.
struct RawSpan {
    /// Hash count of the opener, matched by the closer.
    hashes: usize,
    /// Resume index past the closer, if on the opened line.
    end: Option<usize>,
}

/// Multi-line raw-string blanking across ordered lines.
///
/// `strip_strings` blanks each line in isolation, so continuation lines of a
/// multi-line raw string scan as code. Feed a file's lines through
/// `blank_line` in order, including skipped ones, so carried state stays
/// aligned; interiors blank to empty while code around them scans normally.
/// A slash-pair marker outside strings ends code for the line with the tail
/// kept raw, so openers inside comments never open carried state. Byte variants
/// and any hash count stay covered through `raw_open_span`; `r#ident` raw
/// identifiers never open.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawStringLines {
    /// Hash count of a raw string carried past the previous line, if any.
    open: Option<usize>,
    /// True if a quoted string continues past the previous line ending.
    quoted: bool,
}

impl RawStringLines {
    /// Blank string and char literals in `line`, carrying open strings on.
    pub fn blank_line(&mut self, line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let Some(start) = self.resume(&chars) else {
            return String::new();
        };
        let mut out = String::with_capacity(line.len());
        let mut open_quote = false;
        scan_line(&chars, start, &mut out, &mut self.open, &mut open_quote);
        self.quoted = open_quote;
        out
    }

    /// Resume index past any carried string closer, if the line holds one.
    ///
    /// Returns `None` when the whole line sits inside the carried string.
    fn resume(&mut self, chars: &[char]) -> Option<usize> {
        if let Some(hashes) = self.open {
            let end = close_carried(chars, hashes)?;
            self.open = None;
            return Some(end);
        }
        if !self.quoted {
            return Some(0);
        }
        let past = quoted_close(chars, 0)?;
        self.quoted = false;
        Some(past)
    }
}

/// Opening hash count and content start of the raw string at `i`, if any.
///
/// Accepts byte variants (`br"..."`) and any hash count; `r#ident` raw
/// identifiers stay excluded since no quote follows the hashes.
#[must_use]
pub fn raw_open_span(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i.saturating_add(usize::from(chars.get(i) == Some(&'b')));
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j = j.saturating_add(1);
    let hash_start = j;
    while chars.get(j) == Some(&'#') {
        j = j.saturating_add(1);
    }
    (chars.get(j) == Some(&'"')).then_some((j.saturating_sub(hash_start), j.saturating_add(1)))
}

/// Resume index past the raw-string closer opening at `j`, if any.
///
/// A closer is `"` followed by exactly `hashes` markers, mirroring the
/// opener counted by `raw_open_span`.
fn close_at(chars: &[char], j: usize, hashes: usize) -> Option<usize> {
    if chars.get(j) != Some(&'"') {
        return None;
    }
    let mut k = j.saturating_add(1);
    while k.saturating_sub(j.saturating_add(1)) < hashes && chars.get(k) == Some(&'#') {
        k = k.saturating_add(1);
    }
    (k.saturating_sub(j.saturating_add(1)) == hashes).then_some(k)
}

/// Resume index past a carried raw-string closer on this line, if any.
fn close_carried(chars: &[char], hashes: usize) -> Option<usize> {
    (0..chars.len()).find_map(|j| close_at(chars, j, hashes))
}

/// Span of the raw string starting at `i`, if any.
///
/// Handles byte variants and any hash count.
fn raw_span_end(chars: &[char], i: usize) -> Option<RawSpan> {
    let (hashes, mut j) = raw_open_span(chars, i)?;
    while j < chars.len() {
        if let Some(past) = close_at(chars, j, hashes) {
            return Some(RawSpan {
                hashes,
                end: Some(past),
            });
        }
        j = j.saturating_add(1);
    }
    Some(RawSpan { hashes, end: None })
}

/// End exclusive of the quoted string whose quote sits at `i`.
fn quoted_end(chars: &[char], i: usize) -> usize {
    let mut j = i.saturating_add(1);
    while let Some(current) = chars.get(j) {
        if *current == '"' {
            break;
        }
        j = j.saturating_add(1);
        if *current == '\\' {
            j = j.saturating_add(1);
        }
    }
    let past = j.saturating_add(usize::from(j < chars.len()));
    past.min(chars.len().max(i.saturating_add(1)))
}

/// Index just past the brace closing an escape from `from`, if any.
fn close_brace(chars: &[char], from: usize) -> Option<usize> {
    (from..chars.len())
        .find(|&k| chars.get(k).is_some_and(|c| *c == '}'))
        .map(|k| k.saturating_add(1))
}

/// End exclusive of the char literal starting at `i`, if any.
///
/// Returns `None` for lifetimes and stray quotes.
fn char_literal_end(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i) != Some(&'\'') {
        return None;
    }
    let mut j = i.saturating_add(1);
    if chars.get(j) == Some(&'\\') {
        if chars.get(j.saturating_add(1)) == Some(&'u')
            && chars.get(j.saturating_add(2)) == Some(&'{')
        {
            j = close_brace(chars, j.saturating_add(3))?;
        } else {
            j = chars
                .get(j.saturating_add(1))
                .map(|_| j.saturating_add(2))?;
        }
    } else if chars.get(j).is_some_and(|c| *c != '\'' && *c != '\n') {
        j = j.saturating_add(1);
    } else {
        return None;
    }
    (chars.get(j) == Some(&'\'')).then(|| j.saturating_add(1))
}

/// Process the literal if any at `i`; returns the resume index.
///
/// An unterminated quoted string sets `open_quote` so callers scanning lines
/// in order can carry it into the next line.
fn step_literal(
    chars: &[char],
    i: usize,
    out: &mut String,
    open: &mut Option<usize>,
    open_quote: &mut bool,
) -> Option<usize> {
    if (chars.get(i) == Some(&'r') || chars.get(i) == Some(&'b'))
        && let Some(span) = raw_span_end(chars, i)
    {
        out.push_str("\"\"");
        if let Some(past) = span.end {
            return Some(past);
        }
        *open = Some(span.hashes);
        return Some(chars.len());
    }
    if chars.get(i) == Some(&'"')
        || (chars.get(i) == Some(&'b') && chars.get(i.saturating_add(1)) == Some(&'"'))
    {
        let quote = i.saturating_add(usize::from(chars.get(i) == Some(&'b')));
        out.push_str("\"\"");
        let end = quoted_end(chars, quote);
        let tail = chars.get(quote.saturating_add(1)..).unwrap_or(&[]);
        *open_quote = quoted_close(tail, 0).is_none();
        return Some(end);
    }
    if let Some(end) = char_literal_end(chars, i) {
        out.push_str("\"\"");
        return Some(end);
    }
    None
}

/// Replace every string and char literal in `line` with blank quotes.
///
/// A lone quote for lifetimes stays untouched.
#[must_use]
pub fn strip_strings(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut open = None;
    let mut open_quote = false;
    while let Some(current) = chars.get(i).copied() {
        if let Some(next) = step_literal(&chars, i, &mut out, &mut open, &mut open_quote) {
            i = next;
        } else {
            out.push(current);
            i = i.saturating_add(1);
        }
    }
    out
}

/// Blank literals from `start`, recording an open string in `open_quote`.
///
/// A slash-pair marker outside strings ends code for the line; the tail keeps
/// single-line literals blanked but never opens carried state, so comment
/// text stays visible to comment-aware callers without phantom strings.
/// Same-line block comments copy through verbatim, so quotes inside them
/// never open strings.
fn scan_line(
    chars: &[char],
    start: usize,
    out: &mut String,
    open: &mut Option<usize>,
    open_quote: &mut bool,
) {
    let mut i = start;
    while let Some(current) = chars.get(i).copied() {
        if let Some(end) = skip_block_comment(chars, i, out) {
            i = end;
            continue;
        }
        if current == '/'
            && chars.get(i.saturating_add(1)) == Some(&'/')
            && let Some(tail) = chars.get(i..)
        {
            let text: String = tail.iter().collect();
            out.push_str(&strip_strings(&text));
            break;
        }
        if let Some(next) = step_literal(chars, i, out, open, open_quote) {
            i = next;
        } else {
            out.push(current);
            i = i.saturating_add(1);
        }
    }
}

/// String-blanked lines for `lines`, aligned 1:1 with the input.
///
/// Like repeated `strip_strings` except multi-line raw-string interiors and
/// backslash-continued quoted strings blank via carried state, so every
/// detector shares identical analysis text from a single pass. Comments stay
/// intact for comment-aware callers; code positions cut them as before.
/// Display paths keep original lines.
#[must_use]
pub fn stripped_lines(lines: &[String]) -> Vec<String> {
    let mut blanker = RawStringLines::default();
    lines.iter().map(|line| blanker.blank_line(line)).collect()
}

#[cfg(test)]
mod tests {
    use crate::lexer::literal::{RawStringLines, strip_strings};

    #[test]
    fn single_line_raw_blanked() {
        assert_eq!(strip_strings("let q = r#\"a?b\"#;"), "let q = \"\";");
        assert_eq!(strip_strings("let q = br#\"a?b\"#;"), "let q = \"\";");
        assert_eq!(strip_strings("let q = r##\"a\"#b\"##;"), "let q = \"\";");
        assert_eq!(
            strip_strings("let r#type = 1;"),
            "let r#type = 1;",
            "raw identifiers stay code"
        );
    }

    #[test]
    fn multiline_raw_interiors_blank() {
        let mut blanker = RawStringLines::default();
        assert_eq!(
            blanker.blank_line("    let q = sqlx::query!("),
            "    let q = sqlx::query!("
        );
        assert_eq!(blanker.blank_line("        r#\""), "        \"\"");
        assert_eq!(blanker.blank_line("VALUES ( ? )"), String::new());
        assert_eq!(
            blanker.blank_line("        \"#, description"),
            ", description"
        );
        assert_eq!(
            blanker.blank_line("    let x = foo()?;"),
            "    let x = foo()?;",
            "state resets after the closer"
        );
    }

    #[test]
    fn comment_tails_never_open_state() {
        let mut blanker = RawStringLines::default();
        assert_eq!(
            blanker.blank_line("/// let string = r#\""),
            "/// let string = \"\""
        );
        assert_eq!(
            blanker.blank_line("/// fn main() {"),
            "/// fn main() {",
            "doc lines after comment openers stay code-visible"
        );
    }

    #[test]
    fn carried_closer_keeps_trailing_code() {
        let mut blanker = RawStringLines::default();
        assert_eq!(blanker.blank_line("let q = r#\""), "let q = \"\"");
        assert_eq!(blanker.blank_line("\"#"), String::new());
        assert_eq!(
            blanker.blank_line("let y = 1;"),
            "let y = 1;",
            "closer-only line resumes code after it"
        );
    }

    #[test]
    fn quoted_continuations_blank() {
        let mut blanker = RawStringLines::default();
        assert_eq!(blanker.blank_line("    foo(\""), "    foo(\"\"");
        assert_eq!(blanker.blank_line("        use x;"), String::new());
        assert_eq!(blanker.blank_line("        done\", y);"), ", y);");
        assert_eq!(
            blanker.blank_line("    let z = 1;"),
            "    let z = 1;",
            "state resets after the closer"
        );
    }

    #[test]
    fn escaped_quotes_keep_continuation() {
        let mut blanker = RawStringLines::default();
        assert_eq!(
            blanker.blank_line("    let s = \"a\\\""),
            "    let s = \"\""
        );
        assert_eq!(blanker.blank_line("    tail"), String::new());
        assert_eq!(blanker.blank_line("    end\";"), ";");
    }

    #[test]
    fn block_quotes_never_open_strings() {
        let mut blanker = RawStringLines::default();
        assert_eq!(
            blanker.blank_line("    code(/* \"a */ rest"),
            "    code(/* \"a */ rest"
        );
        assert_eq!(
            blanker.blank_line("    // plain comment"),
            "    // plain comment",
            "following comments still scan"
        );
    }
}
