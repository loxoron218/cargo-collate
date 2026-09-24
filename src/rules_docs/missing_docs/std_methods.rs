//! Well-known standard-trait methods needing no per-item docs.
//!
//! Split from the missing-docs driver to keep single-file detectors under
//! the project file-length limit.

/// True if `name` is a well-known standard-trait method needing no docs.
///
/// Contracts for `Default`, `Display`/`Debug`, `Clone`/`ToOwned`, conversions
/// (`From`/`Into`/`TryFrom`/`TryInto`/`FromStr`/`ToString`), comparisons
/// (`PartialEq`/`Eq`/`PartialOrd`/`Ord`), hashing (`Hash`), `Drop`, borrow
/// traits (`Borrow`/`BorrowMut`/`AsRef`/`AsMut`/`Deref`/`DerefMut`) live in
/// std (nightly std trait index, 1.100.0);
/// per-item docs only restate them. Matching is by exact name.
#[must_use]
pub fn is_std_method(name: &str) -> bool {
    matches!(
        name,
        "default"
            | "fmt"
            | "write_str"
            | "write_fmt"
            | "clone"
            | "clone_from"
            | "clone_into"
            | "to_owned"
            | "to_string"
            | "from"
            | "into"
            | "try_from"
            | "try_into"
            | "from_str"
            | "from_iter"
            | "into_iter"
            | "hash"
            | "eq"
            | "ne"
            | "lt"
            | "le"
            | "gt"
            | "ge"
            | "cmp"
            | "partial_cmp"
            | "max"
            | "min"
            | "clamp"
            | "drop"
            | "deref"
            | "deref_mut"
            | "borrow"
            | "borrow_mut"
            | "as_ref"
            | "as_mut"
    )
}
