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

- `src/main.rs` — binary entry, exit codes 0/1/2, rule aggregation and reporting
- `src/scan.rs` — process runners, file discovery (`rg --files`) and loading
- `src/gating.rs` — shared cfg-test gating helpers
- `src/lexer.rs` + `src/lexer/` — comment/string lexing, markers, qualified paths, visibility
- `src/rules_alias.rs` + `src/rules_alias/` — unnecessary import aliases, thiserror alias
- `src/rules_docs.rs` + `src/rules_docs/` — missing docs, must-use, doc sections
- `src/rules_imports.rs` + `src/rules_imports/` — import grouping, stray uses
- `src/rules_paths.rs` + `src/rules_paths/` — qualified paths, enum variants, prelude groups
- `src/rules_self_import.rs` + `src/rules_self_import/` — self-import detection
- `src/rules_simple.rs` + `src/rules_simple/` — text-level hygiene, layout, naming, manifest
- `src/rules_tests.rs` + `src/rules_tests/` — test-module gating, order, docs
- `src/rules_types.rs` + `src/rules_types/` — unnecessary type-alias detection

## Conventions & workflow

- ALWAYS read `CODING_STANDARDS.md` in full before any code-related task — single source of truth
  for style, error handling, concurrency, tracing, docs, testing, and build commands (lint,
  format, test, bench) — no exceptions.
