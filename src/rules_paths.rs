//! Path rules: qualified call sites, enum variants, prelude groups.

pub mod prelude;

use std::collections::HashSet;

use crate::{
    lexer::{
        literal::strip_strings,
        marker::cut_line_comment,
        qualified::{enum_variant_hit, has_qualified_path},
        visibility::strip_pub_word,
        word::{boundary_after, has_word},
    },
    scan::SourceFile,
};

/// True if `code` (string-stripped) opens a `use` statement.
fn is_use_start(code: &str) -> bool {
    let rest = strip_pub_word(code.trim_start());
    rest.starts_with("use") && boundary_after(rest, 3)
}

/// Check one non-`use` line for a qualified path; returns the finding, if any.
fn qualified_line(path: &str, idx: usize, line: &str, code: &str) -> Option<String> {
    if line.trim_start().starts_with("//") || line.contains("$crate::") {
        return None;
    }
    has_qualified_path(cut_line_comment(code))
        .then(|| format!("{path}:{}:{line}", idx.saturating_add(1)))
}

/// Fully-qualified `foo::Bar` at call sites (constructors exempt).
pub fn qualified_paths(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_qualified).collect()
}

/// Visit each non-use line with its stripped code.
fn each_code_line(file: &SourceFile, mut visit: impl FnMut(usize, &str, &str)) {
    let mut in_use = false;
    for (idx, (line, code)) in file.lines.iter().zip(file.stripped.iter()).enumerate() {
        let blanked = strip_strings(code);
        if !in_use && is_use_start(&blanked) {
            in_use = !blanked.contains(';');
            continue;
        }
        if in_use {
            in_use = !blanked.contains(';');
            continue;
        }
        visit(idx, line, &blanked);
    }
}

/// Qualified-path findings for one file, skipping `use` statements.
fn check_file_qualified(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    each_code_line(file, |idx, line, code| {
        out.extend(qualified_line(&file.path, idx, line, code));
    });
    out
}

/// Enum name declared by one string-stripped line, if any.
fn enum_name(code: &str) -> Option<String> {
    let after = strip_pub_word(code.trim_start()).strip_prefix("enum")?;
    if !after.is_empty()
        && after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let name: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Collect enum names defined in `lines`.
fn local_enums(lines: &[String]) -> HashSet<String> {
    lines
        .iter()
        .filter_map(|line| enum_name(&strip_strings(line)))
        .collect()
}

/// Multi-line `use …;` statements in `lines` (string-stripped, joined).
fn use_statements(lines: &[String]) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut buf: Option<String> = None;
    for line in lines {
        let code = strip_strings(line);
        if buf.is_some() {
            flush_use_line(&mut buf, &mut stmts, &code);
            continue;
        }
        if !is_use_start(&code) {
            continue;
        }
        if code.contains(';') {
            stmts.push(code);
        } else {
            buf = Some(code);
        }
    }
    stmts
}

/// Append `code` to the pending `use` statement, flushing on `;`.
fn flush_use_line(buf: &mut Option<String>, stmts: &mut Vec<String>, code: &str) {
    if buf.is_none() {
        return;
    }
    let mut pending = buf.take().unwrap_or_default();
    pending.push('\n');
    pending.push_str(code);
    if code.contains(';') {
        stmts.push(pending);
    } else {
        *buf = Some(pending);
    }
}

/// True if the file imports `Key` from `gdk` (associated constants, not an enum).
fn uses_gdk_key(lines: &[String]) -> bool {
    use_statements(lines)
        .iter()
        .any(|s| has_word(s, "gdk") && has_word(s, "Key"))
}

/// Check one non-`use` line for an enum-variant path.
fn enum_line(
    path: &str,
    idx: usize,
    line: &str,
    code: &str,
    enums: &HashSet<String>,
    gdk_key: bool,
) -> Option<String> {
    if line.trim_start().starts_with("//") || line.contains("$crate::") {
        return None;
    }
    let left = enum_variant_hit(cut_line_comment(code))?;
    (!enums.contains(&left) && (left != "Key" || !gdk_key))
        .then(|| format!("{path}:{}:{line}", idx.saturating_add(1)))
}

/// `Type::Variant` at call sites; nested `use` imports expected instead.
pub fn enum_variants(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_enums).collect()
}

/// Enum-variant findings for one file, skipping `use` statements.
fn check_file_enums(file: &SourceFile) -> Vec<String> {
    let enums = local_enums(&file.stripped);
    let gdk_key = uses_gdk_key(&file.stripped);
    let mut out = Vec::new();
    each_code_line(file, |idx, line, code| {
        out.extend(enum_line(&file.path, idx, line, code, &enums, gdk_key));
    });
    out
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_paths::{enum_variants, qualified_paths},
        scan::support::test_file,
    };

    #[test]
    fn use_blocks_are_skipped() {
        let files = vec![test_file(
            "src/a.rs",
            &["use foo::{", "    Bar,", "};", "fn f() {}"],
        )];
        assert_eq!(qualified_paths(&files).len(), 0);
        assert_eq!(enum_variants(&files).len(), 0);
    }

    #[test]
    fn qualified_call_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["fn f() {", "    let x = foo::Bar;", "}"],
        )];
        assert_eq!(qualified_paths(&files).len(), 1);
    }

    #[test]
    fn local_enum_stays_qualified() {
        let files = vec![test_file(
            "src/a.rs",
            &["pub enum Error {", "    Io,", "}", "fn f() -> Error::Io {"],
        )];
        assert_eq!(enum_variants(&files).len(), 0);
    }
}
