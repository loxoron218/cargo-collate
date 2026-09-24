//! Underscore-identifier filtering for ripgrep prefilter hits.
//!
//! Split from the text-rules index to keep single-file detectors under the
//! project file-length limit.

use crate::{
    lexer::{literal::strip_strings, word::has_underscore_ident},
    scan::SourceFile,
};

/// Re-filter underscore prefilter hits after blanking string literals.
///
/// Looks each hit up in its file's precomputed blanked lines so identifiers
/// inside multi-line raw strings stay silent; falls back to the hit content
/// when the file or line is unknown.
#[must_use]
pub fn underscore_idents(rg_hits: &[String], files: &[SourceFile]) -> Vec<String> {
    rg_hits
        .iter()
        .filter(|hit| underscore_hit(hit, files))
        .cloned()
        .collect()
}

/// True if an rg hit names an underscore identifier outside strings.
fn underscore_hit(hit: &str, files: &[SourceFile]) -> bool {
    let mut parts = hit.splitn(3, ':');
    let (Some(path), Some(num), Some(content)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let lineno: usize = num.parse().unwrap_or(0);
    if let Some(code) = files
        .iter()
        .find(|file| file.path == path)
        .and_then(|file| file.stripped.get(lineno.saturating_sub(1)))
    {
        return has_underscore_ident(code);
    }
    has_underscore_ident(&strip_strings(content))
}
