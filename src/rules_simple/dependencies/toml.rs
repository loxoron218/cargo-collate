//! String-aware TOML scanners for manifest checks.
//!
//! Every helper shares the sibling quote tracker so comment marks, braces,
//! and key tokens inside strings never scan as syntax.

use crate::rules_simple::dependencies::quotes::live_bytes;

/// Line without its `#` comment tail outside strings.
#[must_use]
pub fn strip_comment(line: &str) -> &str {
    live_bytes(line)
        .into_iter()
        .find_map(|(i, byte)| (byte == b'#').then_some(i))
        .map_or(line, |i| line.get(..i).unwrap_or(line))
}

/// Inner text of a `[header]` or `[[header]]` line, if any.
#[must_use]
pub fn section_inner(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("[[") {
        let rest = trimmed.strip_prefix("[[")?;
        let inner = rest.strip_suffix("]]")?;
        return Some(inner.trim());
    }
    if trimmed.starts_with('[') {
        let rest = trimmed.strip_prefix('[')?;
        let inner = rest.strip_suffix(']')?;
        return Some(inner.trim());
    }
    None
}

/// Byte indexes of live dots splitting header segments.
fn segment_cuts(inner: &str) -> Vec<usize> {
    live_bytes(inner)
        .into_iter()
        .filter_map(|(i, byte)| (byte == b'.').then_some(i))
        .collect()
}

/// Header segments split on dots outside quotes.
fn split_segments(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    for cut in segment_cuts(inner) {
        parts.push(inner.get(start..cut).unwrap_or("").trim().to_owned());
        start = cut.saturating_add(1);
    }
    parts.push(inner.get(start..).unwrap_or("").trim().to_owned());
    parts
}

/// True for list-style dependency sections.
#[must_use]
pub fn is_dep_list(inner: &str) -> bool {
    split_segments(inner).last().is_some_and(|last| {
        last == "dependencies" || last == "dev-dependencies" || last == "build-dependencies"
    })
}

/// Dependency name for table-style headers, if any.
#[must_use]
pub fn dep_table_name(inner: &str) -> Option<String> {
    let parts = split_segments(inner);
    if parts.len() < 2 {
        return None;
    }
    let len = parts.len();
    let marker = parts.get(len.saturating_sub(2))?;
    if marker != "dependencies" && marker != "dev-dependencies" && marker != "build-dependencies" {
        return None;
    }
    let raw = parts.get(len.saturating_sub(1))?;
    let name = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_owned())
}

/// Byte index of the first `=` outside strings, if any.
#[must_use]
pub fn find_equals(text: &str) -> Option<usize> {
    live_bytes(text)
        .into_iter()
        .find_map(|(i, byte)| (byte == b'=').then_some(i))
}

/// Depth stepped by one live bracket byte.
const fn stepped_depth(depth: i32, byte: u8) -> i32 {
    if byte == b'{' || byte == b'[' {
        return depth.saturating_add(1);
    }
    if byte == b'}' || byte == b']' {
        return depth.saturating_sub(1);
    }
    depth
}

/// Net open depth of braces and brackets outside strings.
#[must_use]
pub fn brace_delta(text: &str) -> i32 {
    let mut depth: i32 = 0;
    for (_, byte) in live_bytes(text) {
        depth = stepped_depth(depth, byte);
    }
    depth
}

/// Index after ASCII whitespace from `j`.
fn skip_ws(bytes: &[u8], mut j: usize) -> usize {
    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
        j = j.saturating_add(1);
    }
    j
}

/// End index after a bare `default-features` key at `i`, if any.
fn bare_key_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i > 0
        && bytes
            .get(i.saturating_sub(1))
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    {
        return None;
    }
    if bytes.get(i..i.saturating_add("default-features".len()))
        == Some(b"default-features".as_slice())
    {
        return Some(i.saturating_add("default-features".len()));
    }
    if bytes.get(i..i.saturating_add("default_features".len()))
        == Some(b"default_features".as_slice())
    {
        return Some(i.saturating_add("default_features".len()));
    }
    None
}

