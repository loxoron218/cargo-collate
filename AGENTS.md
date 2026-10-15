---
name: code_agent
description: Senior Rust developer using modern idiomatic Rust for `collate`
---

# collate

Project-specific hygiene checks for strict Rust projects (cargo plugin, std-only plus `rg` / `jscpd`
CLIs).

## Tech stack

- **Runtime:** std-only, no `[dependencies]`; synchronous CLI via `std::process::Command`
- **External tools:** `rg` (ripgrep) and `jscpd` in `PATH`, invoked as subprocesses
- **Distribution:** `cargo install --path collate`, binary `cargo-collate` via `src/main.rs`

## Codebase map (src/)

Every lint below is named by its report function; the `README.md` lints table carries the rule
text per lint.

- `src/main.rs` — binary entry, exit codes 0/1/2, and rule aggregation
- `src/scan.rs` — process runners, file discovery (`rg --files`) and loading
- `src/gating.rs` — shared cfg-test gating helpers
- `src/lexer.rs` + `src/lexer/` — comment/string lexing, markers, qualified paths, visibility
- `src/report.rs` — finding sections, misuse reports, and clone detection (`clone_check` via
  `jscpd`)
- `src/rules_alias.rs` + `src/rules_alias/` — `first_occurrence_alias`, `thiserror_alias`,
  `unnecessary_aliases`
- `src/rules_docs.rs` + `src/rules_docs/` — `missing_fn_docs`, `missing_mod_docs`,
  `pub_mod_inner_docs`, `unnecessary_errors`, `unnecessary_must_use`, `unnecessary_panics`,
  `unnecessary_test_docs`
- `src/rules_imports.rs` + `src/rules_imports/` — `mixed_import_groups`, `stray_uses`
- `src/rules_paths.rs` + `src/rules_paths/` — `enum_variants`, `prelude_groups`, `qualified_paths`
- `src/rules_self_import.rs` + `src/rules_self_import/` — `self_imports`
- `src/rules_simple.rs` + `src/rules_simple/` — `deep_nesting`, `default_features_hits`,
  `doc_blank_hits`, `dot_ok_hits`, `duplicate_stems`, `expect_hits`, `generic_dirs`,
  `line_comments`, `long_files`, `low_log_hits`, `multi_cfg_files`, `parent_hits`,
  `path_include_hits`, `scoped_pub_hits`, `split_mod_doc_hits`, `std_sync_hits`, `stray_mod_rs`, `thread_unsafe_hits`,
  `underscore_idents`, `unstructured_tracing_hits` (quote tracking in `dependencies/quotes.rs`,
  TOML scanners in `dependencies/toml.rs`, call grouping and argument analysis in
  `tracing_parse.rs`, string-literal masking in `tracing_mask.rs` plus `tracing_string.rs`)
- `src/rules_tests.rs` + `src/rules_tests/` — `anyhow_leak`, `anyhow_site`, `inner_test_gates`,
  `missing_context`, `mixed_mod_groups`, `module_order`, `stray_test_comments`,
  `ungated_test_modules` (`multi_cfg_files` is wired here but implemented in `rules_simple`)
- `src/rules_types.rs` + `src/rules_types/` — `unnecessary_types`
- `src/rules_ui.rs` + `src/rules_ui/` — `builder_pattern_hits`, `missing_labels_hits`, `missing_underline_hits`, `ui_file_hits`

## Conventions & workflow

- ALWAYS read `CODING_STANDARDS.md` in full before any code-related task — single source of truth
  for style, error handling, concurrency, tracing, docs, testing, and build commands (lint,
  format, test, bench) — no exceptions.
