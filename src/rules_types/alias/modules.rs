//! Module-path-precise import evidence for type aliases.
//!
//! Names alone collide across modules (`testing::fixtures::Value` vs
//! `value::Value`), so same-package files count only when their imports
//! resolve to the alias's home module. `crate`/`super`/`self`/`$crate`
//! roots resolve against the importing file; a leading segment matching
//! the home crate segment re-roots own-crate self-imports; anything else
//! stays foreign. Direct root-child references (`crate::Name`) count
//! leniently, since same-crate roots commonly re-export; re-export chains
//! through other modules stay invisible and keep silent rather than risk
//! a false flag.

use crate::{
    lexer::word::word_match_at,
    rules_simple::manifest::package_dir_for,
    rules_tests::range::code_of,
    rules_types::alias::{head::split_top_level, scope::use_statements},
};

/// One imported binding: module path plus bound name (`None` for globs).
struct Binding {
    /// Absolute module path holding the binding.
    module: Vec<String>,
    /// Bound name, or `None` for glob imports.
    name: Option<String>,
}

/// Module path of `path`, if derivable.
///
/// Strips the package dir and the leaf: `src/a/b.rs` maps to `a::b`,
/// `src/a.rs` and `src/a/mod.rs` to `a`, roots (`lib.rs`, `main.rs`,
/// `mod.rs`) to the crate root. Non-`src` leaves gain their leaf as a
/// namespace (`tests/x.rs` is another crate's `x`), keeping test doubles
/// isolated from library items.
#[must_use]
pub fn module_of(path: &str) -> Vec<String> {
    let package = package_dir_for(path);
    let rest = path
        .strip_prefix(package)
        .unwrap_or(path)
        .trim_start_matches('/');
    let (leaf, rel) = rest.split_once('/').unwrap_or(("", rest));
    let mut segments: Vec<String> = Vec::new();
    for part in rel.split('/') {
        segments.push(part.to_owned());
    }
    let Some(file) = segments.pop() else {
        return Vec::new();
    };
    let stem = file.strip_suffix(".rs").unwrap_or(&file);
    if stem != "mod" && stem != "lib" && stem != "main" {
        segments.push(stem.to_owned());
    }
    if leaf != "src" && !leaf.is_empty() {
        segments.insert(0, leaf.to_owned());
    }
    segments
}

/// Bindings of `part` under `prefix`.
fn expand_part(prefix: &[String], part: &str, out: &mut Vec<Binding>) {
    let part = part.trim();
    if let Some(group) = part
        .strip_prefix('{')
        .and_then(|inner| inner.strip_suffix('}'))
    {
        if prefix.is_empty() {
            return;
        }
        for item in split_top_level(group) {
            expand_part(prefix, &item, out);
        }
        return;
    }
    let (head, tail) = match part.split_once("::") {
        Some((before, after)) => (before.trim(), Some(after.trim())),
        None => (part, None),
    };
    if head.is_empty() {
        return;
    }
    let Some(rest) = tail else {
        if prefix.is_empty() {
            return;
        }
        leaf_binding(prefix, head, out);
        return;
    };
    let mut prefix = prefix.to_vec();
    prefix.push(head.to_owned());
    expand_part(&prefix, rest, out);
}

/// Leaf binding for `head` under `prefix`.
fn leaf_binding(prefix: &[String], head: &str, out: &mut Vec<Binding>) {
    if head == "*" {
        out.push(Binding {
            module: prefix.to_vec(),
            name: None,
        });
        return;
    }
    let tokens: Vec<&str> = head.split_whitespace().collect();
    let (path, bound) = match tokens.as_slice() {
        [single] => (*single, None),
        [path, keyword, bound] if *keyword == "as" => (*path, Some(*bound)),
        _ => return,
    };
    if path == "self" {
        let mut module = prefix.to_vec();
        let Some(last) = module.pop() else {
            return;
        };
        out.push(Binding {
            module,
            name: Some(bound.unwrap_or(&last).to_owned()),
        });
        return;
    }
    out.push(Binding {
        module: prefix.to_vec(),
        name: Some(bound.unwrap_or(path).to_owned()),
    });
}

/// Apply one path segment; `first` marks the leading segment.
///
/// Returns false when unresolvable: `super` past the root, or a foreign
/// leading segment.
fn step_segment(module: &mut Vec<String>, part: &str, first: bool, seg: &str) -> bool {
    match part {
        "crate" | "$crate" if first => {
            module.clear();
            true
        }
        "super" => {
            if module.is_empty() {
                return false;
            }
            module.truncate(module.len().saturating_sub(1));
            true
        }
        "self" => true,
        _ if first && part == seg && !seg.is_empty() => {
            module.clear();
            true
        }
        _ if first => false,
        _ => {
            module.push(part.to_owned());
            true
        }
    }
}

