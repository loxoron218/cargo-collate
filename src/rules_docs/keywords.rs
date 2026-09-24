//! Rust keyword lists shared by doc-attribute detectors.
//!
//! Holds the complete keyword set so callers stay small enough to keep
//! single-file detectors under the project file-length limit.

/// True if `name` is a keyword (never a call callee).
///
/// Complete keyword set from the Rust Reference
/// (`doc.rust-lang.org/reference/keywords.html`): strict, reserved,
/// and weak keywords. Any of these before `(` opens control-flow, a
/// declaration, or a modifier — never a callable item — so the call scan
/// resumes past the paren instead of treating arguments as forwarded items.
#[must_use]
pub fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "match"
            | "for"
            | "while"
            | "loop"
            | "return"
            | "break"
            | "continue"
            | "let"
            | "in"
            | "fn"
            | "as"
            | "async"
            | "await"
            | "box"
            | "const"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "gen"
            | "impl"
            | "macro"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "yield"
            | "abstract"
            | "become"
            | "final"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
    )
}
