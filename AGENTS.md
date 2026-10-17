---
name: code_agent
description: Senior Rust developer using modern idiomatic Rust for `cargo-collate`
---

# cargo-collate

Project-specific hygiene checks for strict Rust projects (cargo plugin, std-only plus `rg` / `jscpd`
CLIs).

## Tech stack

- **Runtime:** std-only, no `[dependencies]`; synchronous CLI via `std::process::Command`
- **External tools:** `rg` (ripgrep) and `jscpd` in `PATH`, invoked as subprocesses
- **Distribution:** `cargo install cargo-collate` (user-level), binary `cargo-collate` via `src/main.rs`

## Codebase map (src/)

The `README.md` lints table is the single source of truth for lint names and rule text.

- `src/main.rs` — binary entry, exit codes 0/1/2, and rule aggregation
- `src/scan.rs` + `src/scan/` — process runners, file discovery (`rg --files`) and loading
- `src/gating.rs` — shared cfg-test gating helpers
- `src/lexer.rs` + `src/lexer/` — comment/string lexing, markers, qualified paths, visibility
- `src/report.rs` — finding sections, misuse reports, and clone detection via `jscpd`
- `src/rules_alias.rs` + `src/rules_alias/` — import alias rules
- `src/rules_cycles.rs` + `src/rules_cycles/` — module dependency rules
- `src/rules_docs.rs` + `src/rules_docs/` — documentation rules
- `src/rules_imports.rs` + `src/rules_imports/` — import grouping and placement rules
- `src/rules_paths.rs` + `src/rules_paths/` — path and variant rules
- `src/rules_self_import.rs` + `src/rules_self_import/` — own-module import rules
- `src/rules_simple.rs` + `src/rules_simple/` — text-level hygiene, layout, manifest, naming,
  sync, and tracing rules
- `src/rules_tests.rs` + `src/rules_tests/` — test-module and error-context rules
- `src/rules_types.rs` + `src/rules_types/` — type definition rules
- `src/rules_ui.rs` + `src/rules_ui/` — widget builder and declarative UI rules

## Conventions & workflow

- ALWAYS read `CODING_STANDARDS.md` in full before any code-related task — single source of truth
  for style, error handling, concurrency, tracing, docs, testing, and build commands (lint,
  format, test, bench) — no exceptions.
