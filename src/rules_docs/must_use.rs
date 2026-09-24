//! Unnecessary `#[must_use]` attributes.
//!
//! Mirrors the syntactically visible skip conditions of
//! `must_use_candidate` and `return_self_not_must_use`: scoped visibility
//! (`pub(crate)`/… counts as private since `is_exported()` is false), private
//! items, unit or never returns, named `&mut` or `*mut` parameters (which the
//! lint skips unconditionally, while a bare `_` parameter stays ignored
//! exactly like in clippy), and bodies forwarding non-local paths as call
//! arguments (which count as side effects).
//!
//! Type-level `#[must_use]` never flags (it drives `is_must_use_ty`).
//! Bare-`Self` methods with a receiver never flag (`return_self` still fires).
//!
//! Deliberately unflagged: methods on `!Freeze` types (locks or channels in
//! `Self`, e.g. getters on state holders). `has_mutable_arg` skips those too,
//! but that needs type resolution no syntactic checker can do.

use crate::{
    lexer::word::has_word,
    rules_docs::{
        code_of, is_pub_sig, is_pub_trait_fn, item_args::body_forwards_item, joined_signature,
        return_tail, sig_returns_unit,
    },
    scan::{SourceFile, indexed_lines},
};

/// True if stripped `code` holds a `must_use` attribute.
fn has_must_use(code: &str) -> bool {
    code.contains("#[") && has_word(code, "must_use")
}

/// True if `c` opens a bracket level.
const fn is_bracket_opener(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '<')
}

/// True if `c` closes a bracket level.
const fn is_bracket_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '>')
}

/// Parameter list of `sig` between the first `(` and its match, if any.
fn params_of(sig: &str) -> Option<String> {
    let open = sig.find('(')?;
    let mut depth: i32 = 0;
    for (k, c) in sig.get(open..)?.char_indices() {
        if is_bracket_opener(c) {
            depth = depth.saturating_add(1);
            continue;
        }
        if is_bracket_closer(c) {
            depth = depth.saturating_sub(1);
        }
        if depth <= 0 {
            return sig
                .get(open.saturating_add(1)..open.saturating_add(k))
                .map(str::to_owned);
        }
    }
    None
}

/// Split `params` on top-level commas.
fn split_params(params: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (k, c) in params.char_indices() {
        if is_bracket_opener(c) {
            depth = depth.saturating_add(1);
            continue;
        }
        if is_bracket_closer(c) {
            depth = depth.saturating_sub(1);
            continue;
        }
        if c == ',' && depth <= 0 {
            out.push(params.get(start..k).unwrap_or(""));
            start = k.saturating_add(1);
        }
    }
    out.push(params.get(start..).unwrap_or(""));
    out
}

/// True if `piece` is a bare wildcard parameter, which clippy ignores.
fn is_wild_param(piece: &str) -> bool {
    let trimmed = piece.trim();
    let named = trimmed
        .strip_prefix("mut")
        .filter(|rest| rest.starts_with(|c: char| c.is_whitespace()))
        .map_or(trimmed, str::trim_start);
    named == "_" || named.starts_with("_:")
}

/// Index after ASCII whitespace from `i`.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i = i.saturating_add(1);
    }
    i
}

/// Index after a lifetime annotation from `i`, if any.
fn skip_lifetime(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i) != Some(&b'\'') {
        return i;
    }
    let mut j = i.saturating_add(1);
    while bytes
        .get(j)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        j = j.saturating_add(1);
    }
    j
}

