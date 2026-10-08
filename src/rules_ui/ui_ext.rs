//! Extension matching for declarative UI and CSS references.
//!
//! Holds the denied suffix set with bounded ASCII case-insensitive matching
//! so scanner states share one spelling of the widget rule.

/// Declarative extensions denied by the widget rule, already lowercase.
const UI_EXTENSIONS: [&str; 4] = ["ui", "blp", "xml", "css"];

/// True if `byte` continues an identifier.
///
/// # Arguments
///
/// * `byte` - Byte to classify.
///
/// # Returns
///
/// * `bool` - True for ASCII alphanumeric or underscore.
#[must_use]
pub const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True if `idx` starts a word after a non-identifier boundary.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate start index.
///
/// # Returns
///
/// * `bool` - True at the start or after a non-identifier byte.
#[must_use]
pub fn boundary_before(bytes: &[u8], idx: usize) -> bool {
    idx == 0
        || bytes
            .get(idx.saturating_sub(1))
            .is_none_or(|prev| !is_ident_byte(*prev))
}

/// True if `ext` matches at `start` with a trailing boundary.
///
/// Matches a leading dot plus ASCII case-insensitive letters, so `FOO.UI`
/// counts alongside `foo.ui` while `foo.uix` stays silent.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `start` - Index of the leading dot.
/// * `ext` - Lowercase extension without its dot.
///
/// # Returns
///
/// * `bool` - True on a full bounded match.
fn ext_matches_at(bytes: &[u8], start: usize, ext: &str) -> bool {
    if bytes.get(start) != Some(&b'.') {
        return false;
    }
    let wanted = ext.as_bytes();
    let mut offset = 0;
    while offset < wanted.len() {
        let actual = bytes.get(start.saturating_add(1).saturating_add(offset));
        let expected = wanted.get(offset);
        let same = actual
            .zip(expected)
            .is_some_and(|(got, want)| got.to_ascii_lowercase() == *want);
        if !same {
            return false;
        }
        offset = offset.saturating_add(1);
    }
    bytes
        .get(start.saturating_add(1).saturating_add(wanted.len()))
        .is_none_or(|next| !is_ident_byte(*next))
}

/// True if any denied extension opens at `idx`.
///
/// # Arguments
///
/// * `bytes` - Line bytes under scan.
/// * `idx` - Candidate index of the leading dot.
///
/// # Returns
///
/// * `bool` - True for `.ui`, `.blp`, `.xml`, or `.css` with a boundary.
#[must_use]
pub fn has_ui_ext_at(bytes: &[u8], idx: usize) -> bool {
    UI_EXTENSIONS
        .iter()
        .any(|ext| ext_matches_at(bytes, idx, ext))
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::ui_ext::{boundary_before, has_ui_ext_at, is_ident_byte};

    #[test]
    fn ident_bytes_classified() {
        assert!(is_ident_byte(b'a'));
        assert!(is_ident_byte(b'9'));
        assert!(is_ident_byte(b'_'));
        assert!(!is_ident_byte(b'.'));
        assert!(!is_ident_byte(b'"'));
    }

    #[test]
    fn boundaries_split_words() {
        let bytes = b"a b";
        assert!(boundary_before(bytes, 0));
        assert!(!boundary_before(bytes, 1));
        assert!(boundary_before(bytes, 2));
    }

    #[test]
    fn extensions_match_bounded() {
        assert!(has_ui_ext_at(b"a.ui", 1));
        assert!(has_ui_ext_at(b"a.UI", 1));
        assert!(!has_ui_ext_at(b"a.uix", 1));
        assert!(!has_ui_ext_at(b"a.cssx", 1));
        assert!(!has_ui_ext_at(b"aui", 1));
    }
}
