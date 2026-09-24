//! Unnecessary import alias detection.
//!
//! Flags `use … as …` where the alias is not needed to resolve a name
//! conflict within the same file.

pub mod collect;
pub mod import_alias;
pub mod local_scope;
pub mod thiserror;

use std::collections::BTreeSet;

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    rules_alias::{
        collect::{collect_imports, collect_use_stmts},
        import_alias::is_unnecessary_alias,
        local_scope::{collect_locals, ident_at, prev_is_gap, skip_whitespace},
    },
    scan::SourceFile,
};

/// Unnecessary `as` aliases with no name conflict in the same file.
///
/// Reports one finding per aliased line, including `pub use` and `as _`.
pub fn unnecessary_aliases(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_alias).collect()
}

/// Findings for one file.
fn check_file_alias(file: &SourceFile) -> Vec<String> {
    let stmts = collect_use_stmts(&file.lines);
    if stmts.is_empty() {
        return Vec::new();
    }
    let (binders, aliased) = collect_imports(&stmts);
    if aliased.is_empty() {
        return Vec::new();
    }
    mark_flagged(file, &stmts, &aliased, &binders)
}

/// Findings for flagged statement indices.
fn mark_flagged(
    file: &SourceFile,
    stmts: &[(usize, usize, String)],
    aliased: &[(String, String, usize)],
    binders: &BTreeSet<String>,
) -> Vec<String> {
    let locals = collect_locals(&file.stripped);
    let mut flagged = BTreeSet::new();
    for (orig, alias, stmt_idx) in aliased {
        if !is_unnecessary_alias(orig, alias, binders, &locals) {
            continue;
        }
        let Some((start, end, _)) = stmts.get(*stmt_idx) else {
            continue;
        };
        flagged.extend([alias_line(&file.lines, *start, *end, alias)]);
    }
    format_hits(file, flagged)
}

/// Format flagged line numbers as path hits.
#[must_use]
pub fn format_hits(file: &SourceFile, flagged: BTreeSet<usize>) -> Vec<String> {
    flagged
        .into_iter()
        .filter_map(|line_no| {
            file.lines
                .get(line_no.saturating_sub(1))
                .map(|line| format!("{}:{line_no}:{line}", file.path))
        })
        .collect()
}

/// One-based line holding `as alias` within a statement range, if any.
#[must_use]
pub fn alias_line(lines: &[String], start: usize, end: usize, alias: &str) -> usize {
    let mut line_no = start;
    while line_no <= end {
        if let Some(line) = lines.get(line_no.saturating_sub(1))
            && line_has_alias(line, alias)
        {
            return line_no;
        }
        line_no = line_no.saturating_add(1);
    }
    start
}

/// True if a source line contains `as alias` outside strings and comments.
fn line_has_alias(line: &str, alias: &str) -> bool {
    let stripped = strip_strings(line);
    let code = cut_line_comment(&stripped);
    has_alias_word(code, alias)
}

/// True if `code` holds `as alias` with identifier boundaries.
fn has_alias_word(code: &str, alias: &str) -> bool {
    let mut search = code;
    let mut offset: usize = 0;
    while let Some(rel) = search.find("as") {
        let pos = offset.saturating_add(rel);
        if is_alias_at(code, pos, alias) {
            return true;
        }
        search = search.get(rel.saturating_add(2)..).unwrap_or_default();
        offset = pos.saturating_add(2);
    }
    false
}

/// True if `as alias` opens at byte `pos` with boundaries on both sides.
fn is_alias_at(code: &str, pos: usize, alias: &str) -> bool {
    if !prev_is_gap(code, pos) {
        return false;
    }
    let mut next = pos.saturating_add(2);
    if code
        .as_bytes()
        .get(next)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return false;
    }
    next = skip_whitespace(code, next);
    let stripped = alias.strip_prefix("r#").unwrap_or(alias);
    let Some(name) = ident_at(code, next) else {
        return false;
    };
    if name != stripped {
        return false;
    }
    let raw_len = if code.get(next..next.saturating_add(2)) == Some("r#") {
        name.len().saturating_add(2)
    } else {
        name.len()
    };
    let name_end = next.saturating_add(raw_len);
    code.as_bytes()
        .get(name_end)
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

#[cfg(test)]
mod tests {
    use crate::{rules_alias::unnecessary_aliases, scan::support::test_file};

    #[test]
    fn plain_alias_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["use crate::foo::Bar as Baz;", "fn f() {}"],
        )];
        assert_eq!(unnecessary_aliases(&files).len(), 1);
    }

    #[test]
    fn self_and_underscore_flagged() {
        let files = vec![test_file("src/a.rs", &["use crate::foo::Bar as Bar;"])];
        assert_eq!(unnecessary_aliases(&files).len(), 1);
        let underscore = vec![test_file("src/a.rs", &["use crate::foo::Bar as _;"])];
        assert_eq!(unnecessary_aliases(&underscore).len(), 1);
    }

    #[test]
    fn import_collision_allowed() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use crate::a::Foo;",
                "use crate::b::Foo as Bar;",
                "fn f() {}",
            ],
        )];
        assert!(
            unnecessary_aliases(&files).is_empty(),
            "colliding original stays silent"
        );
    }

    #[test]
    fn local_collision_allowed() {
        let files = vec![test_file(
            "src/a.rs",
            &["use crate::foo::Bar as Baz;", "pub struct Bar;"],
        )];
        assert!(
            unnecessary_aliases(&files).is_empty(),
            "local original stays silent"
        );
    }

    #[test]
    fn grouped_and_pub_flagged() {
        let grouped = vec![test_file(
            "src/a.rs",
            &["use crate::foo::{Bar as Baz, Qux};", "fn f() {}"],
        )];
        assert_eq!(unnecessary_aliases(&grouped).len(), 1);
        let multiline = vec![test_file(
            "src/a.rs",
            &["use crate::foo::{", "    Bar as Baz,", "};"],
        )];
        assert_eq!(unnecessary_aliases(&multiline).len(), 1);
        assert!(
            unnecessary_aliases(&multiline)
                .iter()
                .all(|hit| hit.starts_with("src/a.rs:2:")),
            "multiline points at alias line"
        );
        let public = vec![test_file("src/a.rs", &["pub use crate::foo::Bar as Baz;"])];
        assert_eq!(unnecessary_aliases(&public).len(), 1);
        let scoped = vec![test_file(
            "src/a.rs",
            &["pub(crate) use crate::foo::Bar as Baz;"],
        )];
        assert_eq!(unnecessary_aliases(&scoped).len(), 1);
    }

    #[test]
    fn clean_imports_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &["use crate::foo::Bar;", "fn f() {}"],
        )];
        assert!(
            unnecessary_aliases(&files).is_empty(),
            "plain imports stay silent"
        );
    }

    #[test]
    fn prelude_collision_allowed() {
        let gtk_box = vec![test_file("src/a.rs", &["use gtk::Box as GtkBox;"])];
        assert!(
            unnecessary_aliases(&gtk_box).is_empty(),
            "prelude Box stays silent"
        );
        let fmt_result = vec![test_file(
            "src/a.rs",
            &["use std::fmt::Result as FmtResult;"],
        )];
        assert!(
            unnecessary_aliases(&fmt_result).is_empty(),
            "prelude Result stays silent"
        );
        let std_box = vec![test_file("src/a.rs", &["use std::boxed::Box as StdBox;"])];
        assert!(
            unnecessary_aliases(&std_box).is_empty(),
            "prelude Box stays silent"
        );
    }
}
