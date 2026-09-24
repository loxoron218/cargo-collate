//! Inner `#![cfg(test)]` gates on files.
//!
//! Whole-file inner gates duplicate the parent `#[cfg(test)]` file-module
//! gate or smuggle test-only files past it; test code lives in a trailing
//! `#[cfg(test)] mod tests` block instead, so any inner gate flags.

use crate::{
    rules_tests::range::code_of,
    scan::{SourceFile, indexed_lines},
};

/// True if `stripped` gates the whole file behind `#![cfg(test)]`.
///
/// Whitespace inside the attribute stays ignored while strings, line tails,
/// and block-comment markers keep the line silent like the outer detector.
#[must_use]
pub fn is_real_inner_cfg_line(stripped: &str) -> bool {
    let code = code_of(stripped);
    if code.contains("/*") || code.contains("*/") {
        return false;
    }
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("#![cfg(test)]")
}

/// Inner `#![cfg(test)]` gates in any files.
///
/// Every file gates test code through a trailing `#[cfg(test)] mod tests`
/// block with parent file modules gated from above instead.
#[must_use]
pub fn inner_test_gates(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file)
                .filter(|(_, (_, stripped))| is_real_inner_cfg_line(stripped))
                .map(|(idx, (line, _))| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_tests::inner_gate::{inner_test_gates, is_real_inner_cfg_line},
        scan::support::test_file,
    };

    #[test]
    fn spaced_gate_flagged() {
        assert!(is_real_inner_cfg_line("#![cfg(test)]"));
        assert!(
            is_real_inner_cfg_line("#! [ cfg ( test ) ]"),
            "whitespace stays ignored"
        );
        let files = vec![test_file(
            "src/a.rs",
            &["//! Docs.", "", "#![cfg(test)]", "fn helper() {}"],
        )];
        assert_eq!(
            inner_test_gates(&files),
            vec!["src/a.rs:3:#![cfg(test)]".to_owned()],
            "gate line flagged"
        );
    }

    #[test]
    fn lookalikes_silent() {
        assert!(
            !is_real_inner_cfg_line("#[cfg(test)]"),
            "outer stays silent"
        );
        assert!(
            !is_real_inner_cfg_line("#![cfg(not(test))]"),
            "negation stays silent"
        );
        assert!(
            !is_real_inner_cfg_line("let s = \"#![cfg(test)]\";"),
            "string contents stay silent"
        );
        assert!(
            !is_real_inner_cfg_line("// #![cfg(test)]"),
            "line tails stay silent"
        );
        assert!(
            !is_real_inner_cfg_line("/* #![cfg(test)] */"),
            "block markers stay silent"
        );
        let files = vec![test_file("src/a.rs", &["#[cfg(test)]", "mod tests {", "}"])];
        assert!(
            inner_test_gates(&files).is_empty(),
            "inline tests stay silent"
        );
    }
}
