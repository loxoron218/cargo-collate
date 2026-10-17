//! Low-level logging detectors.

use crate::{
    lexer::word::has_word,
    rules_simple::hygiene::{line_hits, marker_at},
    scan::SourceFile,
};

/// True if `code` holds low-level logging outside strings.
///
/// # Arguments
///
/// * `code` - String-blanked code tail without line comments.
///
/// # Returns
///
/// * `bool` - True when `trace!` or `debug!` appears.
#[must_use]
pub fn has_low_log(code: &str) -> bool {
    for marker in ["trace", "debug"] {
        if !has_word(code, marker) {
            continue;
        }
        let chars: Vec<char> = code.chars().collect();
        let found = (0..chars.len()).any(|i| {
            marker_at(code, &chars, i, marker)
                && chars.get(i.saturating_add(marker.len())) == Some(&'!')
        });
        if found {
            return true;
        }
    }
    false
}

/// Low-level logging findings across files, strings and tails ignored.
///
/// # Arguments
///
/// * `files` - Scanned source files.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines.
pub fn low_log_hits(files: &[SourceFile]) -> Vec<String> {
    line_hits(files, has_low_log)
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_simple::hygiene::low_log::{has_low_log, low_log_hits},
        scan::builders::test_file,
    };

    #[test]
    fn low_logs_detected() {
        assert!(has_low_log("trace!(\"x\")"));
        assert!(has_low_log("debug!(\"x\")"));
        assert!(!has_low_log("info!(\"x\")"));
    }

    #[test]
    fn clean_files_silent() {
        let files = vec![test_file("src/a.rs", &["info!(x, \"ok\");"])];
        assert!(low_log_hits(&files).is_empty(), "info stays silent");
    }
}
