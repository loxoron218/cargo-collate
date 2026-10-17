//! Attribute attachment and `cfg`-gating for `use` statements.
//!
//! Leading `#[...]` blocks belong to the `use` they sit above, `#[cfg(...)]`
//! marks an in-group sub-block (unconditional first, all `cfg` predicates
//! lumped), and blanks inside one attributed statement flag. Attachment walks
//! reuse `mod_groups::attr` so the logic never clones across capabilities.

use crate::{
    lexer::word::{has_word, word_match_at},
    rules_harness::{
        mod_groups::attr::{
            attached_from, has_detached_block, is_pure_attr_text, resolve_trailing_attr_start,
        },
        range::code_of,
    },
    scan::SourceFile,
};

/// First line of the contiguous attribute blocks attached to the `use` at
/// `use_idx`, including a multi-line block closing on the `use` line itself.
#[must_use]
pub fn attached_start(file: &SourceFile, use_idx: usize) -> usize {
    attached_from(file, trailing_block_start(file, use_idx).unwrap_or(use_idx))
}

/// Byte index of a `use` head trailing an attribute on the same line, if any.
///
/// Matches shapes like `...)] use foo;` where the `#[...]` opened above; the
/// same-line prefix must hold a closing bracket, otherwise the direct
/// single-line match owns the case.
fn trailing_attr_use_pos(code: &str) -> Option<usize> {
    for (i, _) in code.match_indices("use") {
        if !word_match_at(code, "use", i) {
            continue;
        }
        if code.get(..i).is_some_and(|prefix| prefix.contains(']')) {
            return Some(i);
        }
    }
    None
}

/// Start line of a multi-line attribute closing on the `use` line itself, if
/// any.
fn trailing_block_start(file: &SourceFile, use_idx: usize) -> Option<usize> {
    let line = file.stripped.get(use_idx)?;
    let code = code_of(line);
    let use_pos = trailing_attr_use_pos(&code)?;
    let block_start = resolve_trailing_attr_start(file, use_idx, use_pos)?;
    let mut joined = String::new();
    for line_idx in block_start..use_idx {
        joined.push_str(&code_of(file.stripped.get(line_idx)?));
        joined.push('\n');
    }
    joined.push_str(code.get(..use_pos).unwrap_or(""));
    is_pure_attr_text(&joined).then_some(block_start)
}

/// True if the attached block `attr_start..=use_idx` gates the import with
/// `#[cfg(...)]`.
///
/// Only `cfg` gates existence; `cfg_attr` stays ordinary since the import
/// itself is always present. `has_word` gives the boundary so `cfg_attr`
/// never counts.
#[must_use]
pub fn is_cfg_gated(file: &SourceFile, attr_start: usize, use_idx: usize) -> bool {
    let mut joined = String::new();
    for idx in attr_start..=use_idx {
        let Some(line) = file.stripped.get(idx) else {
            return false;
        };
        joined.push_str(&code_of(line));
        joined.push('\n');
    }
    joined.contains("#[") && has_word(&joined, "cfg")
}

/// True when a pure attribute block sits above `attr_start` across a blank.
///
/// Any blank inside one attributed `use` statement splits it, mirroring the
/// detached-attribute check for file modules.
#[must_use]
pub fn has_detached_attr_gap(file: &SourceFile, attr_start: usize) -> bool {
    has_detached_block(file, attr_start, |_, _, _| true)
}

#[cfg(test)]
mod tests {
    use crate::{rules_imports::mixed_import_groups, scan::builders::test_file};

    #[test]
    fn credentials_shape_fixed_silent() {
        let files = vec![test_file(
            "src/credentials.rs",
            &[
                "use std::{",
                "    fs::{read_to_string, write},",
                "    io::Error,",
                "    path::Path,",
                "};",
                "",
                "#[cfg(unix)]",
                "use std::{",
                "    fs::{Permissions, set_permissions},",
                "    os::unix::fs::PermissionsExt,",
                "};",
                "",
                "use serde::{Deserialize, Serialize};",
                "",
                "use crate::errors::QobuzApiError::{self, CredentialsError};",
            ],
        )];
        assert!(
            mixed_import_groups(&files).is_empty(),
            "fixed credentials shape stays silent"
        );
    }

    #[test]
    fn credentials_shape_original_flagged() {
        let files = vec![test_file(
            "src/credentials.rs",
            &[
                "#[cfg(unix)]",
                "use std::{",
                "    fs::{Permissions, set_permissions},",
                "    os::unix::fs::PermissionsExt,",
                "};",
                "use std::{",
                "    fs::{read_to_string, write},",
                "    io::Error,",
                "    path::Path,",
                "};",
            ],
        )];
        assert!(
            !mixed_import_groups(&files).is_empty(),
            "cfg-first credentials shape flags unconditional-first"
        );
    }

    #[test]
    fn cfg_std_after_crate_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use std::a;",
                "",
                "use anyhow::b;",
                "",
                "use crate::c;",
                "",
                "#[cfg(unix)]",
                "use std::os::unix::fs::MetadataExt;",
            ],
        )];
        let hits = mixed_import_groups(&files);
        assert_eq!(hits.len(), 1, "cfg std after crate flags order");
        assert!(
            hits.first().is_some_and(|hit| hit.contains(":8:")),
            "cfg std line flagged"
        );
    }

    #[test]
    fn cfg_external_in_place_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use std::a;",
                "",
                "use anyhow::b;",
                "",
                "#[cfg(feature = \"gated\")]",
                "use serde::de;",
                "",
                "use crate::c;",
            ],
        )];
        assert!(
            mixed_import_groups(&files).is_empty(),
            "cfg external stays inside external group"
        );
    }

    #[test]
    fn cfg_different_predicates_lumped_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "use std::a;",
                "",
                "#[cfg(unix)]",
                "use std::b;",
                "#[cfg(windows)]",
                "use std::c;",
            ],
        )];
        assert!(
            mixed_import_groups(&files).is_empty(),
            "different cfg predicates lump contiguous"
        );
    }
}
