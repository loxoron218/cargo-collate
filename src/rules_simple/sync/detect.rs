//! Byte matchers for explicit `std::sync` lock paths.

/// Sync primitives that must come from `parking_lot` instead of `std`.
pub const SYNC_ITEMS: [&str; 3] = ["Mutex", "RwLock", "Condvar"];

/// True if `b` continues an ASCII identifier.
#[must_use]
pub const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Index after ASCII whitespace from `i`.
pub fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i = i.saturating_add(1);
    }
    i
}

/// Index past `::` from `pos` with surrounding whitespace allowed, if any.
fn expect_colons(bytes: &[u8], pos: usize) -> Option<usize> {
    let start = skip_ws(bytes, pos);
    if bytes.get(start) != Some(&b':') {
        return None;
    }
    let next = start.saturating_add(1);
    if bytes.get(next) != Some(&b':') {
        return None;
    }
    Some(skip_ws(bytes, next.saturating_add(1)))
}

/// Index past `word` at `pos` with an identifier boundary after it, if any.
fn match_word(bytes: &[u8], pos: usize, word: &str) -> Option<usize> {
    let end = pos.saturating_add(word.len());
    if bytes.get(pos..end) != Some(word.as_bytes()) {
        return None;
    }
    if bytes.get(end).is_some_and(|b| is_ident_byte(*b)) {
        return None;
    }
    Some(end)
}

/// True if `std::sync::Mutex`, `RwLock`, or `Condvar` opens at `start`.
fn parse_std_at(bytes: &[u8], start: usize) -> bool {
    let Some(after_std) = match_word(bytes, start, "std") else {
        return false;
    };
    let Some(after_first) = expect_colons(bytes, after_std) else {
        return false;
    };
    let Some(after_sync) = match_word(bytes, after_first, "sync") else {
        return false;
    };
    let Some(after_second) = expect_colons(bytes, after_sync) else {
        return false;
    };
    for name in SYNC_ITEMS {
        if match_word(bytes, after_second, name).is_some() {
            return true;
        }
    }
    false
}

/// True if `code` holds an explicit `std::sync` lock path.
#[must_use]
pub fn has_std_sync_lock(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if is_word_start(bytes, i) && parse_std_at(bytes, i) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True if `i` opens an identifier run instead of continuing one.
#[must_use]
pub fn is_word_start(bytes: &[u8], i: usize) -> bool {
    i == 0
        || bytes
            .get(i.saturating_sub(1))
            .is_none_or(|b| !is_ident_byte(*b))
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::sync::detect::has_std_sync_lock;

    #[test]
    fn explicit_paths_flagged() {
        assert!(
            has_std_sync_lock("let m = std::sync::Mutex::new(1);"),
            "qualified mutex flags"
        );
        assert!(
            has_std_sync_lock("let r = ::std::sync::RwLock::new(1);"),
            "leading colons flag"
        );
        assert!(
            has_std_sync_lock("let m = std :: sync :: Mutex::new(1);"),
            "spaced separators flag"
        );
        assert!(
            has_std_sync_lock("let c = std::sync::Condvar::new();"),
            "qualified condvar flags"
        );
        assert!(
            !has_std_sync_lock("let m = parking_lot::Mutex::new(1);"),
            "parking_lot stays silent"
        );
        assert!(
            !has_std_sync_lock("let c = parking_lot::Condvar::new();"),
            "parking_lot condvar stays silent"
        );
        assert!(
            !has_std_sync_lock("let m = tokio::sync::Mutex::new(1);"),
            "tokio stays silent"
        );
        assert!(
            !has_std_sync_lock("let o = std::sync::OnceLock::new();"),
            "oncelock stays silent"
        );
        assert!(
            !has_std_sync_lock("let g = std::sync::MutexGuard::new();"),
            "guards stay silent"
        );
        assert!(
            !has_std_sync_lock("let mutex = 1;"),
            "bare words stay silent"
        );
        assert!(
            !has_std_sync_lock("let condvar = 1;"),
            "lowercase stays silent"
        );
    }
}
