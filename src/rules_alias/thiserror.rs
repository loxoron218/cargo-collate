//! Aliased `thiserror::Error` detection.
//!
//! Flags `use thiserror::Error as …`; on collision the other `Error` keeps
//! the alias instead.

use std::collections::BTreeSet;

use crate::{
    rules_alias::{
        alias_line,
        collect::{collect_use_stmts, use_body},
        format_hits,
        import_alias::{split_alias, strip_raw},
    },
    rules_self_import::expand::expand,
    scan::SourceFile,
};

/// Aliased `thiserror::Error` imports, one finding per alias line.
///
/// Plain `use thiserror::Error;` stays silent while any `as` alias flags,
/// including `as _` and grouped or `pub use` forms.
pub fn thiserror_alias(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file).collect()
}

/// Findings for one file.
fn check_file(file: &SourceFile) -> Vec<String> {
    let stmts = collect_use_stmts(&file.lines);
    if stmts.is_empty() {
        return Vec::new();
    }
    let mut flagged = BTreeSet::new();
    for (start, end, buf) in &stmts {
        collect_stmt_hits(&file.lines, *start, *end, buf, &mut flagged);
    }
    format_hits(file, flagged)
}

/// Insert alias lines for one `use` statement into `flagged`.
fn collect_stmt_hits(
    lines: &[String],
    start: usize,
    end: usize,
    buf: &str,
    flagged: &mut BTreeSet<usize>,
) {
    let Some(body) = use_body(buf) else {
        return;
    };
    for expanded in expand(&body) {
        let (path_part, alias_opt) = split_alias(&expanded);
        let Some(alias_raw) = alias_opt else {
            continue;
        };
        if !is_thiserror_error(&path_part) {
            continue;
        }
        let Some(alias) = clean_alias(&alias_raw) else {
            continue;
        };
        flagged.extend([alias_line(lines, start, end, &alias)]);
    }
}

/// True if `path` names exactly `thiserror::Error`.
fn is_thiserror_error(path: &str) -> bool {
    let trimmed = path.trim();
    let without_leading = trimmed.strip_prefix("::").unwrap_or(trimmed);
    let segs: Vec<&str> = without_leading
        .split("::")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if segs.len() != 2 {
        return false;
    }
    let crate_part = segs.first().map_or_default(|part| strip_raw(part));
    let item_part = segs.get(1).map_or_default(|part| strip_raw(part));
    crate_part == "thiserror" && item_part == "Error"
}

/// Cleaned alias name, if usable for line attribution.
fn clean_alias(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches([',', ';', '}']);
    let name = strip_raw(trimmed).trim().to_owned();
    if name.is_empty() || name.contains('*') || name.contains(':') {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use crate::{rules_alias::thiserror::thiserror_alias, scan::support::test_file};

    #[test]
    fn plain_alias_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["use thiserror::Error as ThisError;"],
        )];
        assert_eq!(thiserror_alias(&files).len(), 1);
    }

    #[test]
    fn grouped_pub_and_absolute_flagged() {
        let grouped = vec![test_file(
            "src/a.rs",
            &["use thiserror::{Error as ThisError};"],
        )];
        assert_eq!(thiserror_alias(&grouped).len(), 1);
        let multiline = vec![test_file(
            "src/a.rs",
            &["use thiserror::{", "    Error as ThisError,", "};"],
        )];
        let hits = thiserror_alias(&multiline);
        assert_eq!(hits.len(), 1);
        assert!(
            hits.first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:2:")),
            "multiline points at alias line"
        );
        let public = vec![test_file(
            "src/a.rs",
            &["pub use thiserror::Error as ThisError;"],
        )];
        assert_eq!(thiserror_alias(&public).len(), 1);
        let scoped = vec![test_file(
            "src/a.rs",
            &["pub(crate) use thiserror::Error as ThisError;"],
        )];
        assert_eq!(thiserror_alias(&scoped).len(), 1);
        let absolute = vec![test_file(
            "src/a.rs",
            &["use ::thiserror::Error as ThisError;"],
        )];
        assert_eq!(thiserror_alias(&absolute).len(), 1);
        let underscore = vec![test_file("src/a.rs", &["use thiserror::Error as _;"])];
        assert_eq!(thiserror_alias(&underscore).len(), 1);
        let consolidated = vec![test_file(
            "src/a.rs",
            &["use {", "    thiserror::Error as ThisError,", "};"],
        )];
        assert_eq!(thiserror_alias(&consolidated).len(), 1);
    }

    #[test]
    fn plain_and_other_errors_silent() {
        let plain = vec![test_file("src/a.rs", &["use thiserror::Error;"])];
        assert!(
            thiserror_alias(&plain).is_empty(),
            "plain thiserror stays silent"
        );
        let other = vec![test_file(
            "src/a.rs",
            &[
                "use thiserror::Error;",
                "use std::io::Error as IoError;",
                "use foo::Error as FooError;",
            ],
        )];
        assert!(
            thiserror_alias(&other).is_empty(),
            "aliased other Errors stay silent"
        );
        let similar = vec![test_file(
            "src/a.rs",
            &["use thiserror::ErrorKind as Kind;"],
        )];
        assert!(
            thiserror_alias(&similar).is_empty(),
            "similar names stay silent"
        );
        let glob = vec![test_file("src/a.rs", &["use thiserror::*;"])];
        assert!(thiserror_alias(&glob).is_empty(), "globs stay silent");
    }

    #[test]
    fn derive_and_qualified_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use thiserror::Error;",
                "#[derive(Debug, Error)]",
                "pub enum MyError {",
                "    #[error(\"oops\")]",
                "    Oops,",
                "}",
            ],
        )];
        assert!(thiserror_alias(&files).is_empty(), "derive stays silent");
        let qualified = vec![test_file(
            "src/a.rs",
            &["fn f() -> Result<(), thiserror::Error> {", "}"],
        )];
        assert!(
            thiserror_alias(&qualified).is_empty(),
            "qualified paths stay silent"
        );
    }

    #[test]
    fn wrong_collision_direction_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["use thiserror::Error as ThisError;", "use foo::Error;"],
        )];
        assert_eq!(thiserror_alias(&files).len(), 1);
    }
}
