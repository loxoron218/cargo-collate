//! Unnecessary type alias detection.
//!
//! Flags free-standing `type Alias = RHS;` items removable without tripping
//! `clippy::type_complexity`: the alias itself and every use site, with the
//! right-hand side substituted in, must score at or below the threshold.
//! Associated types inside `trait` or `impl` blocks stay exempt since they
//! are required by the item they belong to. Scoring mirrors the Clippy
//! visitor without type resolution; see the `score` submodule for the exact
//! table. This promises complexity-safety only: inlining can still surface
//! unrelated lints, which no syntactic check can foresee.

pub mod alias;
pub mod score;
pub mod threshold;

use crate::{
    rules_types::{
        alias::{discovery::collect_aliases, sites::is_unnecessary},
        threshold::complexity_threshold,
    },
    scan::SourceFile,
};

/// Unnecessary `type` aliases removable without tripping `type_complexity`.
///
/// Reports one finding per alias start line in `path:line:content` form,
/// covering private and public items alike.
#[must_use]
pub fn unnecessary_types(files: &[SourceFile]) -> Vec<String> {
    let limit = complexity_threshold();
    files
        .iter()
        .enumerate()
        .flat_map(|(fi, file)| file_unnecessary_types(files, fi, file, limit))
        .collect()
}

/// Findings for file `fi` at the active `limit`.
fn file_unnecessary_types(
    files: &[SourceFile],
    fi: usize,
    file: &SourceFile,
    limit: u64,
) -> Vec<String> {
    collect_aliases(&file.stripped)
        .into_iter()
        .filter(|alias| is_unnecessary(files, fi, alias, limit))
        .filter_map(|alias| {
            file.lines
                .get(alias.start)
                .map(|line| format!("{}:{}:{line}", file.path, alias.start.saturating_add(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_types::unnecessary_types, scan::support::test_file};

    #[test]
    fn simple_alias_flagged() {
        let files = vec![test_file("src/a.rs", &["type Foo = u32;"])];
        assert_eq!(unnecessary_types(&files).len(), 1);
    }

    #[test]
    fn vec_alias_flagged() {
        let files = vec![test_file("src/a.rs", &["type Short = Vec<String>;"])];
        assert_eq!(unnecessary_types(&files).len(), 1);
    }

    #[test]
    fn complex_alias_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "type Complex = HashMap<String, HashMap<String, HashMap<String, HashMap<String, \
                 Vec<u8>>>>>;",
            ],
        )];
        assert!(
            unnecessary_types(&files).is_empty(),
            "complex RHS stays silent"
        );
    }

    #[test]
    fn assoc_and_pub_shapes() {
        let assoc = vec![test_file(
            "src/a.rs",
            &["trait Api {", "    type Out = u32;", "}"],
        )];
        assert!(
            unnecessary_types(&assoc).is_empty(),
            "associated types stay silent"
        );
        let public = vec![test_file("src/b.rs", &["pub type Foo = u32;"])];
        assert_eq!(
            unnecessary_types(&public).len(),
            1,
            "public aliases still flag"
        );
    }

    #[test]
    fn nested_use_site_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "type PendingCovers = HashMap<i64, Vec<WeakRef<Picture>>>;",
                "fn f(pending: &Arc<Mutex<PendingCovers>>) {}",
            ],
        )];
        assert!(
            unnecessary_types(&files).is_empty(),
            "use pushing over stays silent"
        );
    }

    #[test]
    fn tuple_use_site_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "type MetaResult = (String, String, String, Option<String>, String, i64);",
                "fn f(tx: Sender<(i64, MetaResult)>) {}",
            ],
        )];
        assert!(
            unnecessary_types(&files).is_empty(),
            "tuple-wrapped use stays silent"
        );
    }

    #[test]
    fn bare_uses_still_flag() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "type UserId = i64;",
                "fn f(x: UserId) {}",
                "fn g() -> UserId { 1 }",
            ],
        )];
        assert_eq!(unnecessary_types(&files).len(), 1, "bare uses still flag");
    }

    #[test]
    fn cross_file_nested_use_silent() {
        let files = vec![
            test_file("src/a.rs", &["type Boxed = Vec<String>;"]),
            test_file(
                "src/b.rs",
                &[
                    "use crate::a::Boxed;",
                    "struct S {",
                    "    f: Arc<Mutex<Vec<Vec<Vec<Boxed>>>>>,",
                    "}",
                ],
            ),
        ];
        assert!(
            unnecessary_types(&files).is_empty(),
            "complex use in another file stays silent"
        );
    }

    #[test]
    fn unrelated_same_name_ignored() {
        let files = vec![
            test_file("src/a.rs", &["type Box<T> = alloc::boxed::Box<T>;"]),
            test_file(
                "src/b.rs",
                &["fn g(x: Box<Arc<Mutex<Vec<HashMap<String, Vec<u8>>>>>>>) {}"],
            ),
        ];
        assert_eq!(
            unnecessary_types(&files).len(),
            1,
            "import-less shadowing use never silences"
        );
    }

    #[test]
    fn use_line_mentions_skipped() {
        let files = vec![
            test_file("src/a.rs", &["type Foo = u32;"]),
            test_file("src/b.rs", &["use crate::a::Foo;"]),
        ];
        assert_eq!(
            unnecessary_types(&files).len(),
            1,
            "import lines never count as uses"
        );
    }
}