/// True if `piece` holds an `&mut` or `*mut` token with a trailing boundary.
///
/// Lifetimes between `&` and `mut` (as in `&'a mut T`) stay covered, while
/// `*const` and lookalikes such as `&mutual` stay excluded.
fn has_mut_ptr(piece: &str) -> bool {
    let bytes = piece.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some(c) = bytes.get(i) else {
            break;
        };
        if *c != b'&' && *c != b'*' {
            i = i.saturating_add(1);
            continue;
        }
        let mut j = i.saturating_add(1);
        if *c == b'&' {
            j = skip_ws(bytes, j);
            j = skip_lifetime(bytes, j);
            j = skip_ws(bytes, j);
        }
        if piece.get(j..).is_some_and(|rest| rest.starts_with("mut"))
            && bytes
                .get(j.saturating_add(3))
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
        {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True if `sig` takes a named `&mut` or `*mut` parameter.
fn sig_has_mut_param(sig: &str) -> bool {
    params_of(sig).is_some_and(|params| {
        split_params(&params)
            .iter()
            .any(|piece| !is_wild_param(piece) && has_mut_ptr(piece))
    })
}

/// True if return tail of `sig` is bare `Self` (not `&Self`/`Option<Self>`).
fn returns_bare_self(sig: &str) -> bool {
    let Some(tail) = return_tail(sig) else {
        return false;
    };
    let tail = tail.trim_start();
    if !tail.starts_with("Self") {
        return false;
    }
    tail.as_bytes()
        .get(4)
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

/// True if `sig` takes a `self` receiver (`self`, `&self`, `&mut self`, …).
fn has_self_param(sig: &str) -> bool {
    params_of(sig).is_some_and(|params| has_word(&params, "self"))
}

/// True if `#[must_use]` at `idx` is removable warning-free.
///
/// Private items, unit or never returns, named mutable parameters, and bodies
/// forwarding non-local paths never require the attribute. Type-level
/// `#[must_use]` (struct/enum/trait/…) is the mechanism that makes
/// `is_must_use_ty` true and never flags. Bare-`Self`-returning methods with
/// a receiver are `return_self_not_must_use` candidates and stay silent even
/// with `&mut self` or forwarding bodies. Other public functions — including
/// methods of public traits, whose visibility is inherited — stay silent.
fn must_use_at(lines: &[String], idx: usize) -> bool {
    let sig = joined_signature(lines, idx.saturating_add(1), 12);
    if !has_word(&sig, "fn") {
        return false;
    }
    if !is_pub_sig(&sig) && !is_pub_trait_fn(lines, idx) {
        return true;
    }
    if has_self_param(&sig) && returns_bare_self(&sig) {
        return false;
    }
    sig_returns_unit(&sig) || sig_has_mut_param(&sig) || body_forwards_item(lines, idx)
}

/// Unnecessary `#[must_use]` on private, unit-returning, mutably-borrowing,
/// or item-forwarding items.
#[must_use]
pub fn unnecessary_must_use(files: &[SourceFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            indexed_lines(file)
                .filter(|(_, (_, code))| has_must_use(&code_of(code)))
                .filter(|(idx, _)| must_use_at(&file.stripped, *idx))
                .map(|(idx, (line, _))| format!("{}:{}:{line}", file.path, idx.saturating_add(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{rules_docs::must_use::unnecessary_must_use, scan::support::test_file};

    #[test]
    fn mut_param_flagged() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["#[must_use]", "pub fn f(x: &mut i32) -> i32 { x }"],
            ),
            test_file(
                "src/b.rs",
                &[
                    "#[must_use]",
                    "pub fn g(x: &'b mut Vec<f32>) -> usize { 1 }",
                ],
            ),
            test_file(
                "src/c.rs",
                &["#[must_use]", "pub fn h(p: *mut u8) -> u8 { 1 }"],
            ),
        ];
        assert_eq!(unnecessary_must_use(&files).len(), 3, "mut params flag");
    }

    #[test]
    fn wild_and_const_ptr_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["#[must_use]", "pub fn f(_: &mut i32) -> i32 { 1 }"],
            ),
            test_file(
                "src/b.rs",
                &["#[must_use]", "pub fn g(p: *const u8) -> u8 { 1 }"],
            ),
            test_file(
                "src/c.rs",
                &["#[must_use]", "pub fn h(m: &mutual) -> i32 { 1 }"],
            ),
            test_file(
                "src/d.rs",
                &["#[must_use]", "pub fn k(mut x: i32) -> i32 { x }"],
            ),
        ];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "wildcards and const pointers stay silent"
        );
    }

    #[test]
    fn never_return_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["#[must_use]", "pub fn f() -> ! { loop {} }"],
        )];
        assert_eq!(unnecessary_must_use(&files).len(), 1, "never flags");
    }

    #[test]
    fn trait_method_silent() {
        let files = vec![test_file(
            "src/a.rs",
            &["pub trait Api {", "#[must_use]", "fn f() -> i32 { 1 }", "}"],
        )];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "public trait methods stay silent"
        );
    }

    #[test]
    fn type_level_silent() {
        let files = vec![
            test_file(
                "src/a.rs",
                &[
                    "#[must_use = \"query must be executed\"]",
                    "pub struct Query { x: i32 }",
                ],
            ),
            test_file("src/b.rs", &["#[must_use]", "pub enum IsNull { Yes, No }"]),
            test_file(
                "src/c.rs",
                &["#[must_use]", "pub struct Stream(StreamInner);"],
            ),
        ];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "type-level must_use is the mechanism, never flagged"
        );
    }

    #[test]
    fn scoped_pub_flagged() {
        let files = vec![test_file(
            "src/a.rs",
            &["#[must_use]", "pub(crate) fn f() -> i32 { 1 }"],
        )];
        assert_eq!(
            unnecessary_must_use(&files).len(),
            1,
            "scoped pub counts as private since candidate never fires"
        );
        let bare = vec![test_file(
            "src/b.rs",
            &["#[must_use]", "pub fn g() -> i32 { 1 }"],
        )];
        assert!(
            unnecessary_must_use(&bare).is_empty(),
            "bare pub stays silent"
        );
    }

    #[test]
    fn self_return_silent_despite_mut() {
        let files = vec![
            test_file(
                "src/a.rs",
                &["#[must_use]", "pub fn foo(&self) -> Self { Self }"],
            ),
            test_file(
                "src/b.rs",
                &["#[must_use]", "pub fn bar(&mut self) -> Self { Self }"],
            ),
            test_file(
                "src/c.rs",
                &["#[must_use]", "pub fn baz(self) -> Self { self }"],
            ),
        ];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "return_self candidates stay silent even with &mut self"
        );
        let ref_self = vec![test_file(
            "src/d.rs",
            &["#[must_use]", "pub fn qux(&self) -> &Self { self }"],
        )];
        assert!(
            unnecessary_must_use(&ref_self).is_empty(),
            "reference Self stays silent as pure candidate"
        );
    }

    #[test]
    fn raw_string_hides_forwarding() {
        let files = vec![test_file(
            "src/a.rs",
            &[
                "fn helper(x: i32) -> i32 { x }",
                "#[must_use]",
                "pub fn f(v: Vec<i32>) -> Vec<i32> {",
                "    let q = r#\"",
                "    v.into_iter().map(helper).collect()",
                "    \"#;",
                "    v",
                "}",
            ],
        )];
        assert!(
            unnecessary_must_use(&files).is_empty(),
            "calls inside raw strings never forward"
        );
        let live = vec![test_file(
            "src/b.rs",
            &[
                "fn helper(x: i32) -> i32 { x }",
                "#[must_use]",
                "pub fn g(v: Vec<i32>) -> Vec<i32> {",
                "    v.into_iter().map(helper).collect()",
                "}",
            ],
        )];
        assert_eq!(
            unnecessary_must_use(&live).len(),
            1,
            "real forwarding still releases the attribute"
        );
    }
}
