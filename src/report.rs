//! Finding sections, misuse reports, and clone detection.
//!
//! Reporting stays apart from rule aggregation so the binary entry keeps
//! orchestrating checks while this module owns output shapes and the
//! external `jscpd` detector.

use std::{
    env::args_os,
    ffi::OsString,
    io::{Write, stderr},
    process::Command,
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
#[must_use]
pub fn report_misuse(message: &str) -> i32 {
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
#[must_use]
pub fn clone_check(dirs: &[String]) -> (Vec<String>, bool) {
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
#[must_use]
pub fn user_args() -> Vec<OsString> {
    let args: Vec<OsString> = args_os().skip(1).collect();
    let drop_first: bool = args.first().is_some_and(|first| first == "collate");
    if drop_first {
        args.into_iter().skip(1).collect()
    } else {
        args
    }
}
