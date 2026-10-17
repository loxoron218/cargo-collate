//! Missing `# Errors` and `# Panics` doc sections on private functions.
//!
//! Complements `clippy::missing_errors_doc` and `missing_panics_doc` with
//! `check-private-items` semantics, but test code stays exempt: inline test
//! modules, test attributes, integration tests, parent-gated files, the
//! `main` entry, and standard-trait methods never flag.

use std::collections::HashSet;

use crate::{
    gating::gated_stems,
    lexer::word::{declared_fn_name, has_word},
    rules_annotations::{
        fn_body, is_pub_sig, joined_signature,
        missing_headers::{is_test_file, std_methods::is_std_method},
        pub_trait_ranges,
        sections::{body_can_panic, has_section, is_test_fn, returns_result},
    },
    rules_harness::range::{code_of, stray_gap, test_ranges},
    scan::{SourceFile, indexed_lines},
};

/// True if the `fn` at `pos` is private, inheriting trait visibility.
fn is_private_fn(traits: &[(usize, usize)], pos: usize, sig: &str) -> bool {
    if is_pub_sig(sig) {
        return false;
    }
    !traits.iter().any(|(a, b)| *a <= pos && pos < *b)
}

/// Private non-test signature at `idx`, if it needs section docs.
fn private_sig(
    file: &SourceFile,
    gated: &HashSet<String>,
    exempt: &[(usize, usize)],
    traits: &[(usize, usize)],
    idx: usize,
    code: &str,
) -> Option<String> {
    let body = code_of(code);
    if body.contains("/*") || body.contains("*/") {
        return None;
    }
    let name = declared_fn_name(&body)?;
    if name == "main" || is_std_method(&name) {
        return None;
    }
    if is_test_fn(file, gated, exempt, idx) {
        return None;
    }
    let sig = joined_signature(&file.stripped, idx, 12);
    if !has_word(&sig, "fn") {
        return None;
    }
    is_private_fn(traits, idx, &sig).then_some(sig)
}

/// True if `sig` plus `idx` body needs `wants_errors` section but lacks it.
fn needs_missing(file: &SourceFile, idx: usize, sig: &str, wants_errors: bool) -> bool {
    if wants_errors {
        if !returns_result(sig) {
            return false;
        }
        !has_section(&file.stripped, idx, "# Errors")
    } else if !body_can_panic(&fn_body(&file.stripped, idx)) {
        false
    } else {
        !has_section(&file.stripped, idx, "# Panics")
    }
}

/// Missing sections in one file; `wants_errors` selects the need check.
fn file_missing(file: &SourceFile, gated: &HashSet<String>, wants_errors: bool) -> Vec<String> {
    if is_test_file(file, gated) {
        return Vec::new();
    }
    let exempt = test_ranges(&file.lines, stray_gap);
    let traits = pub_trait_ranges(&file.stripped);
    indexed_lines(file)
        .filter_map(|(idx, (line, code))| {
            let sig = private_sig(file, gated, &exempt, &traits, idx, code)?;
            needs_missing(file, idx, &sig, wants_errors)
                .then(|| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        })
        .collect()
}

/// Missing sections for `wants_errors`; `true` selects `# Errors`.
fn missing_for(files: &[SourceFile], wants_errors: bool) -> Vec<String> {
    let gated = gated_stems(files);
    files
        .iter()
        .flat_map(|file| file_missing(file, &gated, wants_errors))
        .collect()
}

/// Missing `# Errors` on private non-test `Result` functions.
#[must_use]
pub fn missing_errors(files: &[SourceFile]) -> Vec<String> {
    missing_for(files, true)
}

/// Missing `# Panics` on private non-test panic-capable functions.
#[must_use]
pub fn missing_panics(files: &[SourceFile]) -> Vec<String> {
    missing_for(files, false)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_annotations::missing_sections::{missing_errors, missing_panics},
        scan::builders::test_file,
    };

    #[test]
    fn missing_private_flagged() {
        let errors = vec![test_file(
            "src/a.rs",
            &["/// D.", "fn f() -> Result<i32> { Ok(1) }"],
        )];
        assert_eq!(missing_errors(&errors).len(), 1, "private Result flags");
        let panics = vec![test_file("src/b.rs", &["/// D.", "fn f() { panic!(); }"])];
        assert_eq!(missing_panics(&panics).len(), 1, "private panic flags");
        let documented = vec![test_file(
            "src/c.rs",
            &["/// D.", "/// # Errors", "fn f() -> Result<i32> { Ok(1) }"],
        )];
        assert!(
            missing_errors(&documented).is_empty(),
            "documented stays silent"
        );
    }

    #[test]
    fn missing_test_silent() {
        let files = vec![
            test_file("src/a.rs", &["#[test]", "fn t() -> Result<i32> { Ok(1) }"]),
            test_file(
                "src/b.rs",
                &["#[cfg(test)]", "mod tests {", "fn h() { panic!(); }", "}"],
            ),
            test_file("tests/x.rs", &["fn h() -> Result<i32> { Ok(1) }"]),
            test_file("src/c.rs", &["fn main() -> Result<i32> { Ok(1) }"]),
        ];
        assert!(missing_errors(&files).is_empty(), "test stays silent");
        assert!(missing_panics(&files).is_empty(), "test stays silent");
    }

    #[test]
    fn missing_public_silent() {
        let files = vec![
            test_file("src/a.rs", &["pub fn f() -> Result<i32> { Ok(1) }"]),
            test_file("src/b.rs", &["pub fn g() { panic!(); }"]),
        ];
        assert!(missing_errors(&files).is_empty(), "public stays silent");
        assert!(missing_panics(&files).is_empty(), "public stays silent");
    }
}
