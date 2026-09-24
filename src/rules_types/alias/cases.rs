//! Capability tests for unnecessary type aliases.

use crate::{
    rules_types::alias::{discovery::collect_aliases, sites::is_unnecessary},
    scan::{SourceFile, support::test_file},
};

fn ws_file(path: &str, lines: &[&str], ws: &str) -> SourceFile {
    let mut file = test_file(path, lines);
    file.ws = ws.to_owned();
    file
}

fn check_solo(def: SourceFile, name: &str, flag: bool, message: &str) {
    check_first(&[def], name, flag, message);
}

fn check_first(files: &[SourceFile], name: &str, flag: bool, message: &str) {
    let aliases = files
        .first()
        .map_or_else(Vec::new, |file| collect_aliases(&file.lines));
    assert!(
            aliases.first().is_some_and(
                |alias| alias.name == name && is_unnecessary(files, 0, alias, 250) == flag
            ),
            "{message}"
        );
}

#[test]
fn nested_use_keeps_alias() {
    let def = test_file(
        "src/a.rs",
        &[
            "type PendingCovers = HashMap<i64, Vec<WeakRef<Picture>>>;",
            "fn f(pending: &Arc<Mutex<PendingCovers>>) {}",
        ],
    );
    check_solo(
        def,
        "PendingCovers",
        false,
        "nested complex use keeps the alias",
    );
}

#[test]
fn bare_use_releases_alias() {
    let def = test_file("src/a.rs", &["type UserId = i64;", "fn f(x: UserId) {}"]);
    check_solo(def, "UserId", true, "bare use releases the alias");
}

#[test]
fn multiline_use_import_keeps_alias() {
    let def = test_file(
        "src/player/sidebar.rs",
        &[
            "pub type MetaResult = (String, String, String, Option<String>, String, i64);",
            "fn f(meta: MetaResult) {}",
        ],
    );
    let user = test_file(
        "src/player/events.rs",
        &[
            "use crate::{",
            "    player::{",
            "        sidebar::{MetaResult, format_time},",
            "    },",
            "};",
            "fn g(meta_tx: &Sender<(i64, MetaResult)>) {}",
        ],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def, user];
    assert!(
        aliases.first().is_some_and(|alias| {
            alias.name == "MetaResult" && !is_unnecessary(&files, 0, alias, 250)
        }),
        "complex use behind a multi-line import keeps the alias"
    );
}

#[test]
fn generic_params_substitute_at_use() {
    let def = test_file(
        "src/a.rs",
        &[
            "type M<K, V> = (Vec<Vec<K>>, Vec<Vec<V>>, u8, u8);",
            "fn f(m: M<String, u8>) {}",
        ],
    );
    check_solo(def, "M", true, "applied args substitute past the splice");
}

#[test]
fn generic_complex_use_keeps_alias() {
    let def = test_file(
        "src/a.rs",
        &[
            "type M<K, V> = HashMap<K, HashMap<String, HashMap<String, Vec<V>>>>;",
            "fn f(m: Arc<Mutex<M<String, u8>>>) {}",
        ],
    );
    check_solo(
        def,
        "M",
        false,
        "substituted wrapper scoring over keeps the alias",
    );
}

#[test]
fn generic_default_fills_bare_use() {
    let def = test_file(
        "src/a.rs",
        &["type Foo<T = u32> = Vec<T>;", "fn f(x: Foo) {}"],
    );
    check_solo(def, "Foo", true, "defaulted params fill bare mentions");
}

#[test]
fn macro_use_keeps_alias() {
    let def = test_file(
        "src/a.rs",
        &["type Token = u32;", "fn f() -> Vec<Token> { vec![Token] }"],
    );
    check_solo(def, "Token", false, "macro interiors veto the verdict");
}

#[test]
fn cross_file_macro_mention_skipped() {
    let def = test_file("src/a.rs", &["type Token = u32;", "fn f(t: Token) {}"]);
    let other = test_file(
        "src/b.rs",
        &["use crate::a::Token;", "fn g() -> Vec<u32> { vec![Token] }"],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def, other];
    assert!(
        aliases.first().is_some_and(|alias| {
            alias.name == "Token" && is_unnecessary(&files, 0, alias, 250)
        }),
        "foreign macro mentions cannot veto a verifiable verdict"
    );
}

#[test]
fn negated_block_use_flags() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Token = u32;",
            "fn f(t: Token, ready: bool) {",
            "    if !ready {",
            "        let x: Token = t;",
            "    }",
            "}",
        ],
    );
    check_solo(def, "Token", true, "negation blocks never mask real uses");
}

#[test]
fn same_workspace_member_use_keeps_alias() {
    let def = ws_file(
        "member-a/src/a.rs",
        &[
            "pub type MetaResult = (String, String, String, Option<String>, String, i64);",
            "fn f(meta: MetaResult) {}",
        ],
        "/w",
    );
    let other = ws_file(
        "member-b/src/b.rs",
        &[
            "use member_a::MetaResult;",
            "fn g(meta_tx: &Sender<(i64, MetaResult)>) {}",
        ],
        "/w",
    );
    check_first(
        &[def, other],
        "MetaResult",
        false,
        "complex use through a same-workspace re-export keeps the alias",
    );
}

