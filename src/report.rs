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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs::read_to_string};

    #[test]
    fn docs_single_source() {
        let readme_text = read_to_string("README.md").unwrap_or_default();
        let agents_text = read_to_string("AGENTS.md").unwrap_or_default();
        let entry_text = read_to_string("src/main.rs").unwrap_or_default();
        let rules_text = read_to_string("src/rules_harness.rs").unwrap_or_default();
        let annotation_text = read_to_string("src/annotation_rules.rs").unwrap_or_default();
        assert!(!readme_text.is_empty(), "readme loads");
        assert!(!agents_text.is_empty(), "agents loads");
        assert!(!entry_text.is_empty(), "entry loads");
        assert!(!rules_text.is_empty(), "rules load");
        assert!(!annotation_text.is_empty(), "annotations load");
        let slugs: Vec<String> = readme_text
            .lines()
            .map(str::trim_start)
            .filter_map(|trimmed| trimmed.strip_prefix("| `"))
            .filter(|rest| rest.contains('`'))
            .map(|rest| rest.split('`').next().unwrap_or_default())
            .filter(|slug| !slug.is_empty())
            .map(str::to_owned)
            .collect();
        assert!(!slugs.is_empty(), "lints table parses");
        let unique: BTreeSet<&str> = slugs.iter().map(String::as_str).collect();
        assert_eq!(unique.len(), slugs.len(), "lints table stays unique");
        let readme_norm: BTreeSet<String> = slugs
            .iter()
            .map(|slug| {
                slug.strip_suffix("_hits")
                    .unwrap_or(slug.as_str())
                    .to_owned()
            })
            .collect();
        let combined = format!("{entry_text}{rules_text}{annotation_text}");
        let mut found: Vec<String> = combined
            .match_indices('&')
            .map(|pair| pair.0)
            .map(|pos| {
                let trimmed = combined
                    .get(pos..)
                    .map_or("", |rest| rest.strip_prefix('&').unwrap_or(""));
                let ident: String = trimmed
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                (trimmed, ident)
            })
            .filter(|pair| {
                !pair.1.is_empty()
                    && pair
                        .1
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase())
                    && pair
                        .0
                        .get(pair.1.len()..)
                        .is_some_and(|tail| tail.trim_start().starts_with('('))
                    && !matches!(
                        pair.1.as_str(),
                        "cfg_test_files"
                            | "load_manifests"
                            | "rg_files"
                            | "rg_manifests"
                            | "rg_search"
                            | "rg_ui_files"
                    )
            })
            .map(|pair| pair.1)
            .collect();
        if entry_text.contains("clone_check") {
            found.push("clone_check".to_owned());
        }
        let wired_norm: BTreeSet<String> = found
            .iter()
            .map(|name| {
                name.strip_suffix("_hits")
                    .unwrap_or(name.as_str())
                    .to_owned()
            })
            .collect();
        assert_eq!(wired_norm, readme_norm, "readme covers wired detectors");
        let mut spans = agents_text.split('`').skip(1).step_by(2).filter(|part| {
            let base = part.strip_suffix("_hits").unwrap_or(part);
            readme_norm.contains(base)
        });
        assert!(spans.next().is_none(), "agents stays free of lint names");
    }
}
