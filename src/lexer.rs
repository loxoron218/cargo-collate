//! String-literal and comment lexing shared by all rules.
//!
//! Parent index for the lexer capability. Submodules hold focused helpers
//! while this file re-exports the public surface used by rule modules.

pub mod literal;
pub mod marker;
pub mod qualified;
pub mod visibility;
pub mod word;

#[cfg(test)]
mod tests {
    use crate::lexer::{
        literal::strip_strings,
        marker::{has_plain_line_comment, is_doc_line},
        qualified::{enum_variant_hit, has_qualified_path},
        word::{declared_fn_name, has_underscore_ident, has_word},
    };

    #[test]
    fn strings_are_blanked() {
        assert_eq!(strip_strings("let s = \"a\";"), "let s = \"\";");
        assert_eq!(strip_strings("let x: &'a str;"), "let x: &'a str;");
        assert_eq!(strip_strings("ends with b"), "ends with b");
    }

    #[test]
    fn plain_markers_detected() {
        assert!(has_plain_line_comment("code // note"));
        assert!(!has_plain_line_comment("/// doc"));
        assert!(!has_plain_line_comment("//! module"));
        assert!(!has_plain_line_comment("//// rule"));
    }

    #[test]
    fn underscore_idents_detected() {
        assert!(has_underscore_ident("let _tmp = 1;"));
        assert!(!has_underscore_ident("let x = 1;"));
        assert!(!has_underscore_ident("let _ = 1;"));
    }

    #[test]
    fn qualified_paths_detected() {
        assert!(has_qualified_path("let x = foo::Bar;"));
        assert!(!has_qualified_path("let x = str::from_utf8(s);"));
        assert!(!has_qualified_path("let x = Foo::new();"));
    }

    #[test]
    fn enum_variants_detected() {
        assert_eq!(
            enum_variant_hit("let x = Error::NotFound;").as_deref(),
            Some("Error")
        );
        assert_eq!(enum_variant_hit("let x = Level::INFO;"), None);
        assert!(has_word("use gdk::Key;", "gdk"));
    }

    #[test]
    fn fn_keyword_boundaries() {
        assert!(
            declared_fn_name("fn f() {}").is_some(),
            "declarations stay detected"
        );
        assert!(
            declared_fn_name("let body = fn_body(lines, 0);").is_none(),
            "fn-prefixed calls stay silent"
        );
        assert!(
            declared_fn_name("pub unsafe fn f() {}").is_some(),
            "qualified declarations stay detected"
        );
    }

    #[test]
    fn doc_lines_detected() {
        assert!(is_doc_line("/// doc"));
        assert!(is_doc_line("    /// indented"));
        assert!(!is_doc_line("//// rule"));
        assert!(!is_doc_line("//! module"));
        assert!(!is_doc_line("let x = 1; /// trailing"));
        assert!(!is_doc_line("let x = 1;"));
    }
}
