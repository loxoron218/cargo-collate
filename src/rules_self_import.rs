//! Self-import rule parent index.

pub mod expand;
pub mod resolve;
pub mod token;
pub mod traversal;

use crate::{
    gating::src_gated_stems,
    rules_self_import::{resolve::file_mod, traversal::SelfScan},
    scan::SourceFile,
};

/// Use imports resolving into the file own module.
#[must_use]
pub fn self_imports(files: &[SourceFile]) -> Vec<String> {
    let stems = src_gated_stems(files);
    let mut out = Vec::new();
    for file in files {
        let Some(cur) = file_mod(&file.path) else {
            continue;
        };
        let stem = file.path.rsplit('/').next().unwrap_or(&file.path);
        let stem = stem.strip_suffix(".rs").unwrap_or(stem);
        if stems.contains(stem) {
            continue;
        }
        out.extend(SelfScan::scan_file(&file.path, &file.lines, &cur));
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{
        gating::src_gated_stems,
        rules_self_import::{expand::expand, resolve::file_mod, self_imports},
        scan::support::test_file,
    };

    #[test]
    fn module_mapping() {
        assert_eq!(file_mod("src/foo.rs"), Some(vec!["foo".to_owned()]));
        assert_eq!(file_mod("src/foo/mod.rs"), Some(vec!["foo".to_owned()]));
        assert_eq!(file_mod("src/main.rs"), Some(Vec::new()));
        assert_eq!(file_mod("tests/a.rs"), None);
        assert_eq!(
            file_mod("collate/src/lexer.rs"),
            Some(vec!["lexer".to_owned()])
        );
    }

    #[test]
    fn braces_expand() {
        assert_eq!(expand("a::{b,c}").len(), 2);
        assert_eq!(expand("a::b"), vec!["a::b".to_owned()]);
    }

    #[test]
    fn own_item_use_flagged() {
        let files = vec![test_file(
            "src/foo.rs",
            &["use crate::foo::Bar;", "pub struct Bar;"],
        )];
        assert_eq!(self_imports(&files).len(), 1);
    }

    #[test]
    fn external_use_allowed() {
        let files = vec![test_file(
            "src/foo.rs",
            &["use crate::bar::Baz;", "pub struct Bar;"],
        )];
        assert_eq!(self_imports(&files).len(), 0);
    }

    #[test]
    fn tool_tree_stems_resolve() {
        let files = vec![test_file(
            "collate/src/foo.rs",
            &["use crate::foo::Bar;", "pub struct Bar;"],
        )];
        assert_eq!(self_imports(&files).len(), 1);
        let gated = vec![test_file(
            "collate/src/foo.rs",
            &["#[cfg(test)]", "mod foo;"],
        )];
        assert!(src_gated_stems(&gated).contains("foo"));
    }
}
