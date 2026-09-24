//! Tests for stray `use` detection.

use crate::{rules_imports::stray::stray_uses, scan::support::test_file};

fn hits(lines: &[&str]) -> Vec<String> {
    stray_uses(&[test_file("src/a.rs", lines)])
}

fn check_hit(name: &str, lines: &[&str], line: usize) {
    let found = hits(lines);
    assert_eq!(found.len(), 1, "{name}: one stray flags");
    assert!(
        found
            .first()
            .is_some_and(|hit| hit.contains(&format!(":{line}:"))),
        "{name}: flagged line matches"
    );
}

fn check_silent(name: &str, lines: &[&str]) {
    assert!(hits(lines).is_empty(), "{name}: stays silent");
}

#[test]
fn headers_stay_silent() {
    let clean: &[(&str, &[&str])] = &[
        (
            "top",
            &[
                "//! Docs.",
                "use std::a;",
                "",
                "use crate::b;",
                "fn make() {}",
            ],
        ),
        (
            "mods first",
            &[
                "extern crate foo;",
                "mod bar;",
                "use std::a;",
                "fn first() {}",
            ],
        ),
        (
            "plain mod",
            &["mod foo {", "    use std::a;", "}", "use std::b;"],
        ),
        (
            "test header",
            &[
                "#[cfg(test)]",
                "mod tests {",
                "    use super::y;",
                "",
                "    use std::b;",
                "    #[test]",
                "    fn first() {}",
                "}",
            ],
        ),
        (
            "quoted",
            &[
                "use std::a;",
                "fn first() {}",
                "let s = \"use b;\";",
                "// use c;",
            ],
        ),
        (
            "guard",
            &[
                "compile_error!(\"need a feature\");",
                "pub use crate::thing::Thing;",
            ],
        ),
    ];
    for (name, lines) in clean {
        check_silent(name, lines);
    }
}

#[test]
fn templates_stay_silent() {
    let clean: &[(&str, &[&str])] = &[
        (
            "template",
            &[
                "macro_rules! make {",
                "    ($name:ident) => {",
                "        mod $name {",
                "            use $crate::inner::Thing;",
                "        }",
                "    };",
                "}",
                "fn first() {}",
            ],
        ),
        (
            "template late",
            &[
                "macro_rules! make {",
                "    ($t:ident) => {",
                "        mod inner {",
                "            fn a() {}",
                "            fn b() {",
                "                use x::Y;",
                "            }",
                "        }",
                "    };",
                "}",
            ],
        ),
        (
            "quote brace",
            &[
                "fn make() {",
                "    quote! {",
                "        use foo::Bar;",
                "    }",
                "}",
            ],
        ),
        (
            "quote paren",
            &[
                "fn make(span: Span) {",
                "    quote_spanned!(span =>",
                "        if false {",
                "            use foo::Bar;",
                "        }",
                "    )",
                "}",
            ],
        ),
    ];
    for (name, lines) in clean {
        check_silent(name, lines);
    }
}

#[test]
fn late_module_uses_flag() {
    let late: &[(&str, &[&str], usize)] = &[
        (
            "single",
            &["use std::a;", "pub fn first() {}", "use std::b;"],
            3,
        ),
        (
            "glob",
            &["use std::a;", "fn first() {}", "use std::b::*;"],
            3,
        ),
        (
            "joined",
            &["fn first() {}", "use std::{", "    path::Path,", "};"],
            2,
        ),
        (
            "item macro",
            &[
                "bitflags! {",
                "    struct Flags: u8 {",
                "    }",
                "}",
                "use std::a;",
            ],
            5,
        ),
        (
            "test inner",
            &[
                "use super::x;",
                "#[cfg(test)]",
                "mod tests {",
                "    use super::y;",
                "",
                "    #[test]",
                "    fn first() {}",
                "    use std::b;",
                "}",
            ],
            8,
        ),
    ];
    for (name, lines, line) in late {
        check_hit(name, lines, *line);
    }
}

#[test]
fn block_uses_flag() {
    let blocked: &[(&str, &[&str], usize)] = &[
        ("fn body", &["fn make() {", "    use std::a;", "}"], 2),
        (
            "negated",
            &[
                "fn first(ok: bool) {",
                "    if !ok {",
                "        use std::a;",
                "    }",
                "}",
            ],
            3,
        ),
        (
            "invocation",
            &[
                "bitflags! {",
                "    use crate::flags::Mask;",
                "    struct X: u8 {",
                "    }",
                "}",
            ],
            2,
        ),
        (
            "wrapper",
            &[
                "wrap_client! {",
                "    fn make() {",
                "        use std::a;",
                "    }",
                "}",
            ],
            3,
        ),
    ];
    for (name, lines, line) in blocked {
        check_hit(name, lines, *line);
    }
}
