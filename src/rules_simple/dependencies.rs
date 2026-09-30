//! Crate-dependency checks for `default-features` opt-outs.
//!
//! Every dependency entry must carry `default-features = false` with only
//! required features enabled. Pure `workspace = true` uses stay silent
//! while their `[workspace.dependencies]` definition decides.

pub mod quotes;
pub mod toml;

use crate::{
    rules_simple::dependencies::toml::{
        brace_delta, dep_table_name, find_equals, has_default_false, is_dep_list,
        is_pure_workspace_block, is_pure_workspace_inline, section_inner, strip_comment,
    },
    scan::ManifestFile,
};

/// Pending multiline entry awaiting balanced braces.
struct Pending {
    /// Zero-based index where the entry starts.
    start: usize,
    /// Dependency name for reporting.
    name: String,
    /// Accumulated value text across continuation lines.
    buffer: String,
    /// Open minus close brace and bracket depth.
    depth: i32,
}

/// Open `[dependencies.name]` table awaiting its closing header.
struct TableBlock {
    /// Zero-based index of the header line.
    start: usize,
    /// Comment-stripped body lines.
    lines: Vec<String>,
}

/// Dependencies without `default-features = false`; add the opt-out with only required features.
#[must_use]
pub fn default_features_hits(manifests: &[ManifestFile]) -> Vec<String> {
    let mut out = Vec::new();
    for manifest in manifests {
        out.extend(check_manifest(manifest));
    }
    out
}

/// Findings for one manifest across all dependency tables.
fn check_manifest(manifest: &ManifestFile) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_list = false;
    let mut table: Option<TableBlock> = None;
    let mut pending: Option<Pending> = None;
    for (idx, raw) in manifest.lines.iter().enumerate() {
        let code = strip_comment(raw);
        let header = section_inner(code.trim());
        if header.is_some() {
            flush_pending(manifest, &mut pending, &mut out);
            flush_table(manifest, &mut table, &mut out);
        }
        if let Some(inner) = header {
            open_section(idx, inner, &mut in_list, &mut table);
        }
        if header.is_none() && table.is_some() {
            push_body_line(&mut table, code);
        }
        if header.is_none() && table.is_none() && in_list {
            step_entry(manifest, idx, code, &mut pending, &mut out);
        }
        if header.is_none() && table.is_none() && !in_list {
            flush_pending(manifest, &mut pending, &mut out);
        }
    }
    flush_pending(manifest, &mut pending, &mut out);
    flush_table(manifest, &mut table, &mut out);
    out
}

/// Track one section header for dependency parsing.
fn open_section(idx: usize, inner: &str, in_list: &mut bool, table: &mut Option<TableBlock>) {
    let is_table = dep_table_name(inner).is_some();
    if is_table {
        *table = Some(TableBlock {
            start: idx,
            lines: Vec::new(),
        });
    }
    *in_list = !is_table && is_dep_list(inner);
}

/// Store one table-body line, if meaningful.
fn push_body_line(table: &mut Option<TableBlock>, code: &str) {
    if code.trim().is_empty() {
        return;
    }
    if let Some(block) = table.as_mut() {
        block.lines.push(code.to_owned());
    }
}

/// Advance list-section parsing for one code line.
fn step_entry(
    manifest: &ManifestFile,
    idx: usize,
    code: &str,
    pending: &mut Option<Pending>,
    out: &mut Vec<String>,
) {
    if pending.is_some() {
        step_pending(manifest, code, pending, out);
        return;
    }
    if code.trim().is_empty() {
        return;
    }
    let Some(eq) = find_equals(code) else {
        return;
    };
    let left = code.get(..eq).map_or("", str::trim);
    let right = code.get(eq.saturating_add(1)..).unwrap_or("");
    let name = dep_name(left);
    if name.is_empty() {
        return;
    }
    start_entry(manifest, idx, &name, right, pending, out);
}

/// Start one entry from its value text, buffering splits across lines.
fn start_entry(
    manifest: &ManifestFile,
    idx: usize,
    name: &str,
    right: &str,
    pending: &mut Option<Pending>,
    out: &mut Vec<String>,
) {
    let depth = brace_delta(right);
    if depth > 0 {
        *pending = Some(Pending {
            start: idx,
            name: name.to_owned(),
            buffer: right.to_owned(),
            depth,
        });
        return;
    }
    eval_inline(manifest, idx, name, right, out);
}

/// Advance a pending multiline entry with one continuation line.
fn step_pending(
    manifest: &ManifestFile,
    code: &str,
    pending: &mut Option<Pending>,
    out: &mut Vec<String>,
) {
    let Some(entry) = pending.take() else {
        return;
    };
    let mut buffer = entry.buffer;
    buffer.push('\n');
    buffer.push_str(code);
    let depth = entry.depth.saturating_add(brace_delta(code));
    if depth <= 0 {
        eval_inline(manifest, entry.start, &entry.name, &buffer, out);
        return;
    }
    *pending = Some(Pending {
        start: entry.start,
        name: entry.name,
        buffer,
        depth,
    });
}

/// Dependency name from the left side of `=`, without quotes or dotted suffixes.
fn dep_name(left: &str) -> String {
    let head = left.split('.').next().unwrap_or("");
    head.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_owned()
}

/// True if an inline entry carries the opt-out or inherits purely.
fn inline_is_clean(name: &str, trimmed: &str) -> bool {
    if name.contains('.') {
        return dotted_is_clean(name, trimmed);
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return false;
    }
    if trimmed.starts_with('{') {
        return is_pure_workspace_inline(trimmed) || has_default_false(trimmed);
    }
    has_default_false(trimmed)
}

