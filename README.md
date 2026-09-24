# collate

Project-specific hygiene checks for strict Rust projects. Pure Rust plus the `rg` / `jscpd` CLIs.

## Requirements

- `rg` (ripgrep) in `PATH`
- `jscpd` in `PATH`

## Usage

```bash
cargo install --path collate
cargo collate
```

No arguments. Exits `0` when clean, `1` on findings, `2` on usage / environment errors.

Scans every `*.rs` file under `.` via `rg --files` (`target/` and `vendor/` excluded), so
workspace-excluded crates stay visible. Package dirs resolve via the nearest ancestor `Cargo.toml`;
workspace scope is the shared git toplevel.

## Lints

| Lint | Rule |
| --- | --- |
| `anyhow_leak` | Anyhow in public API; library uses typed thiserror enums. |
| `anyhow_site` | Anyhow outside tests and binaries; library uses typed errors. |
| `clone_check` | Copy-paste clones across sources; ignore duplicates that are only import blocks. |
| `deep_nesting` | Module nesting over the depth limit; keep module nesting shallow (max 2). |
| `doc_blank_hits` | Doc comments without a preceding blank line; separate `///` blocks with an empty line. |
| `dot_ok_hits` | Dot-ok error swallows; propagate errors with context instead. |
| `duplicate_stems` | Duplicate module stems; keep every word of file names unique codebase-wide. |
| `enum_variants` | Enum variants at call sites; import variants via nested `use` instead. |
| `expect_hits` | Expect attributes; fix lints instead of suppressing them. |
| `generic_dirs` | Generic module directories; group by capability/domain instead. |
| `inner_test_gates` | Inner cfg-test gates; use cfg(test) mod tests instead. |
| `line_comments` | Remove all line comments. |
| `long_files` | Source files over limit (400 lines); split into smaller modules. |
| `low_log_hits` | Low-level logging; use `info`, `warn`, or `error` for significant events. |
| `missing_context` | Bare `?` without context, including inline anyhow! errors; use `.context()` or `.with_context()`. |
| `missing_fn_docs` | Functions without attached docs; every fn outside test code needs `///`, std methods exempt. Complements `clippy::missing_docs_in_private_items`. |
| `missing_mod_docs` | Files without leading module docs; every file needs a `//!` line on line 1. Complements `rustc::missing_docs` / `clippy::missing_docs_in_private_items`. |
| `mixed_import_groups` | Mixed import groups; separate `std`, external, self-package, and `crate` blocks with blank lines. Complements `group_imports = "StdExternalCrate"` in `rustfmt.toml`. |
| `mixed_mod_groups` | Mixed mod groups; separate `macro_use`, ordinary, and `cfg(test)` blocks with blank lines. |
| `module_order` | Module order with tests at bottom; keep declarations at top with actual tests. |
| `multi_cfg_files` | Files with more than one inline cfg-test module; one `mod tests` per file. |
| `ungated_test_modules` | Top-level test code without a parent cfg(test) gate; gate this module from the parent instead. |
| `parent_hits` | Parent-module paths; prefer crate-relative imports. |
| `path_include_hits` | Path attributes and include macros; use standard module declarations instead. |
| `prelude_groups` | Prelude imports inside gtk family groups; use libadwaita prelude instead. |
| `qualified_paths` | Qualified call sites; import via `use` instead with constructors exempt. |
| `scoped_pub_hits` | Scoped pub visibility; keep visibility explicit and consistent. |
| `self_imports` | Use imports resolving into the file own module; refer to own items directly instead. |
| `stray_mod_rs` | Nested indexes that are not target roots; use parent indexes instead. |
| `stray_test_comments` | Stray comments inside cfg-test modules with docs exempt. |
| `stray_uses` | Stray use after code; keep all use at module top. Complements `clippy::arbitrary_source_item_ordering`. |
| `thiserror_alias` | Aliased thiserror::Error; keep thiserror plain and alias the other Error instead. |
| `underscore_idents` | Underscore-prefixed identifiers; use meaningful names instead of discards. |
| `unnecessary_aliases` | Unnecessary import aliases; remove `as` without conflicts. |
| `unnecessary_errors` | Unnecessary Errors sections; private items and non-Result items never require them. Complements `clippy::missing_errors_doc`. |
| `unnecessary_must_use` | Unnecessary `must_use` attributes; private, unit, mut-arg, or forwarding items never require them. Complements `clippy::must_use_candidate` / `clippy::return_self_not_must_use`. |
| `unnecessary_panics` | Unnecessary Panics sections; private items and pure public items never require them. Complements `clippy::missing_panics_doc`. |
| `unnecessary_test_docs` | Comments on private test-module functions; shared pub helpers stay documented. |
| `unnecessary_types` | Unnecessary type definitions; aliases removable warning-free at every use site never require them. Complements `clippy::type_complexity`. |
