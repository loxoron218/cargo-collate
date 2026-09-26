//! File aggregation for thread-unsafe cell and rc findings.

use crate::{
    rules_simple::sync::{
        detect::has_thread_unsafe,
        hits::{check_body_with, collect_names, has_bare_name},
        locks::use_blocks_with_thread_unsafe,
    },
    scan::SourceFile,
};

/// True if `code` uses any imported thread-unsafe name bare.
fn bare_thread_use(code: &str, thread_names: &[String]) -> bool {
    thread_names.iter().any(|name| has_bare_name(code, name))
}

/// True if `tail` violates the thread-unsafe rule under imported `names`.
fn line_violates(tail: &str, names: &[String]) -> bool {
    has_thread_unsafe(tail) || (!names.is_empty() && bare_thread_use(tail, names))
}

/// Thread-unsafe `Cell`, `RefCell`, and `Rc` uses; use atomics, `parking_lot`, or `Arc` instead.
#[must_use]
pub fn thread_unsafe_hits(files: &[SourceFile]) -> Vec<String> {
    let mut out = Vec::new();
    for file in files {
        check_file_thread_unsafe(file, &mut out);
    }
    out
}

/// Append thread-unsafe findings of one file to `out`.
fn check_file_thread_unsafe(file: &SourceFile, out: &mut Vec<String>) {
    let (blocks, per_block) = use_blocks_with_thread_unsafe(file);
    let names = collect_names(file, &blocks, per_block, out);
    check_body_with(file, &blocks, &names, out, line_violates);
}

#[cfg(test)]
mod tests {
    use crate::{rules_simple::sync::thread_hits::thread_unsafe_hits, scan::support::test_file};

    #[test]
    fn thread_imports_flagged() {
        for line in [
            "use std::cell::Cell;",
            "use std::cell::RefCell;",
            "use std::cell::UnsafeCell;",
            "use std::cell::OnceCell;",
            "use std::cell::LazyCell;",
            "use std::cell::{Cell, RefCell};",
            "use std::{cell::Cell};",
            "use core::cell::RefCell;",
            "use std::rc::Rc;",
            "use std::rc::Weak;",
            "use alloc::rc::Rc;",
            "use ::std::cell::Cell;",
            "use std::cell::Cell as MyCell;",
            "use std::cell::*;",
            "use std::rc::*;",
        ] {
            let files = vec![test_file("src/a.rs", &[line])];
            assert_eq!(thread_unsafe_hits(&files).len(), 1, "{line} flags");
        }
    }

    #[test]
    fn allowed_thread_safe_silent() {
        let arc = vec![test_file(
            "src/a.rs",
            &["use std::sync::Arc;", "let a = Arc::new(1);"],
        )];
        assert!(thread_unsafe_hits(&arc).is_empty(), "arc stays silent");
        let oncelock = vec![test_file(
            "src/a.rs",
            &["use std::sync::OnceLock;", "let o = OnceLock::new();"],
        )];
        assert!(
            thread_unsafe_hits(&oncelock).is_empty(),
            "oncelock stays silent"
        );
        let parking = vec![test_file(
            "src/a.rs",
            &["use parking_lot::Mutex;", "let m = Mutex::new(1);"],
        )];
        assert!(
            thread_unsafe_hits(&parking).is_empty(),
            "parking_lot stays silent"
        );
        let bare = vec![test_file("src/a.rs", &["let c: Cell = Cell::new(1);"])];
        assert!(
            thread_unsafe_hits(&bare).is_empty(),
            "bare without import stays silent"
        );
    }

    #[test]
    fn bare_names_follow_imports() {
        let files = vec![test_file(
            "src/a.rs",
            &["use std::cell::Cell;", "let c: Cell = Cell::new(1);"],
        )];
        assert_eq!(thread_unsafe_hits(&files).len(), 2, "import and use flag");
        let rc = vec![test_file(
            "src/a.rs",
            &["use std::rc::Rc;", "let r: Rc<i32> = Rc::new(1);"],
        )];
        assert_eq!(thread_unsafe_hits(&rc).len(), 2, "rc flags");
        let aliased = vec![test_file(
            "src/a.rs",
            &[
                "use std::cell::Cell as MyCell;",
                "let c: MyCell = MyCell::new(1);",
            ],
        )];
        assert_eq!(thread_unsafe_hits(&aliased).len(), 2, "alias and use flag");
        let shadow = vec![test_file(
            "src/a.rs",
            &[
                "use std::cell::Cell as MyCell;",
                "let c: Cell = Cell::new(1);",
            ],
        )];
        assert_eq!(
            thread_unsafe_hits(&shadow).len(),
            1,
            "unaliased bare silent"
        );
    }

    #[test]
    fn qualified_other_crates_silent() {
        let mixed = vec![test_file(
            "src/a.rs",
            &["use std::cell::Cell;", "let c = foo::Cell::new(1);"],
        )];
        assert_eq!(
            thread_unsafe_hits(&mixed).len(),
            1,
            "other paths stay silent"
        );
        let sync = vec![test_file(
            "src/a.rs",
            &["use std::rc::Rc;", "let a = std::sync::Arc::new(1);"],
        )];
        assert_eq!(thread_unsafe_hits(&sync).len(), 1, "arc stays silent");
    }

    #[test]
    fn multiline_and_qualified_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["use std::cell::{", "    Cell,", "    RefCell,", "};"],
        )];
        assert_eq!(thread_unsafe_hits(&files).len(), 1, "multiline flags once");
        let qualified = vec![test_file(
            "src/a.rs",
            &["fn f() {", "    let c = std::cell::Cell::new(1);", "}"],
        )];
        assert_eq!(thread_unsafe_hits(&qualified).len(), 1, "qualified flags");
        let rc_qualified = vec![test_file(
            "src/a.rs",
            &["fn f() {", "    let r = alloc::rc::Rc::new(1);", "}"],
        )];
        assert_eq!(
            thread_unsafe_hits(&rc_qualified).len(),
            1,
            "alloc qualified flags"
        );
    }

    #[test]
    fn strings_comments_locals_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                r#"let s = "std::cell::Cell";"#,
                "let x = 1; // use std::cell::Cell;",
            ],
        )];
        assert!(
            thread_unsafe_hits(&files).is_empty(),
            "strings and tails stay silent"
        );
        let local = vec![test_file(
            "src/a.rs",
            &[
                "use std::cell::Cell;",
                "struct Cell;",
                "let c: Cell = Cell::new();",
            ],
        )];
        assert_eq!(
            thread_unsafe_hits(&local).len(),
            1,
            "locals exempt bare uses"
        );
    }
}