#[test]
fn same_workspace_simple_use_flags() {
    let def = ws_file("member-a/src/a.rs", &["pub type Token = u32;"], "/w");
    let other = ws_file(
        "member-b/src/b.rs",
        &["use member_a::Token;", "fn g(t: Token) {}"],
        "/w",
    );
    check_first(
        &[def, other],
        "Token",
        true,
        "simple cross-member uses never silence a flag",
    );
}

#[test]
fn foreign_workspace_still_ignored() {
    let def = ws_file("member-a/src/a.rs", &["pub type Token = u32;"], "/w1");
    let other = ws_file(
        "member-b/src/b.rs",
        &[
            "use member_a::Token;",
            "fn g(tx: &Arc<Mutex<HashMap<String, HashMap<String, Vec<Token>>>>>>) {}",
        ],
        "/w2",
    );
    check_first(
        &[def, other],
        "Token",
        true,
        "other-workspace names never silence a flag",
    );
}

#[test]
fn foreign_qualified_mention_skipped() {
    let def = ws_file(
        "member-a/src/a.rs",
        &[
            "pub type Result<T, E = Error> = std::result::Result<T, E>;",
            "fn f() -> Result<u8, Error> { todo!() }",
        ],
        "/w",
    );
    let other = ws_file(
        "member-b/src/b.rs",
        &[
            "use std::result::Result;",
            "use std::fmt;",
            "fn g() -> fmt::Result { todo!() }",
        ],
        "/w",
    );
    check_first(
        &[def, other],
        "Result",
        true,
        "foreign paths never resolve to the alias",
    );
}

#[test]
fn foreign_bare_collision_skipped() {
    let def = ws_file(
        "member-a/src/a.rs",
        &[
            "pub type Error = Box<dyn std::error::Error>;",
            "fn f() -> Error { todo!() }",
        ],
        "/w",
    );
    let other = ws_file(
        "member-b/src/b.rs",
        &[
            "use std::error::Error;",
            "fn g(x: Arc<Mutex<HashMap<String, HashMap<String, Vec<Error>>>>>) { todo!() }",
        ],
        "/w",
    );
    check_first(
        &[def, other],
        "Error",
        true,
        "foreign imports never evidence without the home segment",
    );
}

#[test]
fn home_qualified_use_counts() {
    let def = ws_file(
        "member-a/src/a.rs",
        &["pub type Token = u32;", "fn f(t: Token) {}"],
        "/w",
    );
    let other = ws_file(
        "member-b/src/b.rs",
        &["use member_a::Token;", "fn g(x: member_a::Token) {}"],
        "/w",
    );
    check_first(
        &[def, other],
        "Token",
        true,
        "home-segment paths resolve to the alias",
    );
}

#[test]
fn ptr_adjacent_use_flags() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Ids = Vec<u8>;",
            "fn f(ids: &Ids) -> usize { ids.len() }",
        ],
    );
    check_solo(
        def,
        "Ids",
        true,
        "`ptr_arg` fixes without a new alias, so the alias flags",
    );
}

#[test]
fn hasher_adjacent_use_flags() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Scores = HashMap<String, u32>;",
            "fn f(m: &Scores) -> usize { m.len() }",
        ],
    );
    check_solo(
        def,
        "Scores",
        true,
        "`implicit_hasher` fixes without a new alias, so the alias flags",
    );
}

#[test]
fn method_hasher_use_flags() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Scores = HashMap<String, u32>;",
            "struct Store;",
            "impl Store {",
            "    fn get(&self, m: &Scores) -> usize { m.len() }",
            "}",
        ],
    );
    check_solo(def, "Scores", true, "methods never trip the hasher lint");
}

#[test]
fn mut_vec_use_flags() {
    let def = test_file(
        "src/a.rs",
        &[
            "type Ids = Vec<u8>;",
            "fn push(ids: &mut Ids, v: u8) { ids.push(v); }",
        ],
    );
    check_solo(def, "Ids", true, "mutable references never trip `ptr_arg`");
}

#[test]
fn other_package_ignored() {
    let def = test_file(
        "member-a/src/a.rs",
        &["type Box<T> = alloc::boxed::Box<T>;", "fn f(x: Box<u8>) {}"],
    );
    let other = test_file(
        "member-b/src/b.rs",
        &["fn g(x: Box<Arc<Mutex<Vec<HashMap<String, Vec<u8>>>>>>>) {}"],
    );
    let aliases = collect_aliases(&def.lines);
    let files = vec![def, other];
    assert!(
        matches!(aliases.as_slice(), [alias] if alias.name == "Box" && is_unnecessary(&files, 0, alias, 250)),
        "other-package names never silence a flag"
    );
}
