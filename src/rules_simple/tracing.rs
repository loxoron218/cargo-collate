//! Structured-tracing detectors for interpolated or field-less events.

use crate::{
    rules_simple::{
        tracing_core::MaskedLiteral,
        tracing_mask::mask_text,
        tracing_parse::{
            effective_args, group_end, has_bang_name, is_field_arg, next_bang, paren_at, span_text,
        },
    },
    scan::SourceFile,
};

/// Unstructured tracing findings across files.
///
/// Flags `info!`, `warn!`, and `error!` calls that interpolate values into
/// the message or carry no fields; structured field-plus-message calls stay
/// silent. Applies everywhere including test code.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines with an optional fix hint appended.
#[must_use]
pub fn unstructured_tracing_hits(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            let masked = mask_text(&file.lines.join("\n"));
            file_hits(&file.path, &file.lines, &masked.text, &masked.literals)
        })
        .collect()
}

/// Unstructured tracing findings for one masked file.
fn file_hits(
    path: &str,
    lines: &[String],
    masked: &str,
    literals: &[MaskedLiteral],
) -> Vec<String> {
    let mut hits = Vec::new();
    let mut pos = 0;
    while pos < masked.len() {
        let Some(bang) = next_bang(masked, pos) else {
            break;
        };
        pos = bang.saturating_add(1);
        if let Some(hit) = check_call(path, lines, masked, literals, bang) {
            hits.push(hit);
        }
    }
    hits
}

/// Finding for one macro invocation starting at `bang`, if unstructured.
fn check_call(
    path: &str,
    lines: &[String],
    masked: &str,
    literals: &[MaskedLiteral],
    bang: usize,
) -> Option<String> {
    let open = paren_at(masked, bang)?;
    let end = group_end(masked, open)?;
    let inner = masked.get(open.saturating_add(1)..end).unwrap_or("");
    let fieldless = classify_group(inner, literals, open.saturating_add(1))?;
    let lineno = line_of(masked, bang);
    let line = lines
        .get(lineno.saturating_sub(1))
        .map_or("", String::as_str);
    let mut hit = format!("{path}:{lineno}:{line}");
    if fieldless {
        hit.push_str(" help: add a field, e.g. info!(item_id, \"message\")");
    }
    Some(hit)
}

/// Line number holding the byte offset in masked text, starting at one.
fn line_of(masked: &str, offset: usize) -> usize {
    let Some(prefix) = masked.as_bytes().get(..offset) else {
        return 1;
    };
    let mut breaks = 0_usize;
    for byte in prefix {
        if *byte == b'\n' {
            breaks = breaks.saturating_add(1);
        }
    }
    breaks.saturating_add(1)
}

/// Unstructured verdict for one macro group, if any.
///
/// `Some(true)` flags a field-less message with a fix hint; `Some(false)`
/// flags interpolation; `None` marks structured calls.
fn classify_group(inner: &str, literals: &[MaskedLiteral], base: usize) -> Option<bool> {
    let args = effective_args(inner);
    if args.is_empty() {
        return None;
    }
    if has_bang_name(inner, "format") || has_bang_name(inner, "format_args") {
        return Some(args.len() == 1);
    }
    let mut message = None;
    for (idx, span) in args.iter().enumerate() {
        if span_text(inner, *span).contains('"') {
            message = Some(idx);
        }
    }
    let msg_idx = message?;
    if msg_idx.saturating_add(1) < args.len() {
        return Some(false);
    }
    if !fields_valid(inner, &args, msg_idx) {
        return Some(false);
    }
    if message_has_placeholder(args.get(msg_idx).copied().unwrap_or((0, 0)), literals, base) {
        return Some(args.len() == 1);
    }
    (args.len() == 1).then_some(true)
}

/// True if every pre-message argument is a well-formed tracing field.
fn fields_valid(inner: &str, args: &[(usize, usize)], msg_idx: usize) -> bool {
    for span in args.get(..msg_idx).unwrap_or_default() {
        if !is_field_arg(span_text(inner, *span)) {
            return false;
        }
    }
    true
}

/// True if `content` holds an unescaped `{` format placeholder.
///
/// Doubled `{{` sequences stay exempt as escaped literals.
fn has_placeholder(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if opens_placeholder(bytes, pos) {
            return true;
        }
        pos = advance_content(bytes, pos);
    }
    false
}

/// True if an unescaped `{` opens at `pos`.
fn opens_placeholder(bytes: &[u8], pos: usize) -> bool {
    bytes.get(pos) == Some(&b'{') && bytes.get(pos.saturating_add(1)) != Some(&b'{')
}

/// Next scan position past the content byte at `pos`, escapes included.
fn advance_content(bytes: &[u8], pos: usize) -> usize {
    if bytes.get(pos) == Some(&b'\\') {
        return advance_escape(bytes, pos);
    }
    if bytes.get(pos) == Some(&b'{') {
        return pos.saturating_add(2);
    }
    pos.saturating_add(1)
}

