//! Use-site verdicts for type aliases.
//!
//! Scores each mention's enclosing type with the alias substituted in, so
//! only aliases removable without tripping `type_complexity` stay flagged.
//! Neighboring lints (`ptr_arg`, `implicit_hasher`) stay ignored: they fix
//! without a new `type` alias, so they never keep an alias.

use crate::{
    lexer::word::{char_end, word_match_at},
    rules_simple::manifest::package_dir_for,
    rules_tests::range::code_of,
    rules_types::{
        alias::{
            discovery::TypeAlias,
            extract::{applied_args_at, enclosing_type, word_offsets},
            generics::{splice_applied, substitute},
            macros::{cover_hits, def_range_clean, macro_cover, macro_def_cover},
            modules::{imports_module, module_of, std_imported_roots},
            scope::{
                constructs_value, continues_path, imports_from, is_item_head, preceded_by_as,
                preceded_by_path, qualifier_matches, qualifier_root, use_statement_lines,
            },
        },
        score::type_score,
    },
    scan::SourceFile,
};

/// Per-alias verdict context shared across one file scan.
struct Ctx<'a> {
    /// Alias under verdict.
    alias: &'a TypeAlias,
    /// Active complexity limit.
    limit: u64,
    /// Per-line macro ranges of the scanned file.
    cover: &'a [Vec<(usize, usize)>],
    /// Per-line `macro_rules!` definition ranges of the scanned file.
    def_cover: &'a [Vec<(usize, usize)>],
    /// Defining file: macro mentions veto the verdict.
    ///
    /// Interpolation-free definition fragments still score as ordinary
    /// sites since their type text expands literally.
    veto: bool,
    /// Roots imported from `std`, `core`, or `alloc` in the scanned file.
    ///
    /// Qualified mentions under these roots (`fmt::Result`) can never
    /// resolve to the alias, even in the defining file where the segment
    /// gate stays open.
    std_roots: Vec<String>,
    /// Home package segment for foreign qualified mentions, if any.
    ///
    /// `None` keeps every qualified mention (defining file and package
    /// mates); `Some` counts only mentions rooted in the segment, so
    /// `fmt::Result` never resolves to another crate's alias.
    seg: Option<&'a str>,
}

/// `text` with every whole-word `name` replaced by `rhs`.
///
/// Raw substitution keeps the result identical to the true inlined type, so
/// scoring it matches what Clippy would see after removal.
#[must_use]
pub fn splice_alias(text: &str, name: &str, rhs: &str) -> String {
    if name.is_empty() {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if word_match_at(text, name, i) {
            out.push_str(rhs);
            i = i.saturating_add(name.len());
            continue;
        }
        let Some(next) = char_end(text, i) else {
            break;
        };
        out.push_str(text.get(i..next).unwrap_or(""));
        i = next;
    }
    out
}

/// True if inlining `name` as `rhs` keeps one use-site type warning-free.
///
/// Scores the substituted site against `type_complexity` only; neighboring
/// lints stay ignored since they fix without a new `type` alias.
fn site_keeps_alias(site: &str, name: &str, rhs: &str, limit: u64) -> bool {
    type_score(&splice_alias(site, name, rhs)) <= limit
}

/// True if inlining generic `alias` keeps one use-site type warning-free.
///
/// Substitutes the mention's applied arguments (or declared defaults for
/// bare mentions) into the right-hand side before splicing, so the score
/// matches the manual inline. Unparseable groups and unmapped parameters
/// keep the alias rather than scoring a half-substituted type.
fn generic_site_keeps_alias(
    lines: &[String],
    li: usize,
    pos: usize,
    site: &str,
    alias: &TypeAlias,
    limit: u64,
) -> bool {
    let Some((args, group)) = applied_args_at(lines, li, pos, alias.name.len()) else {
        return false;
    };
    let Some(concrete) = substitute(&alias.rhs, &alias.params, &args) else {
        return false;
    };
    type_score(&splice_applied(site, &alias.name, &group, &concrete)) <= limit
}

