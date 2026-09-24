//! Prelude-group checks for gtk-family imports.
//!
//! Flags `::prelude` imports from gtk-family crates so they route through
//! `libadwaita::prelude` instead. Split from the path driver to keep
//! single-file detectors under the project file-length limit.

use std::collections::BTreeSet;

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment, word::boundary_after},
    scan::SourceFile,
};

/// Widget libraries sharing one prelude group.
///
/// gtk-family crates whose `::prelude` imports should route through
/// `libadwaita::prelude` instead. `libadwaita`/`adw` stay out (allowed
/// alternative, see `gtk_prelude_flagged_libadwaita_allowed`). Every entry
/// below has a verified `::prelude` module in the gtk-rs-core/gtk4-rs docs.
/// Both gtk3 (`gtk`, `gdk`) and gtk4 (`gtk4`, `gdk4`) spellings stay listed
/// since gtk4 crates are commonly renamed on import. Dashes in package names
/// use underscores here (`gdk_pixbuf`, `gdk4_wayland`, `gdk4_x11`), matching
/// the segments that appear in `use` paths. `cairo` stays out (no prelude
/// module in its crate index) and `gobject` stays out (only `gobject-sys`
/// exists; its bindings live in `glib`).
const GTK_LIBS: &[&str] = &[
    "gtk",
    "gtk4",
    "gio",
    "glib",
    "gdk",
    "gdk4",
    "gdk4_wayland",
    "gdk4_x11",
    "gdk_pixbuf",
    "pango",
    "pangocairo",
    "graphene",
    "gsk4",
    "atk",
];

/// Line is exempt when commented out or inside a `$crate` macro arm.
fn prelude_commented(code_line: &str) -> bool {
    code_line.trim_start().starts_with("//") || code_line.contains("$crate::")
}

/// Byte-offset line starts for `text`.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i.saturating_add(1));
        }
    }
    starts
}

/// 1-based line number for byte `pos`.
fn lineno(starts: &[usize], pos: usize) -> usize {
    starts.partition_point(|&s| s <= pos)
}

/// Byte positions of `lib::prelude` hits with identifier boundaries.
fn lib_prelude_hits(text: &str, lib: &str) -> Vec<usize> {
    let needle = format!("{lib}::prelude");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(tail) = text.get(from..) {
        let Some(rel) = tail.find(&needle) else {
            break;
        };
        let pos = from.saturating_add(rel);
        if boundary_before(text, pos) && boundary_after(text, pos.saturating_add(needle.len())) {
            out.push(pos);
        }
        from = pos.saturating_add(1);
    }
    out
}

/// True if `text` has an identifier boundary right before byte `pos`.
fn boundary_before(text: &str, pos: usize) -> bool {
    text.get(..pos).is_some_and(|prefix| {
        prefix
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
    })
}

/// Byte positions of bare `prelude::` hits (not `::prelude::`).
fn bare_prelude_hits(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(tail) = text.get(from..) {
        let Some(rel) = tail.find("prelude::") else {
            break;
        };
        let pos = from.saturating_add(rel);
        let standalone: bool = boundary_before(text, pos)
            && text.get(..pos).is_some_and(|prefix| !prefix.ends_with(':'));
        if standalone {
            out.push(pos);
        }
        from = pos.saturating_add(1);
    }
    out
}

/// Byte index of the `{` enclosing `pos`, accounting for nesting.
fn find_opener(bytes: &[u8], pos: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut k = pos;
    while k > 0 {
        k = k.saturating_sub(1);
        match bytes.get(k).copied() {
            Some(b'}') => depth = depth.saturating_add(1),
            Some(b'{') if depth == 0 => return Some(k),
            Some(b'{') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// `use` group path segments right before the `{` at `opener`.
fn use_path_before(text: &str, opener: usize) -> Vec<String> {
    let before = text.get(..opener).map_or("", |prefix| prefix).trim_end();
    let width: usize = before
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b':')
        .count();
    before
        .get(before.len().saturating_sub(width)..)
        .map_or_else(Vec::new, |part| {
            part.split("::")
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
}

/// True if a bare `prelude::` at `pos` sits in a gtk-family `use` group.
fn is_gtk_grouped(text: &str, pos: usize) -> bool {
    let Some(opener) = find_opener(text.as_bytes(), pos) else {
        return false;
    };
    let segs = use_path_before(text, opener);
    segs.first().is_some_and(|s| GTK_LIBS.contains(&s.as_str()))
        && segs.last().is_some_and(|s| GTK_LIBS.contains(&s.as_str()))
        && !segs.iter().any(|s| s == "prelude")
}

/// `prelude` imports from gtk-family groups instead of `libadwaita`.
pub fn prelude_groups(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file_prelude).collect()
}

/// Prelude findings for one file.
fn check_file_prelude(file: &SourceFile) -> Vec<String> {
    let code_lines: Vec<String> = file
        .stripped
        .iter()
        .map(|l| cut_line_comment(&strip_strings(l)).to_owned())
        .collect();
    let text = code_lines.join("\n");
    let starts = line_starts(&text);
    let lib_lines = GTK_LIBS.iter().flat_map(|lib| {
        lib_prelude_hits(&text, lib)
            .into_iter()
            .filter_map(|pos| prelude_line(&code_lines, &starts, pos))
    });
    let bare_lines = bare_prelude_hits(&text).into_iter().filter_map(|pos| {
        let guarded: bool = code_lines
            .get(lineno(&starts, pos).saturating_sub(1))
            .is_some_and(|line| prelude_commented(line))
            || !is_gtk_grouped(&text, pos);
        (!guarded).then(|| lineno(&starts, pos))
    });
    let flagged = lib_lines.chain(bare_lines).collect::<BTreeSet<usize>>();
    flagged
        .into_iter()
        .map(|ln| {
            format!(
                "{}:{ln}:{}",
                file.path,
                file.lines
                    .get(ln.saturating_sub(1))
                    .map_or("", |line| line.as_str())
            )
        })
        .collect()
}

/// Line number for a `lib::prelude` hit unless its line is exempt.
fn prelude_line(code_lines: &[String], starts: &[usize], pos: usize) -> Option<usize> {
    let line_number = lineno(starts, pos);
    (!code_lines
        .get(line_number.saturating_sub(1))
        .is_some_and(|line| prelude_commented(line)))
    .then_some(line_number)
}

#[cfg(test)]
mod tests {
    use crate::{rules_paths::prelude::prelude_groups, scan::support::test_file};

    #[test]
    fn gtk_prelude_flagged_libadwaita_allowed() {
        for lib in [
            "gtk",
            "gtk4",
            "gio",
            "glib",
            "gdk",
            "gdk4",
            "gdk4_wayland",
            "gdk4_x11",
            "gdk_pixbuf",
            "pango",
            "pangocairo",
            "graphene",
            "gsk4",
            "atk",
        ] {
            let files = vec![test_file("src/a.rs", &[&format!("use {lib}::prelude::*;")])];
            assert_eq!(prelude_groups(&files).len(), 1, "{lib} prelude flags");
        }
        let good = vec![test_file("src/a.rs", &["use libadwaita::prelude::*;"])];
        assert!(prelude_groups(&good).is_empty(), "libadwaita stays allowed");
        let cairo = vec![test_file("src/a.rs", &["use cairo::prelude::*;"])];
        assert!(
            prelude_groups(&cairo).is_empty(),
            "cairo has no prelude module, stays out of scope"
        );
    }
}
