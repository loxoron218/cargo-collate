//! Regression tests for unnecessary type aliases.

use crate::{
    rules_types::alias::{
        discovery::{TypeAlias, collect_aliases},
        modules::{imports_module, module_of, std_imported_roots},
        sites::is_unnecessary,
    },
    scan::{SourceFile, support::test_file},
};

fn flags_single(path: &str, lines: &[&str], name: &str, flag: bool) -> bool {
    let def = test_file(path, lines);
    let found = collect_aliases(&def.lines);
    let files = vec![def];
    found
        .into_iter()
        .next()
        .is_some_and(|alias| alias.name == name && is_unnecessary(&files, 0, &alias, 250) == flag)
}

fn modules(lines: &[&str]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

fn flagged_names(files: &[SourceFile], aliases: &[TypeAlias]) -> Vec<String> {
    aliases
        .iter()
        .filter(|alias| is_unnecessary(files, 0, alias, 250))
        .map(|alias| alias.name.clone())
        .collect()
}

#[test]
fn elided_lifetime_use_releases_alias() {
    assert!(
        flags_single(
            "src/a.rs",
            &["type Bar<'a> = &'a u8;", "fn f(x: Bar) {}"],
            "Bar",
            true
        ),
        "elided lifetimes fill with borrow placeholders"
    );
}

#[test]
fn dual_elided_lifetimes_release_alias() {
    assert!(
        flags_single(
            "src/a.rs",
            &["type T<'a, 'e> = HashMap<S<'a, 'e>, ()>;", "fn f(m: &T) {}"],
            "T",
            true
        ),
        "each elided lifetime fills independently"
    );
}

#[test]
fn std_qualified_mention_ignored() {
    assert!(
        flags_single(
            "src/a.rs",
            &[
                "use std::fmt;",
                "type Result<T> = std::result::Result<T, Error>;",
                "fn f(x: Result<u8>) {}",
                "fn g() -> fmt::Result { todo!() }",
            ],
            "Result",
            true
        ),
        "import-evidenced foreign paths never inform the verdict"
    );
}

#[test]
fn invariant_template_use_releases_alias() {
    assert!(
        flags_single(
            "src/a.rs",
            &[
                "type Token = u32;",
                "macro_rules! wrap {",
                "    () => { struct W { field: Token } }",
                "}",
                "wrap!();",
                "fn f(t: Token) {}",
            ],
            "Token",
            true
        ),
        "interpolation-free template fragments score"
    );
}

#[test]
fn matcher_variable_template_keeps_alias() {
    assert!(
        flags_single(
            "src/a.rs",
            &[
                "type Token = u32;",
                "macro_rules! wrap {",
                "    ($t:ty) => { fn f(x: $t, y: Token) {} },",
                "}",
                "wrap!();",
            ],
            "Token",
            false
        ),
        "matcher variables keep the veto"
    );
}

#[test]
fn unused_generic_assoc_alias_releases() {
    assert!(
        flags_single(
            "src/a.rs",
            &[
                "type TypeAlias<T> = HashMap<(), <T as Trait>::Assoc>;",
                "fn main() {}",
            ],
            "TypeAlias",
            true
        ),
        "unused aliases never trip any use site"
    );
}

#[test]
fn invocation_use_keeps_alias() {
    assert!(
        flags_single(
            "src/a.rs",
            &[
                "type Token = u32;",
                "macro_rules! wrap {",
                "    ($t:ty) => { struct W; },",
                "}",
                "wrap!(Token);",
                "fn f(t: Token) {}",
            ],
            "Token",
            false
        ),
        "invocation interiors never resolve"
    );
}

#[test]
fn modules_derive_from_paths() {
    assert_eq!(
        module_of("src/a/b.rs"),
        vec!["a".to_owned(), "b".to_owned()]
    );
    assert_eq!(module_of("src/a.rs"), vec!["a".to_owned()]);
    assert_eq!(module_of("src/a/mod.rs"), vec!["a".to_owned()]);
    assert!(module_of("src/lib.rs").is_empty(), "roots stay empty");
    assert!(module_of("src/main.rs").is_empty(), "roots stay empty");
    assert_eq!(
        module_of("member-a/src/testing/fixtures.rs"),
        vec!["testing".to_owned(), "fixtures".to_owned()],
        "package dirs strip"
    );
    assert_eq!(
        module_of("member-a/tests/x.rs"),
        vec!["tests".to_owned(), "x".to_owned()],
        "test leaves namespace"
    );
}

#[test]
fn imports_resolve_against_modules() {
    let home = vec!["testing".to_owned(), "fixtures".to_owned()];
    let inner = vec!["testing".to_owned(), "events".to_owned()];
    let lines = modules(&["use crate::testing::fixtures::{Value};"]);
    assert!(imports_module(&lines, &inner, &home, "Value", "pkg"));
    let other = modules(&["use crate::value::Value;"]);
    assert!(
        !imports_module(&other, &inner, &home, "Value", "pkg"),
        "same-name other modules stay out"
    );
    let renamed = modules(&["use crate::testing::fixtures::Value as V;"]);
    assert!(
        !imports_module(&renamed, &inner, &home, "Value", "pkg"),
        "renames bind the new name"
    );
    let glob = modules(&["use crate::testing::fixtures::*;"]);
    assert!(imports_module(&glob, &inner, &home, "Value", "pkg"));
    let rooted = modules(&["use crate::Value;"]);
    assert!(
        imports_module(&rooted, &inner, &home, "Value", "pkg"),
        "root children count leniently"
    );
}

#[test]
fn relative_roots_resolve() {
    let home = vec!["player".to_owned(), "sidebar".to_owned()];
    let inner = vec!["player".to_owned(), "events".to_owned()];
    let above = modules(&["use super::sidebar::{MetaResult};"]);
    assert!(imports_module(&above, &inner, &home, "MetaResult", "pkg"));
    let core_home = vec!["value".to_owned()];
    let own = modules(&["use sqlx_core::value::Value;"]);
    assert!(
        imports_module(&own, &inner, &core_home, "Value", "sqlx_core"),
        "own-crate segments re-root"
    );
    assert!(
        !imports_module(&own, &inner, &home, "Value", "sqlx_core"),
        "resolved modules must match"
    );
    assert!(
        !imports_module(&own, &inner, &core_home, "Value", "pkg"),
        "foreign segments stay out"
    );
}

#[test]
fn multiline_import_resolves_through_module() {
    let home = vec!["player".to_owned(), "sidebar".to_owned()];
    let inner = vec!["player".to_owned(), "events".to_owned()];
    let lines = modules(&[
        "use crate::{",
        "    player::{",
        "        sidebar::{MetaResult, format_time},",
        "    },",
        "};",
    ]);
    assert!(imports_module(&lines, &inner, &home, "MetaResult", "pkg"));
    assert!(!imports_module(&lines, &inner, &home, "Other", "pkg"));
}

#[test]
fn same_name_sibling_modules_resolve_apart() {
    let config = test_file(
        "member/src/config/macros.rs",
        &["pub type TableName = Box<str>;", "fn f(t: TableName) {}"],
    );
    let fixtures = test_file(
        "member/src/testing/fixtures.rs",
        &[
            "type TableName = Arc<str>;",
            "fn g(t: TableName) {}",
            "fn h(t: &mut HashMap<TableName, usize>) {}",
        ],
    );
    let home = collect_aliases(&config.lines);
    let files = vec![config, fixtures];
    assert!(
        home.first().is_some_and(|alias| {
            alias.name == "TableName" && is_unnecessary(&files, 0, alias, 250)
        }),
        "sibling modules never share unqualified names"
    );
    let guest = files
        .get(1)
        .map_or_else(Vec::new, |file| collect_aliases(&file.lines));
    assert!(
        guest.first().is_some_and(|alias| {
            alias.name == "TableName" && is_unnecessary(&files, 1, alias, 250)
        }),
        "hasher trips fix without a new alias, so the guest flags too"
    );
}

#[test]
fn std_roots_come_from_direct_children() {
    let plain = modules(&["use std::fmt;"]);
    assert_eq!(std_imported_roots(&plain), vec!["fmt".to_owned()]);
    let renamed = modules(&["use std::fmt as f;"]);
    assert_eq!(
        std_imported_roots(&renamed),
        vec!["f".to_owned()],
        "renames bind the new root"
    );
    let grouped = modules(&["use std::{fmt, io};"]);
    assert_eq!(
        std_imported_roots(&grouped),
        vec!["fmt".to_owned(), "io".to_owned()]
    );
    let deep = modules(&["use std::fmt::Debug;"]);
    assert!(
        std_imported_roots(&deep).is_empty(),
        "deeper leaves bind their own name"
    );
    let local = modules(&["use crate::fmt;"]);
    assert!(
        std_imported_roots(&local).is_empty(),
        "crate roots may resolve home"
    );
    let shadowed = modules(&["use std::fmt;", "mod fmt {}"]);
    assert!(
        std_imported_roots(&shadowed).is_empty(),
        "shadowed roots may resolve locally"
    );
}

#[test]
fn value_constructions_skipped() {
    let def = test_file(
        "src/a.rs",
        &[
            "type P = Vec<WeakRef<Picture>>;",
            "fn f(state: &AppState) -> Result<u32, Error> {",
            "    Ok(P(state.map.clone()))",
            "}",
            "fn g(d: P) {}",
        ],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def];
    assert_eq!(
        flagged_names(&files, &aliases),
        vec!["P".to_owned()],
        "constructor calls never inform the verdict"
    );
}

#[test]
fn shadowing_definitions_skipped() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Processor = u32;",
            "type P = HashMap<String, HashMap<String, Vec<u8>>>;",
            "pub struct Processor<G> {}",
            "impl<G> Processor<G> {}",
            "fn f(x: Processor) {}",
            "fn g(x: impl Mutex<Vec<P>>) {}",
        ],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def];
    assert_eq!(
        flagged_names(&files, &aliases),
        vec!["Processor".to_owned()],
        "shadowing definitions never silence, complex bound uses keep"
    );
}

#[test]
fn cfg_duplicate_defs_flag() {
    let def = test_file(
        "src/a.rs",
        &[
            "#[cfg(not(feature = \"std\"))]",
            "type Box<T> = alloc::boxed::Box<T>;",
            "#[cfg(feature = \"std\")]",
            "type Box<T> = std::boxed::Box<T>;",
            "fn f(x: Box<u8>) {}",
        ],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def];
    assert_eq!(
        flagged_names(&files, &aliases),
        vec!["Box".to_owned(), "Box".to_owned()],
        "cfg duplicates flag once each"
    );
}