/// Definition ranges on line `li`, empty when absent.
fn def_ranges_at(def_cover: &[Vec<(usize, usize)>], li: usize) -> &[(usize, usize)] {
    def_cover.get(li).map_or(&[], Vec::as_slice)
}

/// True if a macro-covered mention may name another item, so it stays skipped.
///
/// Outside the defining file the mention may resolve through a re-export
/// or collision, so it never vetoes the verdict.
fn macro_should_skip(ctx: &Ctx<'_>, li: usize, pos: usize) -> bool {
    cover_hits(ctx.cover, li, pos) && !ctx.veto
}

/// True if a macro-covered mention in the defining file vetoes the verdict.
///
/// Interpolation-free definition fragments still score as ordinary sites
/// since their type text expands literally.
fn macro_vetoes_alias(ctx: &Ctx<'_>, li: usize, code: &str, pos: usize) -> bool {
    if !cover_hits(ctx.cover, li, pos) || !ctx.veto {
        return false;
    }
    !def_range_clean(code, def_ranges_at(ctx.def_cover, li), pos)
}

/// True if every use of `alias` on line `li` keeps warning-free inlined.
///
/// Macro interiors never resolve syntactically. In the defining file a
/// mention inside one vetoes the verdict, since it certainly resolves to
/// the alias — unless it sits in an interpolation-free definition
/// fragment, whose type text expands literally. In other files the mention
/// may name a different item through a re-export or collision, so it stays
/// skipped instead. Qualified mentions count only when rooted in the home
/// segment, while `std`, `core`, `alloc`, and their import-bound roots
/// never resolve to the alias.
fn line_keeps_alias(lines: &[String], li: usize, code: &str, ctx: &Ctx<'_>) -> bool {
    let alias = ctx.alias;
    for pos in word_offsets(code, &alias.name) {
        if is_item_head(code, pos) {
            continue;
        }
        let end = pos.saturating_add(alias.name.len());
        if continues_path(code, end) || constructs_value(code, end) {
            continue;
        }
        if preceded_by_as(code, pos) {
            continue;
        }
        if let Some(seg) = ctx.seg
            && preceded_by_path(code, pos)
            && !qualifier_matches(code, pos, seg)
        {
            continue;
        }
        if ctx.seg.is_none()
            && preceded_by_path(code, pos)
            && let Some(root) = qualifier_root(code, pos)
            && (root == "std"
                || root == "core"
                || root == "alloc"
                || ctx.std_roots.iter().any(|known| known == &root))
        {
            continue;
        }
        if macro_should_skip(ctx, li, pos) {
            continue;
        }
        if macro_vetoes_alias(ctx, li, code, pos) {
            return false;
        }
        let Some(site) = enclosing_type(lines, li, pos) else {
            return false;
        };
        let keeps = if alias.params.is_empty() {
            site_keeps_alias(&site, &alias.name, &alias.rhs, ctx.limit)
        } else {
            generic_site_keeps_alias(lines, li, pos, &site, alias, ctx.limit)
        };
        if !keeps {
            return false;
        }
    }
    true
}

/// True if every use of `name` in `lines` keeps warning-free inlined.
///
/// `own` holds the declaration span to skip when these lines declare it.
/// Import statements never sit in checked type positions, so their path
/// mentions stay out regardless of what they name — continuations of
/// multi-line `use` blocks included. `seg` gates foreign qualified
/// mentions (`None` keeps them all, as in the defining file and package
/// mates).
#[must_use]
pub fn file_keeps_alias(
    lines: &[String],
    own: Option<(usize, usize)>,
    alias: &TypeAlias,
    limit: u64,
    seg: Option<&str>,
) -> bool {
    let use_lines = use_statement_lines(lines);
    let cover = macro_cover(lines);
    let def_cover = macro_def_cover(lines);
    let std_roots = std_imported_roots(lines);
    let ctx = Ctx {
        alias,
        limit,
        cover: &cover,
        def_cover: &def_cover,
        veto: own.is_some(),
        std_roots,
        seg,
    };
    lines.iter().enumerate().all(|(li, line)| {
        if own.is_some_and(|(a, b)| (a..=b).contains(&li)) {
            return true;
        }
        if use_lines.get(li).copied().unwrap_or(false) {
            return true;
        }
        let code = code_of(line);
        line_keeps_alias(lines, li, &code, &ctx)
    })
}

