//! Annotation and doc-section rule aggregation.
//!
//! Hosts `check_doc_rules` so the binary entry stays under the file limit.

use crate::{
    report::show,
    rules_annotations::{
        args_returns::doc_args_returns,
        missing_headers::{missing_fn_docs, module_header::missing_mod_docs},
        missing_sections::{missing_errors, missing_panics},
        must_use::unnecessary_must_use,
        pub_mod_inner::pub_mod_inner_docs,
        sections::{unnecessary_errors, unnecessary_panics},
    },
    rules_harness::suite_comments::unnecessary_test_docs,
    rules_typedef::unnecessary_types,
    scan::SourceFile,
};

/// Unnecessary attributes and doc sections removable warning-free.
///
/// # Arguments
///
/// * `output` - Report buffer receiving formatted findings.
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// True when any doc rule hits.
pub fn check_doc_rules(output: &mut String, files: &[SourceFile]) -> bool {
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
        "Public modules with inner //! docs; move to /// above attributes.",
        &pub_mod_inner_docs(files),
    );
    fail |= show(
        output,
        "Unnecessary must_use attributes; private, unit, mut-arg, or forwarding items never \
         require them.",
        &unnecessary_must_use(files),
    );
    fail |= show(
        output,
        "Unnecessary Panics sections; pure items and private test items never require them.",
        &unnecessary_panics(files),
    );
    fail |= show(
        output,
        "Unnecessary Errors sections; non-Result items and private test items never require them.",
        &unnecessary_errors(files),
    );
    fail |= show(
        output,
        "Missing Panics sections; private non-test panic-capable functions require them.",
        &missing_panics(files),
    );
    fail |= show(
        output,
        "Missing Errors sections; private non-test Result functions require them.",
        &missing_errors(files),
    );
    fail |= show(
        output,
        "Comments on private test-module functions; shared pub helpers stay documented.",
        &unnecessary_test_docs(files),
    );
    fail |= show(
        output,
        "Documented arguments/returns mismatch signature; sync # Arguments bullets and # Returns \
         with the fn signature.",
        &doc_args_returns(files),
    );
    fail |= show(
        output,
        "Unnecessary type definitions; aliases removable warning-free at every use site never \
         require them.",
        &unnecessary_types(files),
    );
    fail
}