/// Absolute module path of `prefix` from `file_module`, if resolvable.
///
/// `crate`/`$crate` root at the crate; `super` climbs (past the root
/// stays unresolved); `self` stays; a leading segment matching `seg`
/// re-roots an own-crate self-import; anything else is foreign.
fn resolve_prefix(prefix: &[String], file_module: &[String], seg: &str) -> Option<Vec<String>> {
    let mut module = file_module.to_vec();
    let mut first = true;
    for part in prefix {
        if !step_segment(&mut module, part, first, seg) {
            return None;
        }
        first = false;
    }
    Some(module)
}

/// Path text of the `use` statement `stmt`, if any.
///
/// Finds the `use` keyword with word boundaries (visibility qualifiers
/// included) and cuts the trailing `;`, so same-line trailing code never
/// leaks into path parsing.
fn use_path_text(stmt: &str) -> Option<&str> {
    let mut i = 0;
    while i < stmt.len() {
        if word_match_at(stmt, "use", i) {
            let rest = stmt.get(i.saturating_add(3)..).unwrap_or("").trim();
            return rest.split(';').next().map(str::trim);
        }
        i = i.saturating_add(1);
    }
    None
}

/// Bindings imported by `lines`.
fn statement_bindings(lines: &[String]) -> Vec<Binding> {
    let mut out = Vec::new();
    for stmt in use_statements(lines) {
        let Some(text) = use_path_text(&stmt) else {
            continue;
        };
        expand_part(&[], text, &mut out);
    }
    out
}

/// Roots bound from `std`, `core`, or `alloc` imports in `lines`.
///
/// Only direct children count (`use std::fmt;` binds `fmt`, renames
/// included); deeper leaves (`use std::fmt::Debug;`) bind their own name
/// instead. Roots shadowed by a local `mod` declaration stay out, since
/// qualified mentions there may resolve locally rather than externally.
#[must_use]
pub fn std_imported_roots(lines: &[String]) -> Vec<String> {
    let mut roots = Vec::new();
    for binding in statement_bindings(lines) {
        let Some(name) = binding.name else {
            continue;
        };
        if binding.module.len() != 1
            || !binding
                .module
                .first()
                .is_some_and(|head| matches!(head.as_str(), "std" | "core" | "alloc"))
        {
            continue;
        }
        if roots.iter().any(|known| known == &name) || declares_module(lines, &name) {
            continue;
        }
        roots.push(name);
    }
    roots
}

/// Byte index after the `mod` keyword at `i`, skipping whitespace.
fn after_mod_keyword(code: &str, i: usize) -> usize {
    let mut j = i.saturating_add(3);
    while code.as_bytes().get(j).is_some_and(u8::is_ascii_whitespace) {
        j = j.saturating_add(1);
    }
    j
}

/// True if `mod name` opens at `i` with identifier boundaries.
fn is_mod_decl_at(code: &str, i: usize, name: &str) -> bool {
    word_match_at(code, "mod", i) && word_match_at(code, name, after_mod_keyword(code, i))
}

/// True if `code` declares `mod name` with identifier boundaries.
fn line_declares_module(code: &str, name: &str) -> bool {
    let mut i = 0;
    while i < code.len() {
        if is_mod_decl_at(code, i, name) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True if `lines` declare `mod name` with identifier boundaries.
fn declares_module(lines: &[String], name: &str) -> bool {
    lines
        .iter()
        .any(|line| line_declares_module(&code_of(line), name))
}

/// True if `lines` import `name` from `home_module`.
///
/// Bindings resolve per statement against `file_module`; a matching
/// module with the bound name (or a glob) evidences. Direct root-child
/// references (`crate::Name`) count leniently, since same-crate roots
/// commonly re-export; `seg` re-roots own-crate self-imports.
#[must_use]
pub fn imports_module(
    lines: &[String],
    file_module: &[String],
    home_module: &[String],
    name: &str,
    seg: &str,
) -> bool {
    statement_bindings(lines).iter().any(|binding| {
        let Some(module) = resolve_prefix(&binding.module, file_module, seg) else {
            return false;
        };
        match &binding.name {
            Some(bound) if bound.as_str() == name => {
                module == *home_module || (module.is_empty() && binding.module.len() == 1)
            }
            None => module == *home_module,
            Some(_) => false,
        }
    })
}
