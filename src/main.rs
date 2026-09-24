//! Collate: project-specific hygiene checks for strict Rust projects.
//!
//! Exits 1 when any unwanted pattern hits, 2 on usage and environment errors.
//! Ripgrep and clone detection stay as subprocesses while Python stages are native.
//!
//! # Arguments
//!
//! None. Install once, then run `cargo collate` anywhere.
//!
//! # Returns
//!
//! Process exit code clean, findings, or misuse.

pub mod gating;
pub mod lexer;
pub mod rules_alias;
pub mod rules_docs;
pub mod rules_imports;
pub mod rules_paths;
pub mod rules_self_import;
pub mod rules_simple;
pub mod rules_tests;
pub mod rules_types;
pub mod scan;

use std::{
    env::args_os,
    ffi::OsString,
    io::{Write, stderr, stdout},
    process::{Command, ExitCode},
};

use {
    rules_alias::{thiserror::thiserror_alias, unnecessary_aliases},
    rules_docs::{
        missing_docs::{doc_header::missing_mod_docs, missing_fn_docs},
        must_use::unnecessary_must_use,
        sections::{unnecessary_errors, unnecessary_panics},
    },
    rules_imports::{mixed_import_groups, stray::stray_uses},
    rules_paths::{enum_variants, prelude::prelude_groups, qualified_paths},
    rules_self_import::self_imports,
    rules_simple::{
        expect::expect_hits,
        hygiene::{
            doc_blank_hits, dot_ok_hits, low_log_hits, parent_hits, path_include_hits,
            scoped_pub_hits,
        },
        layout::deep_nesting,
        line_comments, long_files,
        manifest::stray_mod_rs,
        naming::{collision::duplicate_stems, grouping::generic_dirs},
        underscore::underscore_idents,
    },
    rules_tests::{check_test_rules, test_docs::unnecessary_test_docs},
    rules_types::unnecessary_types,
    scan::{SourceFile, discover_roots, have, load_files, rg_files, rg_search},
};

/// Append one rule section to `output`; returns true when it hit.
pub fn show(output: &mut String, title: &str, hits: &[String]) -> bool {
    if hits.is_empty() {
        return false;
    }
    output.push_str("\n# ");
    output.push_str(title);
    output.push('\n');
    for hit in hits {
        output.push_str(hit);
        output.push('\n');
    }
    true
}

/// Report misuse on standard error; returns process code 2.
fn report_misuse(message: &str) -> i32 {
    let mut handle = stderr().lock();
    if writeln!(handle, "{message}").is_err() {
        return 2;
    }
    2
}

/// Keep only clone sections before the details table.
fn clone_sections(output: &str) -> Vec<String> {
    let mut kept = Vec::new();
    let mut emit = false;
    for line in output.lines() {
        if line.starts_with("Clone found") {
            emit = true;
            kept.push(line.to_owned());
            continue;
        }
        if line.starts_with('┌') {
            emit = false;
            continue;
        }
        if emit {
            kept.push(line.to_owned());
        }
    }
    kept
}

/// Run the clone detector; returns findings with failed flag.
///
/// Skips `target/` and `vendor/` explicitly since `jscpd` ignores
/// `.gitignore`, unlike `rg`.
fn clone_check(dirs: &[String]) -> (Vec<String>, bool) {
    let mut cmd = Command::new("jscpd");
    let result = cmd
        .arg("--format")
        .arg("rust")
        .arg("--no-colors")
        .arg("--no-tips")
        .arg("--absolute")
        .arg("--exit-code")
        .arg("1")
        .arg("--ignore")
        .arg("**/target/**,**/vendor/**")
        .args(dirs)
        .output();
    match result {
        Err(_) => (
            vec!["ERROR: failed to execute clone detector".to_owned()],
            true,
        ),
        Ok(out) => {
            let findings = clone_sections(&String::from_utf8_lossy(&out.stdout));
            let failed = !out.status.success() || !findings.is_empty();
            (findings, failed)
        }
    }
}

/// User arguments after the binary and cargo subcommand names.
///
/// Cargo hands external subcommands their own name first, so a bare
/// `cargo collate` run arrives with one argument to ignore.
fn user_args() -> Vec<OsString> {
    let args: Vec<OsString> = args_os().skip(1).collect();
    let drop_first: bool = args.first().is_some_and(|first| first == "collate");
    if drop_first {
        args.into_iter().skip(1).collect()
    } else {
        args
    }
}

/// Entry point without arguments; returns process code.
fn run() -> i32 {
    if !user_args().is_empty() {
        return report_misuse("usage: cargo collate (no arguments)");
    }
    if !have("rg") {
        return report_misuse("ERROR: search tool not found in PATH");
    }
    if !have("jscpd") {
        return report_misuse("ERROR: clone detector not found in PATH");
    }
    let dirs = discover_roots();
    if dirs.is_empty() {
        return report_misuse("ERROR: no scan dirs found");
    }

    let files = load_files(&rg_files(&dirs));
    let mut output = String::new();
    let mut fail = false;
    fail |= check_name_rules(&mut output, &dirs, &files);
    fail |= check_path_rules(&mut output, &files);
    fail |= check_test_rules(&mut output, &files);
    fail |= check_doc_rules(&mut output, &files);
    fail |= check_layout_rules(&mut output, &dirs, &files);

    let mut handle = stdout().lock();
    if write!(handle, "{output}").is_err() {
        return 2;
    }
    i32::from(fail)
}