/// Next scan position past the escape at `pos`.
///
/// Only `\u{...}` can hide braces; other escapes skip two bytes so a `\\`
/// before a placeholder never masks it.
fn advance_escape(bytes: &[u8], pos: usize) -> usize {
    if bytes.get(pos.saturating_add(1)) == Some(&b'u')
        && bytes.get(pos.saturating_add(2)) == Some(&b'{')
    {
        let mut scan = pos.saturating_add(3);
        while scan < bytes.len() && bytes.get(scan) != Some(&b'}') {
            scan = scan.saturating_add(1);
        }
        return scan.saturating_add(1);
    }
    pos.saturating_add(2)
}

/// True if any literal inside `span` holds a format placeholder.
///
/// `base` maps inner-relative spans onto masked-text literal offsets.
fn message_has_placeholder(span: (usize, usize), literals: &[MaskedLiteral], base: usize) -> bool {
    let start = base.saturating_add(span.0);
    let end = base.saturating_add(span.1);
    literals
        .iter()
        .filter(|literal| start <= literal.start && literal.start < end)
        .any(|literal| has_placeholder(&literal.content))
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::tracing::unstructured_tracing_hits, scan::support::test_file};

    fn hit_count(lines: &[&str]) -> usize {
        unstructured_tracing_hits(&[test_file("src/a.rs", lines)]).len()
    }

    #[test]
    fn structured_calls_silent() {
        for line in [
            "info!(item_id, \"Advancing to next item\",);",
            "warn!(error = %e, path = %config_path.display(), \"Failed\");",
            "error!(?err, \"Failed\");",
            "info!(%val, ?err, \"Mixed\");",
            "info!(target: \"net\", item_id, \"Tagged\");",
            "info!(count = 2, \"Counted\");",
            "tracing::info!(item_id, \"Qualified\");",
            "tracing::warn!(error = %e, \"Qualified\");",
            "info!(?err);",
            "info!(c = ')', \"Char\");",
            "info!(x, \"Braces {{kept}}\");",
            "info!(x, r#\"raw\"#);",
            "info!(error = %e, \"Failed \\u{2014} done\");",
        ] {
            assert_eq!(hit_count(&[line]), 0, "structured stays silent: {line}");
        }
    }

    #[test]
    fn other_crate_paths_silent() {
        for line in [
            "log::info!(\"hi {}\", x);",
            "log::warn!(\"static\");",
            "log::error!(error = %e, \"Failed {context}\");",
        ] {
            assert_eq!(hit_count(&[line]), 0, "other crates stay silent: {line}");
        }
    }

    #[test]
    fn multiline_standards_example_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "warn!(",
                "    error = %e,",
                "    path = %config_path.display(),",
                "    \"Failed to parse config, falling back to defaults\",",
                ");",
            ],
        )];
        assert!(
            unstructured_tracing_hits(&files).is_empty(),
            "standards example stays silent"
        );
    }

    #[test]
    fn comments_and_raw_silent() {
        for line in [
            "// info!(\"x {}\", y);",
            "/* warn!(\"x {}\", y); */",
            "let s = \"info!\";",
            "let q = r#\"info!(\"x {}\", y);\"#;",
        ] {
            assert_eq!(hit_count(&[line]), 0, "masked text stays silent: {line}");
        }
        let files = vec![test_file(
            "src/a.rs",
            &[
                "let q = r#\"",
                "info!(\"x {}\", y);",
                "\"#;",
                "info!(\"b {}\", a);",
            ],
        )];
        assert_eq!(
            unstructured_tracing_hits(&files).len(),
            1,
            "only the real call flags"
        );
    }

    #[test]
    fn interpolation_flagged() {
        for line in [
            "info!(\"Advancing to {}\", item_id);",
            "info!(\"Advancing to {item_id}\");",
            "error!(\"{}\", format!(\"nested\"));",
            "info!(format!(\"built\"));",
            "info!(x, \"value={x}\");",
            "tracing::warn!(\"fail: {e}\");",
            "info!(\"two\", \"strings\");",
        ] {
            assert_eq!(hit_count(&[line]), 1, "interpolation flags: {line}");
        }
        let mut debug_case = String::from("warn!(\"Failed: {e:?");
        debug_case.push_str("} at {path}\");");
        let files = vec![test_file("src/a.rs", &[debug_case.as_str()])];
        assert_eq!(
            unstructured_tracing_hits(&files).len(),
            1,
            "debug specifiers flag"
        );
    }

    #[test]
    fn multiline_interpolation_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "warn!(",
                "    \"Failed for {}\",",
                "    path.display(),",
                ");",
            ],
        )];
        assert_eq!(
            unstructured_tracing_hits(&files).len(),
            1,
            "multiline interpolation flags once"
        );
    }

    #[test]
    fn fieldless_flagged_with_hint() {
        let files = vec![test_file("src/a.rs", &["info!(\"started\");"])];
        let hits = unstructured_tracing_hits(&files);
        assert_eq!(hits.len(), 1, "field-less flags");
        assert!(
            hits.first().is_some_and(|hit| hit.contains("help:")),
            "field-less carries a fix hint"
        );
    }

    #[test]
    fn lookalikes_silent() {
        for line in [
            "trace!(\"x\");",
            "debug!(\"x\");",
            "event!(Level::INFO, \"x\");",
            "span!(Level::INFO, \"s\");",
            "#[warn(dead_code)]",
            "fn error() {}",
            "let info = 1;",
            "error_code!(\"x\");",
        ] {
            assert_eq!(hit_count(&[line]), 0, "lookalikes stay silent: {line}");
        }
    }
}
