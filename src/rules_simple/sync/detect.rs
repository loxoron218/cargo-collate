//! Byte matchers for explicit `std::sync` lock paths.

/// Sync primitives that must come from `parking_lot` instead of `std`.
pub const SYNC_ITEMS: [&str; 3] = ["Mutex", "RwLock", "Condvar"];

/// Thread-unsafe cell primitives; use atomics, `parking_lot`, or `OnceLock` instead.
pub const CELL_ITEMS: [&str; 7] = [
    "Cell",
    "RefCell",
    "UnsafeCell",
    "OnceCell",
    "LazyCell",
    "Ref",
    "RefMut",
];

/// Thread-unsafe ref-counted pointers; use `Arc` or `std::sync::Weak` instead.
pub const RC_ITEMS: [&str; 2] = ["Rc", "Weak"];

/// Crate and module roots for thread-unsafe cell types.
pub const CELL_ROOTS: [(&str, &str); 2] = [("std", "cell"), ("core", "cell")];

/// Crate and module roots for thread-unsafe ref-counted types.
pub const RC_ROOTS: [(&str, &str); 2] = [("std", "rc"), ("alloc", "rc")];

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

/// True if `root` and `module` open at `start`, returning the index after them.
fn parse_root_at(bytes: &[u8], start: usize, root: &str, module: &str) -> Option<usize> {
    let after_root = match_word(bytes, start, root)?;
    let after_first = expect_colons(bytes, after_root)?;
    let after_mod = match_word(bytes, after_first, module)?;
    expect_colons(bytes, after_mod)
}

/// True if any of `items` opens at `after` with a boundary after it.
fn matches_item(bytes: &[u8], after: usize, items: &[&str]) -> bool {
    items
        .iter()
        .any(|name| match_word(bytes, after, name).is_some())
}

/// True if any thread-unsafe item opens at `start` under its roots.
fn parse_thread_at(bytes: &[u8], start: usize) -> bool {
    for (root, module) in CELL_ROOTS {
        let Some(after) = parse_root_at(bytes, start, root, module) else {
            continue;
        };
        if matches_item(bytes, after, &CELL_ITEMS) {
            return true;
        }
    }
    for (root, module) in RC_ROOTS {
        let Some(after) = parse_root_at(bytes, start, root, module) else {
            continue;
        };
        if matches_item(bytes, after, &RC_ITEMS) {
            return true;
        }
    }
    false
}

/// True if `code` holds an explicit thread-unsafe `cell` or `rc` path.
#[must_use]
pub fn has_thread_unsafe(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if is_word_start(bytes, i) && parse_thread_at(bytes, i) {
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
    use crate::rules_simple::sync::detect::{has_std_sync_lock, has_thread_unsafe};

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

    #[test]
    fn thread_unsafe_paths_flagged() {
        assert!(
            has_thread_unsafe("let c = std::cell::Cell::new(1);"),
            "cell flags"
        );
        assert!(
            has_thread_unsafe("let r = std::cell::RefCell::new(1);"),
            "refcell flags"
        );
        assert!(
            has_thread_unsafe("let r = ::std::rc::Rc::new(1);"),
            "leading colons flag"
        );
        assert!(
            has_thread_unsafe("let c = std :: cell :: Cell::new(1);"),
            "spaced separators flag"
        );
        assert!(
            has_thread_unsafe("let c = core::cell::Cell::new(1);"),
            "core cell flags"
        );
        assert!(
            has_thread_unsafe("let r = alloc::rc::Rc::new(1);"),
            "alloc rc flags"
        );
        assert!(
            has_thread_unsafe("let o = std::cell::OnceCell::new();"),
            "oncecell flags"
        );
        assert!(
            has_thread_unsafe("let w = std::rc::Weak::new();"),
            "weak flags"
        );
        assert!(
            !has_thread_unsafe("let a = std::sync::Arc::new(1);"),
            "arc stays silent"
        );
        assert!(
            !has_thread_unsafe("let o = std::sync::OnceLock::new();"),
            "oncelock stays silent"
        );
        assert!(
            !has_thread_unsafe("let c = parking_lot::Mutex::new(1);"),
            "parking_lot stays silent"
        );
        assert!(
            !has_thread_unsafe("let m = tokio::sync::Mutex::new(1);"),
            "tokio stays silent"
        );
        assert!(
            !has_thread_unsafe("let cell = 1;"),
            "bare words stay silent"
        );
        assert!(!has_thread_unsafe("let rc = 1;"), "lowercase stays silent");
        assert!(
            !has_thread_unsafe("let c = MyCell::new(1);"),
            "prefixed stays silent"
        );
    }
}