/// End index after a quoted `default-features` key at `i`, if any.
fn quoted_key_end(bytes: &[u8], i: usize) -> Option<usize> {
    let quote = bytes.get(i).copied().unwrap_or(0);
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut j = i.saturating_add(1);
    while j < bytes.len() && bytes.get(j) != Some(&quote) {
        j = j.saturating_add(1);
    }
    if bytes.get(j) != Some(&quote) {
        return None;
    }
    let inner = bytes.get(i.saturating_add(1)..j).unwrap_or_default();
    if inner != b"default-features".as_slice() && inner != b"default_features".as_slice() {
        return None;
    }
    Some(j.saturating_add(1))
}

/// True if bare `false` with a word boundary opens at `j`.
fn false_at(bytes: &[u8], j: usize) -> bool {
    let end = j.saturating_add("false".len());
    if bytes.get(j..end) != Some(b"false".as_slice()) {
        return false;
    }
    bytes
        .get(end)
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// True if a `default-features = false` pair opens at `i` outside strings.
fn pair_at(bytes: &[u8], i: usize) -> bool {
    let mut start = bare_key_end(bytes, i);
    if start.is_none() {
        start = quoted_key_end(bytes, i);
    }
    let Some(begin) = start else {
        return false;
    };
    let mut j = skip_ws(bytes, begin);
    if bytes.get(j) != Some(&b'=') {
        return false;
    }
    j = skip_ws(bytes, j.saturating_add(1));
    false_at(bytes, j)
}

/// True if one comment-free line sets the opt-out.
fn pair_in_line(line: &str) -> bool {
    let code = strip_comment(line);
    let bytes = code.as_bytes();
    live_bytes(code).into_iter().any(|(i, _)| pair_at(bytes, i))
}

/// True if `text` sets `default-features` to bare `false` outside strings.
#[must_use]
pub fn has_default_false(text: &str) -> bool {
    for line in text.lines() {
        if pair_in_line(line) {
            return true;
        }
    }
    false
}

/// Byte indexes of live top-level commas in inline-table `inner`.
fn comma_cuts(inner: &str) -> Vec<usize> {
    let mut depth: i32 = 0;
    let mut cuts = Vec::new();
    for (i, byte) in live_bytes(inner) {
        depth = stepped_depth(depth, byte);
        if byte == b',' && depth == 0 {
            cuts.push(i);
        }
    }
    cuts
}

/// Comma-separated top-level parts of inline-table `inner`.
fn split_top_level(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for cut in comma_cuts(inner) {
        let part = inner.get(start..cut).unwrap_or("").trim();
        if !part.is_empty() {
            out.push(part.to_owned());
        }
        start = cut.saturating_add(1);
    }
    let tail = inner.get(start..).unwrap_or("").trim();
    if !tail.is_empty() {
        out.push(tail.to_owned());
    }
    out
}

/// Key without quotes and value without surrounding whitespace, if splittable.
fn split_pair(part: &str) -> Option<(&str, &str)> {
    let eq = find_equals(part)?;
    let key = part.get(..eq).map_or("", str::trim);
    let key = key.trim_matches('"').trim_matches('\'').trim();
    let value = part.get(eq.saturating_add(1)..).map_or("", str::trim);
    Some((key, value))
}

/// True if inline `value` holds only `workspace = true`.
#[must_use]
pub fn is_pure_workspace_inline(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(rest) = trimmed.strip_prefix('{') else {
        return false;
    };
    let Some(suffixed) = rest.strip_suffix('}') else {
        return false;
    };
    let inner = suffixed.trim();
    let mut count: i32 = 0;
    for part in split_top_level(inner) {
        let Some((key, val)) = split_pair(&part) else {
            return false;
        };
        if key != "workspace" || val.trim() != "true" {
            return false;
        }
        count = count.saturating_add(1);
    }
    count == 1
}

/// True if a `[dependencies.name]` body holds only `workspace = true`.
#[must_use]
pub fn is_pure_workspace_block(lines: &[String]) -> bool {
    let mut count: i32 = 0;
    for line in lines {
        let trimmed = strip_comment(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, val)) = split_pair(trimmed) else {
            return false;
        };
        let last = key.rsplit('.').next().unwrap_or("");
        if last != "workspace" || val.trim() != "true" {
            return false;
        }
        count = count.saturating_add(1);
    }
    count == 1
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::dependencies::toml::{
        brace_delta, dep_table_name, find_equals, has_default_false, is_dep_list,
        is_pure_workspace_block, is_pure_workspace_inline, section_inner, strip_comment,
    };

    #[test]
    fn comments_cut_outside_strings() {
        assert_eq!(strip_comment("a = 1 # tail"), "a = 1 ");
        assert_eq!(
            strip_comment("v = \"1#0\" # tail"),
            "v = \"1#0\" ",
            "hashes in strings stay"
        );
        assert_eq!(strip_comment("# whole"), "");
        assert_eq!(strip_comment("v = 'a#b'"), "v = 'a#b'");
    }

    #[test]
    fn headers_classified() {
        assert_eq!(section_inner("[dependencies]"), Some("dependencies"));
        assert_eq!(section_inner("[[bin]]"), Some("bin"));
        assert_eq!(section_inner("not a header"), None);
        assert!(is_dep_list("dependencies"));
        assert!(is_dep_list("workspace.dependencies"));
        assert!(is_dep_list("target.'cfg(unix)'.dependencies"));
        assert!(is_dep_list("target.'cfg(unix)'.dev-dependencies"));
        assert!(!is_dep_list("package"));
        assert!(!is_dep_list("features"));
        assert_eq!(
            dep_table_name("dependencies.serde"),
            Some("serde".to_owned())
        );
        assert_eq!(
            dep_table_name("target.'cfg(unix)'.dependencies.serde"),
            Some("serde".to_owned())
        );
        assert_eq!(dep_table_name("dependencies"), None);
        assert_eq!(dep_table_name("package"), None);
    }

    #[test]
    fn equals_and_braces_skip_strings() {
        assert_eq!(find_equals("a = \"b=c\""), Some(2));
        assert_eq!(find_equals("no equals"), None);
        assert_eq!(brace_delta("{ a = 1 }"), 0);
        assert_eq!(brace_delta("{ a = 1,"), 1);
        assert_eq!(brace_delta("v = \"{o}\""), 0);
    }

    #[test]
    fn default_false_shapes() {
        assert!(has_default_false("default-features = false"));
        assert!(has_default_false("default-features=false"));
        assert!(has_default_false("\"default-features\" = false"));
        assert!(has_default_false("default_features = false"));
        assert!(has_default_false("a = 1\ndefault-features = false"));
        assert!(!has_default_false("default-features = true"));
        assert!(!has_default_false("default-features = \"false\""));
        assert!(!has_default_false("# default-features = false"));
        assert!(!has_default_false("v = \"default-features = false\""));
        assert!(!has_default_false("my-default-features = false"));
    }

    #[test]
    fn pure_workspace_shapes() {
        assert!(is_pure_workspace_inline("{ workspace = true }"));
        assert!(
            !is_pure_workspace_inline("{ workspace = true, features = [\"a\"] }"),
            "overrides stay impure"
        );
        assert!(!is_pure_workspace_inline("{ version = \"1\" }"));
        assert!(is_pure_workspace_block(&["workspace = true".to_owned()]));
        assert!(
            !is_pure_workspace_block(&[
                "workspace = true".to_owned(),
                "features = [\"a\"]".to_owned()
            ]),
            "overrides stay impure"
        );
    }
}
