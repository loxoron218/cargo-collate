//! Tests for test-module and error-type rules.

use crate::{
    rules_tests::{
        boundary::{anyhow_leak, anyhow_site, missing_context},
        order::module_order,
        range::{anyhow_gap, test_ranges},
        stray_comment::stray_test_comments,
    },
    scan::support::test_file,
};

#[test]
fn multiline_raw_string_keeps_test_range() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(test)]",
            "mod tests {",
            "    use anyhow::{Result, ensure};",
            "    #[test]",
            "    fn t() -> Result<()> {",
            "        let legacy = r#\"{",
            "            \"a\": 1",
            "        }\"#;",
            "        ensure!(true, \"x\");",
            "        Ok(())",
            "    }",
            "    #[test]",
            "    fn u() -> Result<()> {",
            "        ensure!(true, \"y\");",
            "        Ok(())",
            "    }",
            "}",
            "use anyhow::Result;",
        ],
    )];
    let ranges = files
        .first()
        .map_or_default(|file| test_ranges(&file.lines, anyhow_gap));
    assert_eq!(ranges, vec![(1, 17)], "range spans the module");
    assert_eq!(
        anyhow_site(&files),
        vec!["src/a.rs:18:use anyhow::Result;".to_owned()],
        "post-module use still flags"
    );
}

#[test]
fn block_comment_braces_keep_test_range() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(test)]",
            "mod tests {",
            "    use anyhow::{Result, ensure};",
            "    /* disabled shape {",
            "       still commented } */",
            "    #[test]",
            "    fn t() -> Result<()> {",
            "        ensure!(true, \"x\");",
            "        Ok(())",
            "    }",
            "}",
        ],
    )];
    assert!(
        anyhow_site(&files).is_empty(),
        "commented braces stay silent"
    );
}

#[test]
fn leak_flags_pub_result_in_lib() {
    let files = vec![test_file(
        "src/a.rs",
        &["pub fn f() -> anyhow::Result<()> {"],
    )];
    assert_eq!(anyhow_leak(&files).len(), 1);
    assert_eq!(anyhow_site(&files).len(), 1);
}

#[test]
fn site_flags_private_anyhow_in_lib() {
    let files = vec![test_file(
        "src/a.rs",
        &["use anyhow::Result;", "fn f() -> Result<()> {"],
    )];
    assert!(anyhow_leak(&files).is_empty(), "private stays silent");
    assert_eq!(anyhow_site(&files).len(), 1);
}

#[test]
fn site_exempts_bin_and_main() {
    let main = vec![test_file("src/main.rs", &["use anyhow::Result;"])];
    let bin = vec![test_file("src/bin/tool.rs", &["use anyhow::Result;"])];
    assert!(anyhow_site(&main).is_empty(), "main stays silent");
    assert!(anyhow_site(&bin).is_empty(), "bin stays silent");
    assert!(anyhow_leak(&main).is_empty(), "main leak stays silent");
}

#[test]
fn site_exempts_test_modules() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(test)]",
            "mod tests {",
            "    use anyhow::Result;",
            "}",
        ],
    )];
    assert!(anyhow_site(&files).is_empty(), "cfg-test stays silent");
    let integration = vec![test_file("tests/x.rs", &["use anyhow::Result;"])];
    assert!(
        anyhow_site(&integration).is_empty(),
        "tests dir stays silent"
    );
}

#[test]
fn site_still_flags_benches() {
    let files = vec![test_file("benches/b.rs", &["use anyhow::Result;"])];
    assert_eq!(anyhow_site(&files).len(), 1, "benches stay library-like");
}

#[test]
fn context_flags_bare_question_in_bin() {
    let files = vec![test_file(
        "src/main.rs",
        &["fn f() -> anyhow::Result<()> {", "    let x = foo()?;", "}"],
    )];
    assert_eq!(missing_context(&files).len(), 1);
}

#[test]
fn context_allows_attached_calls() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "fn f() -> anyhow::Result<()> {",
            "    let a = foo().context(\"x\")?;",
            "    let b = bar().with_context(|| \"y\")?;",
            "}",
        ],
    )];
    assert!(missing_context(&files).is_empty(), "context stays silent");
}

