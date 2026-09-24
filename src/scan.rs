//! Process runners and file loading.
//!
//! The `rg` (ripgrep, Rust) and `jscpd` (v5, Rust) CLIs stay as subprocesses:
//! they keep their SIMD/parallel/gitignore behavior while `python3`, `awk`,
//! `wc`, and `comm` are gone. Discovery scans every `.rs` file under the
//! current dir, so workspace-excluded crates stay visible without relying on
//! `cargo metadata`.

/// Test-only source-file builders.
#[cfg(test)]
pub mod support;

use std::{
    env::{split_paths, var_os},
    fs::read_to_string,
    path::Path,
    process::Command,
};

use crate::lexer::literal::stripped_lines;

/// A scanned Rust source file with its original lines.
#[derive(Debug)]
pub struct SourceFile {
    /// Path exactly as reported by `rg` (relative, e.g. `src/foo.rs`).
    pub path: String,
    /// Shared git toplevel for workspace scope, else empty.
    pub ws: String,
    /// Original lines without trailing newlines.
    pub lines: Vec<String>,
    /// String-blanked lines aligned 1:1 with `lines`.
    ///
    /// Computed once at load through `stripped_lines` so every detector
    /// shares identical analysis text, including multi-line raw-string
    /// interiors; comments stay intact for comment-aware callers while code
    /// positions cut them as before. Display paths keep using `lines`.
    pub stripped: Vec<String>,
}

/// Original and blanked lines with indices for display-preserving scans.
///
/// The zipped pair keeps analysis on blanked text while hits display
/// original lines; indices align with both vectors.
pub fn indexed_lines(file: &SourceFile) -> impl Iterator<Item = (usize, (&String, &String))> {
    file.lines.iter().zip(file.stripped.iter()).enumerate()
}

/// True if `cmd` resolves in `PATH`.
#[must_use]
pub fn have(cmd: &str) -> bool {
    var_os("PATH").is_some_and(|paths| {
        split_paths(&paths).any(|dir| {
            let candidate = Path::new(&dir).join(cmd);
            candidate.is_file()
        })
    })
}

/// Single scan root covering every `.rs` file.
///
/// `rg --files` enumerates recursively from here, so excluded crates,
/// `fuzz/`, `xtask/`, and other non-standard layouts stay visible.
#[must_use]
pub fn discover_roots() -> Vec<String> {
    vec![".".to_owned()]
}

/// Shared git toplevel for workspace scope, if any.
///
/// Every scanned file shares this root so workspace-excluded crates correlate
/// with members; outside a git checkout the scope stays empty.
#[must_use]
pub fn git_root() -> String {
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_or_default(|out| {
            if out.status.success() {
                String::from_utf8_lossy(&out.stdout).trim().to_owned()
            } else {
                String::new()
            }
        })
}

/// Workspace scope for one scanned `path` under shared `root`.
///
/// Empty paths and empty roots stay empty, keeping the old package-only
/// verdict for unscopable files.
fn scope_for(path: &str, root: &str) -> String {
    if path.is_empty() || root.is_empty() {
        String::new()
    } else {
        root.to_owned()
    }
}

/// Run `rg` with `args`; returns stdout lines (empty on failure).
#[must_use]
pub fn rg_lines(args: &[&str]) -> Vec<String> {
    Command::new("rg")
        .args(args)
        .output()
        .map_or_default(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_owned)
                .collect()
        })
}

/// Run `rg` with a base argument list plus scan dirs.
fn rg_with_dirs(base: &[&str], dirs: &[String]) -> Vec<String> {
    let owned: Vec<String> = dirs.to_vec();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut args: Vec<&str> = base.to_vec();
    args.extend(refs);
    rg_lines(&args)
}

/// Line without a leading `./` search-root marker, if present.
///
/// `rg` echoes an explicit `.` search root into every path; stripping it
/// restores the historical `src/…` format and keeps `starts_with` predicates
/// and package grouping exact.
fn strip_dot_prefix(line: &str) -> &str {
    line.strip_prefix("./").unwrap_or(line)
}

/// `rg` file listing for `dirs`.
///
/// Skips `target/` and `vendor/` explicitly; `rg` already respects
/// `.gitignore` for the rest. Leading `./` markers stay stripped so paths
/// read like historical scan-root-relative hits.
#[must_use]
pub fn rg_files(dirs: &[String]) -> Vec<String> {
    rg_with_dirs(
        &[
            "--files",
            "--glob",
            "*.rs",
            "--glob",
            "!target/**",
            "--glob",
            "!vendor/**",
        ],
        dirs,
    )
    .into_iter()
    .map(|l| strip_dot_prefix(l.trim()).to_owned())
    .filter(|l| !l.is_empty())
    .collect()
}

/// `rg -n` search with literal (`-F`) or PCRE (`-P`) pattern.
///
/// Skips `target/` and `vendor/` explicitly; `rg` already respects
/// `.gitignore` for the rest. Leading `./` markers stay stripped so hit
/// paths read like historical scan-root-relative hits.
#[must_use]
pub fn rg_search(flag: &str, pattern: &str, dirs: &[String]) -> Vec<String> {
    rg_with_dirs(
        &[
            "-n",
            "--glob",
            "*.rs",
            "--glob",
            "!target/**",
            "--glob",
            "!vendor/**",
            flag,
            pattern,
        ],
        dirs,
    )
    .into_iter()
    .map(|l| strip_dot_prefix(&l).to_owned())
    .collect()
}

/// Read every `path`, skipping unreadable files like the legacy script did.
///
/// Every file shares the git toplevel as its workspace scope so excluded
/// crates correlate with members.
#[must_use]
pub fn load_files(paths: &[String]) -> Vec<SourceFile> {
    let root = git_root();
    paths
        .iter()
        .filter_map(|path| {
            read_to_string(path).map_or(None, |text| {
                let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                let stripped = stripped_lines(&lines);
                Some(SourceFile {
                    path: path.clone(),
                    ws: scope_for(path, &root),
                    lines,
                    stripped,
                })
            })
        })
        .collect()
}