/// True if a dotted `name.key = value` line needs no further opt-out.
fn dotted_is_clean(name: &str, trimmed: &str) -> bool {
    let key = name.rsplit('.').next().unwrap_or("");
    if key == "workspace" && trimmed == "true" {
        return true;
    }
    if (key == "default-features" || key == "default_features") && trimmed == "false" {
        return true;
    }
    false
}

/// Evaluate one inline value for the opt-out.
fn eval_inline(
    manifest: &ManifestFile,
    start: usize,
    name: &str,
    value: &str,
    out: &mut Vec<String>,
) {
    if !inline_is_clean(name, value.trim()) {
        push_hit(manifest, start, out);
    }
}

/// Evaluate one `[dependencies.name]` body for the opt-out.
fn eval_table(manifest: &ManifestFile, start: usize, lines: &[String], out: &mut Vec<String>) {
    let joined = lines.join("\n");
    if is_pure_workspace_block(lines) {
        return;
    }
    if has_default_false(&joined) {
        return;
    }
    push_hit(manifest, start, out);
}

/// Evaluate a pending multiline entry, if any.
fn flush_pending(manifest: &ManifestFile, pending: &mut Option<Pending>, out: &mut Vec<String>) {
    if let Some(entry) = pending.take() {
        eval_inline(manifest, entry.start, &entry.name, &entry.buffer, out);
    }
}

/// Evaluate an open dependency table, if any.
fn flush_table(manifest: &ManifestFile, table: &mut Option<TableBlock>, out: &mut Vec<String>) {
    if let Some(block) = table.take() {
        eval_table(manifest, block.start, &block.lines, out);
    }
}

/// Push one `path:line:content` finding for `idx`.
fn push_hit(manifest: &ManifestFile, idx: usize, out: &mut Vec<String>) {
    if let Some(line) = manifest.lines.get(idx) {
        out.push(format!(
            "{}:{}:{line}",
            manifest.path,
            idx.saturating_add(1)
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::dependencies::default_features_hits, scan::support::test_manifest};

    #[test]
    fn string_and_inline_shapes() {
        let flagged = vec![test_manifest(
            "Cargo.toml",
            &["[dependencies]", "serde = \"1\""],
        )];
        assert_eq!(
            default_features_hits(&flagged),
            vec!["Cargo.toml:2:serde = \"1\"".to_owned()]
        );
        let clean = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dependencies]",
                "serde = { version = \"1\", default-features = false }",
                "tokio = { version = \"1\", features = [\"rt\"], default-features = false }",
            ],
        )];
        assert!(
            default_features_hits(&clean).is_empty(),
            "opt-outs stay silent"
        );
        let missing = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dependencies]",
                "serde = { version = \"1\" }",
                "tokio = { version = \"1\", default-features = true }",
            ],
        )];
        assert_eq!(default_features_hits(&missing).len(), 2, "tables flag");
    }

    #[test]
    fn all_sections_covered() {
        let files = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dev-dependencies]",
                "tempfile = \"3\"",
                "[build-dependencies]",
                "cc = { version = \"1\" }",
                "[target.'cfg(unix)'.dependencies]",
                "nix = \"0.29\"",
                "[workspace.dependencies]",
                "serde = \"1\"",
            ],
        )];
        assert_eq!(default_features_hits(&files).len(), 4, "every table flags");
        let clean = vec![test_manifest(
            "Cargo.toml",
            &["[package]", "name = \"demo\"", "[features]", "full = []"],
        )];
        assert!(
            default_features_hits(&clean).is_empty(),
            "non-dependency sections stay silent"
        );
    }

    #[test]
    fn workspace_inheritance() {
        let pure = vec![test_manifest(
            "member/Cargo.toml",
            &["[dependencies]", "serde = { workspace = true }"],
        )];
        assert!(
            default_features_hits(&pure).is_empty(),
            "pure inheritance stays silent"
        );
        let overriding = vec![test_manifest(
            "member/Cargo.toml",
            &[
                "[dependencies]",
                "serde = { workspace = true, features = [\"derive\"] }",
            ],
        )];
        assert_eq!(
            default_features_hits(&overriding).len(),
            1,
            "overrides repeat the opt-out"
        );
    }

    #[test]
    fn table_and_multiline_shapes() {
        let table = vec![test_manifest(
            "Cargo.toml",
            &["[dependencies.serde]", "version = \"1\""],
        )];
        assert_eq!(default_features_hits(&table).len(), 1, "tables flag");
        let clean_table = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dependencies.serde]",
                "version = \"1\"",
                "default-features = false",
            ],
        )];
        assert!(
            default_features_hits(&clean_table).is_empty(),
            "table opt-outs stay silent"
        );
        let split = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dependencies]",
                "serde = { version = \"1\",",
                "  features = [\"derive\"] }",
            ],
        )];
        assert_eq!(
            default_features_hits(&split).len(),
            1,
            "split tables flag once"
        );
    }

    #[test]
    fn comments_and_strings_silent() {
        let files = vec![test_manifest(
            "Cargo.toml",
            &[
                "[dependencies]",
                "# default-features = false",
                "serde = { version = \"1\", default-features = false } # done",
            ],
        )];
        assert!(
            default_features_hits(&files).is_empty(),
            "comments never count as hits"
        );
    }
}
