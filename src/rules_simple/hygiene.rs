//! Low-level hygiene detectors for error swallows and paths.
//!
//! Parent index for the capability. Shared line scanning and marker helpers
//! live here while `dot_ok` flags error swallows, `parent_path` flags
//! `super::` paths, `scoped_pub` flags restricted visibility, `path_include`
//! flags path attributes, `low_log` flags low-level logging, `doc_blank`
//! flags missing blanks above docs, and `split_mod_doc` flags split docs.

pub mod doc_blank;
pub mod dot_ok;
pub mod low_log;
pub mod parent_path;
pub mod path_include;
pub mod scoped_pub;
pub mod split_mod_doc;

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    scan::{SourceFile, indexed_lines},
};

/// Collect findings for lines matching `pred` after blanking.
///
/// Iterates precomputed analysis lines with original lines zipped for
/// display, so multi-line raw-string interiors never scan as code.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
/// * `pred` - Predicate over the string-blanked code tail.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn line_hits(files: &[SourceFile], pred: fn(&str) -> bool) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file).filter_map(|(idx, (line, code))| {
                let stripped = strip_strings(code);
                let tail = cut_line_comment(&stripped);
                pred(tail).then(|| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
            })
        })
        .collect()
}

/// True if `marker` opens at `i` with a gap before it.
///
/// # Arguments
///
/// * `code` - String-blanked code tail.
/// * `chars` - Character view of `code`.
/// * `i` - Byte-adjacent char index to test.
/// * `marker` - Keyword to match.
///
/// # Returns
///
/// * `bool` - True when the marker matches with a boundary before it.
#[must_use]
pub fn marker_at(code: &str, chars: &[char], i: usize, marker: &str) -> bool {
    code.get(i..).is_some_and(|rest| rest.starts_with(marker)) && gap_before_at(chars, i)
}

/// True if `c` continues an identifier.
const fn is_ident_char_at(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if the char before `i` ends any identifier run.
fn gap_before_at(chars: &[char], i: usize) -> bool {
    i == 0
        || chars
            .get(i.saturating_sub(1))
            .is_some_and(|p| !is_ident_char_at(*p))
}
