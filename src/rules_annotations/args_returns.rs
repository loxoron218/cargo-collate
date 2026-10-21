//! Doc `# Arguments` and `# Returns` consistency against signatures.
//!
//! Flags documented parameters missing from the signature and documented
//! names absent from it, plus empty or misplaced sections, only when the
//! section header already exists. Exemptions mirror `missing_fn_docs`.

#[cfg(test)]
mod edge_cases;
#[cfg(test)]
mod scenarios;

use std::collections::{BTreeSet, HashSet};

use crate::{
    gating::gated_stems,
    lexer::{
        literal::strip_strings,
        word::{declared_fn_name, has_word},
    },
    rules_annotations::{
        joined_signature,
        missing_headers::{attachment::scan_above, is_test_file, std_methods::is_std_method},
        sections::{is_section_header, is_test_fn},
        signature::{params_of, sig_param_names, split_params},
    },
    rules_harness::{
        range::{code_of, stray_gap, test_ranges},
        suite_comments::attached_above,
    },
    scan::{SourceFile, indexed_lines},
};

/// True if `code` is any `/// #` section header.
fn is_any_header(code: &str) -> bool {
    let trimmed = code.trim_start();
    if !trimmed.starts_with("///") || trimmed.starts_with("////") {
        return false;
    }
    let after = trimmed.get(3..).unwrap_or("").trim_start();
    after.starts_with('#')
}

/// True if `line` is a non-empty `///` doc line.
fn has_doc_content(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(body) = trimmed.strip_prefix("///") else {
        return false;
    };
    if body.starts_with('/') {
        return false;
    }
    !body.trim().is_empty()
}