#[test]
fn context_allows_multiline_chains() {
    let files = vec![test_file(
        "src/bin/tool.rs",
        &[
            "fn f() -> anyhow::Result<()> {",
            "    let x = foo()",
            "        .with_context(|| \"ctx\")?;",
            "}",
        ],
    )];
    assert!(
        missing_context(&files).is_empty(),
        "split chain stays silent"
    );
}

#[test]
fn site_flags_bail_macro_in_lib() {
    let files = vec![test_file(
        "src/a.rs",
        &["fn f() {", "    bail!(\"x\");", "}"],
    )];
    assert_eq!(anyhow_site(&files).len(), 1, "bail flags");
}

#[test]
fn context_ignores_sized_and_strings() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "use anyhow::Result;",
            "fn f<T: ?Sized>() {}",
            "let s = \"what?\";",
        ],
    )];
    assert!(
        missing_context(&files).is_empty(),
        "non-propagation stays silent"
    );
}

#[test]
fn context_skips_binaries_without_anyhow() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "fn main() -> Result<(), std::io::Error> {",
            "    processor.process(&input, &output)?;",
            "    Ok(())",
            "}",
        ],
    )];
    assert!(
        missing_context(&files).is_empty(),
        "typed errors stay silent"
    );
}

#[test]
fn context_ignores_trace_sigil() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "use anyhow::Result;",
            "fn f() -> Result<()> {",
            "    tracing::trace!(?localedir);",
            "    Ok(())",
            "}",
        ],
    )];
    assert!(missing_context(&files).is_empty(), "sigils stay silent");
}

#[test]
fn context_ignores_raw_string_placeholders() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "fn f(pool: &MySqlPool) -> anyhow::Result<u64> {",
            "    let id = sqlx::query!(",
            "        r#\"",
            "INSERT INTO t (d) VALUES ( ? )",
            "        \"#,",
            "        description",
            "    )",
            "    .execute(pool)",
            "    .await?;",
            "    Ok(id)",
            "}",
        ],
    )];
    let hits = missing_context(&files);
    assert_eq!(hits.len(), 1, "only propagations flag, never placeholders");
    assert!(
        hits.first().is_some_and(|h| h.contains(":9:")),
        "the await line flags"
    );
    let contextual = vec![test_file(
        "src/main.rs",
        &[
            "fn g(pool: &MySqlPool) -> anyhow::Result<u64> {",
            "    let id = sqlx::query!(",
            "        r#\"",
            "SELECT id FROM t WHERE d = ?",
            "        \"#,",
            "        description",
            "    )",
            "    .fetch_one(pool)",
            "    .await",
            "    .context(\"load\")?;",
            "    Ok(id)",
            "}",
        ],
    )];
    assert!(
        missing_context(&contextual).is_empty(),
        "attached context stays silent"
    );
}

#[test]
fn context_flags_anyhow_ctor_without_context() {
    let files = vec![test_file(
        "src/main.rs",
        &[
            "fn f() -> anyhow::Result<()> {",
            "    let x = foo().map_err(|e| anyhow!(e))?;",
            "    let y = bar().ok_or_else(|| anyhow::anyhow!(\"missing\"))?;",
            "    Ok((x, y))",
        ],
    )];
    assert_eq!(
        missing_context(&files).len(),
        2,
        "inline ctors need attached context"
    );
    let contextual = vec![test_file(
        "src/main.rs",
        &[
            "fn g() -> anyhow::Result<()> {",
            "    let x = foo().map_err(|e| anyhow!(e)).context(\"x\")?;",
            "    Ok(x)",
            "}",
        ],
    )];
    assert!(
        missing_context(&contextual).is_empty(),
        "attached context stays silent"
    );
}

#[test]
fn late_mod_flagged() {
    let files = vec![test_file("src/a.rs", &["fn f() {}", "mod late;"])];
    assert_eq!(module_order(&files).len(), 1);
}

#[test]
fn stray_comment_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(test)]",
            "mod tests {",
            "    // stray",
            "    #[test]",
            "    fn t() {}",
            "}",
        ],
    )];
    assert_eq!(stray_test_comments(&files).len(), 1);
}