/// Identifier, error-handling, and comment rules.
fn check_name_rules(output: &mut String, dirs: &[String], files: &[SourceFile]) -> bool {
    let mut fail = false;
    fail |= show(
        output,
        "Underscore-prefixed identifiers; use meaningful names instead of discards.",
        &underscore_idents(
            &rg_search("-P", r#"(?<![\w"}\*`])_[A-Za-z][A-Za-z0-9_]*\b"#, dirs),
            files,
        ),
    );
    fail |= show(
        output,
        "Dot-ok error swallows; propagate errors with context instead.",
        &dot_ok_hits(files),
    );
    fail |= show(
        output,
        "Parent-module paths; prefer crate-relative imports.",
        &parent_hits(files),
    );
    fail |= show(
        output,
        "Scoped pub visibility; keep visibility explicit and consistent.",
        &scoped_pub_hits(files),
    );
    fail |= show(
        output,
        "Expect attributes; fix lints instead of suppressing them.",
        &expect_hits(files),
    );
    fail |= show(output, "Remove all line comments.", &line_comments(files));
    fail |= show(
        output,
        "Doc comments without a preceding blank line; separate /// blocks with an empty line.",
        &doc_blank_hits(files),
    );
    fail
}

/// Import and path rules.
fn check_path_rules(output: &mut String, files: &[SourceFile]) -> bool {
    let mut fail = false;
    fail |= show(
        output,
        "Qualified call sites; import via use instead with constructors exempt.",
        &qualified_paths(files),
    );
    fail |= show(
        output,
        "Enum variants at call sites; import variants via nested use instead.",
        &enum_variants(files),
    );
    fail |= show(
        output,
        "Use imports resolving into the file own module; refer to own items directly instead.",
        &self_imports(files),
    );
    fail |= show(
        output,
        "Mixed import groups; separate std, external, self-package, and crate blocks with blank \
         lines.",
        &mixed_import_groups(files),
    );
    fail |= show(
        output,
        "Stray use after code; keep all use at module top.",
        &stray_uses(files),
    );
    fail |= show(
        output,
        "Path attributes and include macros; use standard module declarations instead.",
        &path_include_hits(files),
    );
    fail |= show(
        output,
        "Low-level logging; use info, warn, or error for significant events.",
        &low_log_hits(files),
    );
    fail |= show(
        output,
        "Prelude imports inside gtk family groups; use libadwaita prelude instead.",
        &prelude_groups(files),
    );
    fail |= show(
        output,
        "Unnecessary import aliases; remove as without conflicts.",
        &unnecessary_aliases(files),
    );
    fail |= show(
        output,
        "Aliased thiserror::Error; keep thiserror plain and alias the other Error instead.",
        &thiserror_alias(files),
    );
    fail
}

/// Unnecessary attributes and doc sections removable warning-free.
fn check_doc_rules(output: &mut String, files: &[SourceFile]) -> bool {
    let mut fail = false;
    fail |= show(
        output,
        "Functions without attached docs; outside test code every fn needs a /// line, std \
         methods exempt.",
        &missing_fn_docs(files),
    );
    fail |= show(
        output,
        "Files without leading module docs; every file needs a //! line on line 1.",
        &missing_mod_docs(files),
    );
    fail |= show(
        output,
        "Unnecessary must_use attributes; private, unit, mut-arg, or forwarding items never \
         require them.",
        &unnecessary_must_use(files),
    );
    fail |= show(
        output,
        "Unnecessary Panics sections; private items and pure public items never require them.",
        &unnecessary_panics(files),
    );
    fail |= show(
        output,
        "Unnecessary Errors sections; private items and non-Result items never require them.",
        &unnecessary_errors(files),
    );
    fail |= show(
        output,
        "Comments on private test-module functions; shared pub helpers stay documented.",
        &unnecessary_test_docs(files),
    );
    fail |= show(
        output,
        "Unnecessary type definitions; aliases removable warning-free at every use site never \
         require them.",
        &unnecessary_types(files),
    );
    fail
}

/// File-layout and clone rules.
fn check_layout_rules(output: &mut String, dirs: &[String], files: &[SourceFile]) -> bool {
    let mut fail = false;
    fail |= show(
        output,
        "Source files over limit; split into smaller modules.",
        &long_files(files),
    );
    fail |= show(
        output,
        "Nested indexes that are not target roots; use parent indexes instead.",
        &stray_mod_rs(&rg_files(dirs)),
    );
    fail |= show(
        output,
        "Module nesting over the depth limit; keep module nesting shallow (max 2).",
        &deep_nesting(&rg_files(dirs)),
    );
    fail |= show(
        output,
        "Generic module directories; group by capability/domain instead.",
        &generic_dirs(&rg_files(dirs)),
    );
    fail |= show(
        output,
        "Duplicate module stems; keep every word of file names unique codebase-wide.",
        &duplicate_stems(&rg_files(dirs)),
    );

    let (clones, clone_fail) = clone_check(dirs);
    if clone_fail {
        fail |= show(
            output,
            "Copy-paste clones across sources; ignore duplicates that are only import blocks.",
            &clones,
        );
        fail |= clones.is_empty();
    }
    fail
}

fn main() -> ExitCode {
    match run() {
        0 => ExitCode::SUCCESS,
        1 => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}
