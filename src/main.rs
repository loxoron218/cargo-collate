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
pub mod report;
pub mod rules_alias;
pub mod rules_docs;
pub mod rules_imports;
pub mod rules_paths;
pub mod rules_self_import;
pub mod rules_simple;
pub mod rules_tests;
pub mod rules_types;
pub mod rules_ui;
pub mod scan;

use std::{
    io::{Write, stdout},
    process::ExitCode,
};

use {
    report::{clone_check, report_misuse, show, user_args},
    rules_alias::{
        first_alias::first_occurrence_alias, thiserror::thiserror_alias, unnecessary_aliases,
    },
    rules_docs::{
        missing_docs::{doc_header::missing_mod_docs, missing_fn_docs},
        must_use::unnecessary_must_use,
        sections::{unnecessary_errors, unnecessary_panics},
    },
    rules_imports::{mixed_import_groups, stray::stray_uses},
    rules_paths::{enum_variants, prelude::prelude_groups, qualified_paths},
    rules_self_import::self_imports,
    rules_simple::{
        comment_lines::line_comments,
        dependencies::default_features_hits,
        expect::expect_hits,
        hygiene::{
            doc_blank::doc_blank_hits, dot_ok::dot_ok_hits, low_log::low_log_hits,
            parent_path::parent_hits, path_include::path_include_hits, scoped_pub::scoped_pub_hits,
            split_mod_doc::split_mod_doc_hits,
        },
        layout::{deep_nesting, long_files},
        manifest::stray_mod_rs,
        naming::{collision::duplicate_stems, grouping::generic_dirs},
        sync::{hits::std_sync_hits, thread_hits::thread_unsafe_hits},
        tracing::unstructured_tracing_hits,
        underscore::underscore_idents,
    },
    rules_tests::{check_test_rules, test_docs::unnecessary_test_docs},
    rules_types::unnecessary_types,
    rules_ui::{
        builder_pattern::builder_pattern_hits, missing_labels::missing_labels_hits,
        missing_underline::missing_underline_hits, ui_files::ui_file_hits,
    },
    scan::{
        SourceFile, discover_roots, have, load_files, load_manifests, rg_files, rg_manifests,
        rg_search, rg_ui_files,
    },
};

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
    fail |= check_ui_rules(&mut output, &dirs, &files);

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
    fail |= show(
        output,
        "Split module docs; keep //! blocks contiguous with //! separators, never empty lines.",
        &split_mod_doc_hits(files),
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
         lines, unconditional then cfg sub-blocks inside one block.",
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
        "Unstructured tracing; use fields for structured data, never interpolated strings.",
        &unstructured_tracing_hits(files),
    );
    fail |= show(
        output,
        "Prelude imports inside gtk family groups; use libadwaita prelude instead.",
        &prelude_groups(files),
    );
    fail |= show(
        output,
        "Std sync Mutex/RwLock/Condvar; use parking_lot instead.",
        &std_sync_hits(files),
    );
    fail |= show(
        output,
        "Thread-unsafe Cell/RefCell/Rc; use Atomics, parking_lot, OnceLock, or Arc instead.",
        &thread_unsafe_hits(files),
    );
    fail |= show(
        output,
        "Unnecessary import aliases; remove as without conflicts.",
        &unnecessary_aliases(files),
    );
    fail |= show(
        output,
        "Import alias on first occurrence; keep the first import plain and alias subsequent \
         colliding imports instead.",
        &first_occurrence_alias(files),
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
        "Source files over limit (400 lines); trim unnecessary lines first, split into smaller \
         modules when no longer possible.",
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
        "Duplicate module stems; keep every file name unique codebase-wide.",
        &duplicate_stems(&rg_files(dirs)),
    );
    fail |= show(
        output,
        "Crate dependencies without default-features = false; add default-features = false with \
         only required features.",
        &default_features_hits(&load_manifests(&rg_manifests(dirs))),
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

/// Declarative UI and CSS rules.
fn check_ui_rules(output: &mut String, dirs: &[String], files: &[SourceFile]) -> bool {
    let mut fail = false;
    fail |= show(
        output,
        "Declarative UI/CSS files or string-literal references; build widgets programmatically \
         with css_classes instead.",
        &ui_file_hits(files, &rg_ui_files(dirs)),
    );
    fail |= show(
        output,
        "Widget builders without `css_classes` and `tooltip_text`, redundant `.can_focus(true)`; \
         direct construction or post-build setters; use the builder pattern instead, or \
         `glib::Object::builder::<T>()` for types without a generated builder.",
        &builder_pattern_hits(files),
    );
    fail |= show(
        output,
        "Widget builders without accessibility labels; call update_property with an \
         import-resolved label alias in the same fn.",
        &missing_labels_hits(files),
    );
    fail |= show(
        output,
        "Mnemonic `_X` label text without underline enablement; add use_underline(true) to the \
         builder or set_use_underline(true) in the same fn.",
        &missing_underline_hits(files),
    );
    fail
}

fn main() -> ExitCode {
    match run() {
        0 => ExitCode::SUCCESS,
        1 => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}