/// Home package segment as a crate segment (dashes to underscores).
///
/// Qualified mentions rooting here (`member_a::Token`) may name the
/// alias; anything else (`fmt::`, `Self::`, `crate::`) cannot.
fn home_segment(home_path: &str) -> String {
    package_dir_for(home_path)
        .rsplit('/')
        .next()
        .unwrap_or("")
        .replace('-', "_")
}

/// True if `file` outside the alias package keeps the alias.
///
/// Same-repo members count with path-evidenced imports under the shared
/// git-root scope; other scopes stay out, since identical names there denote
/// different items.
fn foreign_keeps_alias(
    file: &SourceFile,
    home_ws: &str,
    seg: &str,
    alias: &TypeAlias,
    limit: u64,
) -> bool {
    if home_ws.is_empty() || file.ws.as_str() != home_ws {
        return true;
    }
    if !imports_from(&file.stripped, &alias.name, seg) {
        return true;
    }
    file_keeps_alias(&file.stripped, None, alias, limit, Some(seg))
}

/// True if same-package `file` keeps the alias.
///
/// The defining file and module mates count directly; other modules
/// count with imports resolving to the home module. Scope stays at file
/// granularity, so inline sibling modules within one file share the
/// verdict through lexical scope.
fn same_package_keeps(
    file: &SourceFile,
    fi: usize,
    home: usize,
    home_module: &[String],
    home_seg: &str,
    alias: &TypeAlias,
    limit: u64,
) -> bool {
    if fi == home {
        let own = Some((alias.start, alias.end));
        return file_keeps_alias(&file.stripped, own, alias, limit, None);
    }
    let file_module = module_of(&file.path);
    if file_module != home_module
        && !imports_module(
            &file.stripped,
            &file_module,
            home_module,
            &alias.name,
            home_seg,
        )
    {
        return true;
    }
    file_keeps_alias(&file.stripped, None, alias, limit, None)
}

/// True if `alias` in file `home` is removable without tripping
/// `type_complexity`.
///
/// The alias itself and every substituted use site within the same package
/// must score at or below the limit. Generic parameters substitute with
/// their applied arguments (or declared defaults) before scoring, so
/// applied uses read exactly like the manual inline. Macro interiors never
/// resolve syntactically: definitions and invocations stay out of
/// discovery, and a mention inside one vetoes the verdict in the defining
/// file unless it sits in an interpolation-free definition fragment.
/// Same-package files count when they share the alias's module or
/// import its module path, since identical names elsewhere denote
/// different items. Same-repo members resolve re-exports through
/// path-evidenced imports under the shared git-root scope;
/// other scopes stay unscanned. Genuine uses from crates outside the
/// scanned repo (and re-export chains) stay a documented blind spot.
/// Neighboring lints (`ptr_arg`, `implicit_hasher`) stay ignored since they
/// fix without a new `type` alias. This promises complexity-safety only;
/// other lints stay unforeseeable without type resolution.
#[must_use]
pub fn is_unnecessary(files: &[SourceFile], home: usize, alias: &TypeAlias, limit: u64) -> bool {
    if type_score(&alias.rhs) > limit {
        return false;
    }
    let home_pkg = files
        .get(home)
        .map_or("", |file| package_dir_for(&file.path));
    let home_ws: &str = files.get(home).map_or("", |file| file.ws.as_str());
    let home_seg = files
        .get(home)
        .map_or_default(|file| home_segment(&file.path));
    let home_module = files.get(home).map_or_default(|file| module_of(&file.path));
    files.iter().enumerate().all(|(fi, file)| {
        if package_dir_for(&file.path) != home_pkg {
            return foreign_keeps_alias(file, home_ws, &home_seg, alias, limit);
        }
        same_package_keeps(file, fi, home, &home_module, &home_seg, alias, limit)
    })
}