/// Documented argument name in a strict bullet, if any.
fn parse_arg_bullet(line: &str) -> Option<String> {
    let body = line.trim_start().strip_prefix("///")?;
    if body.starts_with('/') {
        return None;
    }
    let rest = body.trim_start().strip_prefix("* `")?;
    let end = rest.find('`')?;
    let name = rest.get(..end)?;
    let after = rest.get(end.saturating_add(1)..)?.trim_start();
    let desc = after.strip_prefix(['-', '–', '—'])?;
    if desc.trim().is_empty() || name.is_empty() {
        return None;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let first = name.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    Some(name.to_owned())
}

/// Hit line for `file` at zero-based `idx` using original display text.
fn hit_at(file: &SourceFile, idx: usize) -> Option<String> {
    let line = file.lines.get(idx)?;
    Some(format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
}

/// Position of the header with `marker` in ordered attachment, if any.
fn header_pos(ordered: &[(usize, &String)], marker: &str) -> Option<usize> {
    ordered
        .iter()
        .position(|(_, line)| is_section_header(&strip_strings(line), marker))
}

/// Documented `(name, file index)` bullets after ordered `pos` up to the next header.
fn section_bullets(ordered: &[(usize, &String)], pos: usize) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut k = pos.saturating_add(1);
    while let Some((j, line)) = ordered.get(k) {
        if is_any_header(&strip_strings(line)) {
            break;
        }
        if let Some(name) = parse_arg_bullet(line) {
            out.push((name, *j));
        }
        k = k.saturating_add(1);
    }
    out
}

/// True if the section after ordered `pos` holds no doc content.
fn section_empty(ordered: &[(usize, &String)], pos: usize) -> bool {
    let mut k = pos.saturating_add(1);
    while let Some((_, line)) = ordered.get(k) {
        if is_any_header(&strip_strings(line)) {
            break;
        }
        if has_doc_content(line) {
            return false;
        }
        k = k.saturating_add(1);
    }
    true
}

/// Argument hits for `sig` against ordered docs at the `fn` at `fn_idx`.
fn args_hits(
    file: &SourceFile,
    ordered: &[(usize, &String)],
    fn_idx: usize,
    sig: &str,
) -> Vec<String> {
    let Some(pos) = header_pos(ordered, "# Arguments") else {
        return Vec::new();
    };
    let Some((h, _)) = ordered.get(pos) else {
        return Vec::new();
    };
    let h = *h;
    let bullets = section_bullets(ordered, pos);
    if bullets.is_empty() {
        return hit_at(file, h).into_iter().collect();
    }
    let sig_names = sig_param_names(sig);
    let sig_set: BTreeSet<&str> = sig_names.iter().map(String::as_str).collect();
    let doc_set: BTreeSet<&str> = bullets.iter().map(|(name, _)| name.as_str()).collect();
    let mut out = Vec::new();
    if sig_names
        .iter()
        .any(|name| !doc_set.contains(name.as_str()))
        && let Some(hit) = hit_at(file, fn_idx)
    {
        out.push(hit);
    }
    for (name, j) in &bullets {
        if !sig_set.contains(name.as_str())
            && let Some(hit) = hit_at(file, *j)
        {
            out.push(hit);
        }
    }
    out
}

/// Return hit for ordered docs, if the Returns section body is empty.
fn returns_hit(file: &SourceFile, ordered: &[(usize, &String)]) -> Option<String> {
    let pos = header_pos(ordered, "# Returns")?;
    let (h, _) = ordered.get(pos)?;
    let h = *h;
    section_empty(ordered, pos)
        .then(|| hit_at(file, h))
        .flatten()
}

/// Section hits for the documented `fn` at `fn_idx` with `sig`.
fn fn_section_hits(file: &SourceFile, fn_idx: usize, sig: &str) -> Vec<String> {
    let mut ordered = attached_above(&file.stripped, fn_idx);
    ordered.reverse();
    let mut out = args_hits(file, &ordered, fn_idx, sig);
    if let Some(hit) = returns_hit(file, &ordered) {
        out.push(hit);
    }
    out
}

/// True if `=` sits outside brackets after the parameter list closed.
///
/// Real signatures never hold one there; delegation macros emit `= name,`.
/// Naive angle spans keep `where` bindings like `Item = u8` silent, and a
/// `>` only closes a span after a non-`-` character so `->` never corrupts.
fn top_level_eq(sig: &str) -> bool {
    let mut paren: i32 = 0;
    let mut angle: i32 = 0;
    let mut closed = false;
    let mut prev = ' ';
    for c in sig.chars() {
        if c == '(' {
            paren = paren.saturating_add(1);
            prev = c;
            continue;
        }
        if c == ')' {
            paren = paren.saturating_sub(1);
            closed = closed || paren <= 0;
            prev = c;
            continue;
        }
        if c == '<' {
            angle = angle.saturating_add(1);
            prev = c;
            continue;
        }
        if c == '>' && prev == '-' {
            prev = c;
            continue;
        }
        if c == '>' {
            angle = angle.saturating_sub(1);
            prev = c;
            continue;
        }
        if c == '=' && paren <= 0 && angle <= 0 && closed {
            return true;
        }
        prev = c;
    }
    false
}

/// True if `sig` is macro DSL rather than a real signature.
///
/// Real parameters always carry types; delegation macros add `=` clauses
/// and untyped adorners the expansion fills in.
fn is_macro_sig(sig: &str) -> bool {
    let Some(params) = params_of(sig) else {
        return true;
    };
    if split_params(&params).iter().any(|piece| {
        let trimmed = piece.trim();
        !trimmed.is_empty() && trimmed != "..." && !has_word(piece, "self") && !piece.contains(':')
    }) {
        return true;
    }
    top_level_eq(sig)
}

/// Signature of the documented non-exempt `fn` at `idx`, if it needs a check.
fn checked_sig(
    file: &SourceFile,
    gated: &HashSet<String>,
    exempt: &[(usize, usize)],
    idx: usize,
    code: &str,
) -> Option<String> {
    let text = code_of(code);
    let visible = !text.contains("/*") && !text.contains("*/");
    let explained = scan_above(&file.stripped, idx).documented;
    if !(visible && explained) {
        return None;
    }
    let name = declared_fn_name(&text)?;
    let sig = joined_signature(&file.stripped, idx, 12);
    if is_macro_sig(&sig) {
        return None;
    }
    let keep = !is_test_fn(file, gated, exempt, idx)
        && name != "main"
        && !is_std_method(&name)
        && has_word(&sig, "fn");
    keep.then_some(sig)
}

/// Section hits for every documented non-exempt `fn` in `file`.
///
/// # Arguments
///
/// * `file` - Scanned source file with original and blanked lines.
/// * `gated` - Parent-gated test stems for exemption checks.
///
/// # Returns
///
/// Section hits as `path:line:content` lines.
fn file_section_hits(file: &SourceFile, gated: &HashSet<String>) -> Vec<String> {
    let exempt = test_ranges(&file.lines, stray_gap);
    let mut out = Vec::new();
    for (idx, (_, code)) in indexed_lines(file) {
        let Some(sig) = checked_sig(file, gated, &exempt, idx, code) else {
            continue;
        };
        out.extend(fn_section_hits(file, idx, &sig));
    }
    out
}

/// Documented arguments and returns against `fn` signatures.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// Mismatched doc sections as `path:line:content` hits.
#[must_use]
pub fn doc_args_returns(files: &[SourceFile]) -> Vec<String> {
    let gated = gated_stems(files);
    files
        .iter()
        .filter(|file| !is_test_file(file, &gated))
        .flat_map(|file| file_section_hits(file, &gated))
        .collect()
}
