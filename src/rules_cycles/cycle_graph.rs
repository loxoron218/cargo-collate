//! Module index and import edges for cycle detection.
//!
//! Maps scanned files onto package-scoped modules and collects file-level
//! import edges from `crate`-, `self`-, `super`-, own-package-rooted, and
//! bare local `use` statements.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    gating::{is_stem_gated, src_gated_stems},
    rules_alias::{
        collect::{collect_use_stmts, use_body},
        import_alias::{split_alias, strip_raw},
    },
    rules_cycles::cycle_resolve::{
        ModuleIndex, is_prefix_pair, longest_module_target, stmt_test_gated,
    },
    rules_harness::range::{in_ranges, stray_gap, test_ranges},
    rules_imports::{
        import_attr::attached_start,
        origin::{locals_for, package_names, top_modules_by_package},
    },
    rules_self_import::{
        expand::expand,
        resolve::{file_mod, resolve_full},
    },
    rules_simple::manifest::package_dir_for,
    scan::SourceFile,
};

/// Import edges backing cycle search.
#[derive(Debug)]
pub struct EdgeGraph {
    /// Outgoing file edges per source path.
    pub adj: BTreeMap<String, BTreeSet<String>>,
    /// First statement lines per file edge.
    pub edge_lines: BTreeMap<(String, String), usize>,
}

/// Per-file edge collector holding resolution context.
struct EdgeScan<'a> {
    /// Module lookup with dotted names per file.
    index: &'a ModuleIndex,
    /// Scanned source file gaining edges.
    file: &'a SourceFile,
    /// Module path of the scanned file.
    cur: Vec<String>,
    /// Package dir of the scanned file.
    pkg: String,
    /// Own crate name of the package, if known.
    pkg_name: Option<&'a str>,
    /// Local top-level modules of the file source root.
    locals: &'a BTreeSet<String>,
    /// Test-module ranges as zero-based spans.
    ranges: Vec<(usize, usize)>,
    /// Graph receiving adjacency and edge lines.
    graph: &'a mut EdgeGraph,
}

impl EdgeScan<'_> {
    /// Collect outgoing edges of the scanned file into the graph.
    ///
    /// # Returns
    ///
    /// Nothing; the graph gains the file edges.
    fn collect(&mut self) {
        for (start, _, buf) in collect_use_stmts(&self.file.lines) {
            self.stmt(start, &buf);
        }
    }

    /// Collect edges of one `use` statement unless test-gated.
    ///
    /// # Arguments
    ///
    /// * `start` - One-based line where the statement starts.
    /// * `buf` - Joined statement code.
    ///
    /// # Returns
    ///
    /// Nothing; the graph gains the statement edges.
    fn stmt(&mut self, start: usize, buf: &str) {
        let use_idx = start.saturating_sub(1);
        if in_ranges(&self.ranges, use_idx) {
            return;
        }
        if stmt_test_gated(self.file, attached_start(self.file, use_idx), use_idx) {
            return;
        }
        let Some(body) = use_body(buf) else {
            return;
        };
        for expanded in expand(&body) {
            self.edge(&expanded, start);
        }
    }

    /// Collect the edge of one expanded import path, if it hits a module.
    ///
    /// Direct parent-child pairs stay silent; aggregation and back-references
    /// across one nesting level never flag.
    ///
    /// # Arguments
    ///
    /// * `expanded` - One brace-expanded import path.
    /// * `start` - One-based line where the statement starts.
    ///
    /// # Returns
    ///
    /// Nothing; the graph gains the edge with its line.
    fn edge(&mut self, expanded: &str, start: usize) {
        let source = self.file.path.clone();
        let Some(target) = self.resolve(expanded, &source) else {
            return;
        };
        if is_prefix_pair(self.index, &source, &target) {
            return;
        }
        if !self.graph.adj.contains_key(source.as_str()) {
            self.graph.adj.extend([(source.clone(), BTreeSet::new())]);
        }
        if let Some(peers) = self.graph.adj.get_mut(source.as_str()) {
            peers.extend([target.clone()]);
        }
        if !self
            .graph
            .edge_lines
            .contains_key(&(source.clone(), target.clone()))
        {
            self.graph.edge_lines.extend([((source, target), start)]);
        }
    }

    /// Target file path of one expanded import, if it names a known module.
    ///
    /// External roots, globs, and self-edges stay silent. Bare first segments
    /// matching local top-level modules resolve as crate-relative.
    ///
    /// # Arguments
    ///
    /// * `expanded` - One brace-expanded import path.
    /// * `source` - Importing file path for self-edge checks.
    ///
    /// # Returns
    ///
    /// * `Option<String>` - Target file path when the import hits a module.
    fn resolve(&self, expanded: &str, source: &str) -> Option<String> {
        let (path_part, _) = split_alias(expanded);
        if path_part.contains('*') {
            return None;
        }
        let segs: Vec<String> = path_part
            .split("::")
            .map(str::trim)
            .filter(|seg| !seg.is_empty())
            .map(|seg| strip_raw(seg).to_owned())
            .collect();
        let first = segs.first()?;
        if first.as_str() == "crate" {
            let tail: Vec<String> = segs.iter().skip(1).cloned().collect();
            return longest_module_target(&tail, &self.pkg, self.index, source);
        }
        if first.as_str() == "self" || first.as_str() == "super" {
            let full = resolve_full(&segs, &self.cur)?;
            return longest_module_target(&full, &self.pkg, self.index, source);
        }
        if self.pkg_name.is_some_and(|name| name == first.as_str()) {
            let tail: Vec<String> = segs.iter().skip(1).cloned().collect();
            return longest_module_target(&tail, &self.pkg, self.index, source);
        }
        if self.locals.contains(first.as_str()) {
            return longest_module_target(&segs, &self.pkg, self.index, source);
        }
        None
    }
}

/// Scanned files keyed by path in deterministic order.
///
/// Later duplicates of one path stay silent so repeated listings never fork
/// the graph.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `BTreeMap<&str, &SourceFile>` - Files keyed by path.
fn ordered_files(files: &[SourceFile]) -> BTreeMap<&str, &SourceFile> {
    let mut ordered: BTreeMap<&str, &SourceFile> = BTreeMap::new();
    for file in files {
        if !ordered.contains_key(file.path.as_str()) {
            ordered.extend([(file.path.as_str(), file)]);
        }
    }
    ordered
}

/// File paths keyed by package and dotted module.
///
/// Files outside source trees and test-gated file modules stay out so only
/// real library modules become graph nodes.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
///
/// # Returns
///
/// * `ModuleIndex` - Module lookup with dotted names per file.
#[must_use]
pub fn index_modules(files: &[SourceFile]) -> ModuleIndex {
    let gated = src_gated_stems(files);
    let ordered = ordered_files(files);
    let mut index = ModuleIndex {
        by_module: BTreeMap::new(),
        dot_of: BTreeMap::new(),
    };
    for file in ordered.values() {
        let Some(cur) = file_mod(&file.path) else {
            continue;
        };
        if is_stem_gated(file, &gated) {
            continue;
        }
        let pkg = package_dir_for(&file.path).to_owned();
        let dot = cur.join("::");
        if !index.dot_of.contains_key(file.path.as_str()) {
            index.dot_of.extend([(file.path.clone(), dot.clone())]);
        }
        if !index.by_module.contains_key(&(pkg.clone(), dot.clone())) {
            index.by_module.extend([((pkg, dot), file.path.clone())]);
        }
    }
    index
}

/// Import graph edges across indexed files.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
/// * `index` - Module lookup with dotted names per file.
///
/// # Returns
///
/// * `EdgeGraph` - Adjacency with first statement lines per edge.
#[must_use]
pub fn build_edges(files: &[SourceFile], index: &ModuleIndex) -> EdgeGraph {
    let names = package_names(files);
    let modules = top_modules_by_package(files);
    let fallback = BTreeSet::new();
    let mut graph = EdgeGraph {
        adj: BTreeMap::new(),
        edge_lines: BTreeMap::new(),
    };
    for path in index.dot_of.keys() {
        graph.adj.extend([(path.clone(), BTreeSet::new())]);
    }
    let ordered = ordered_files(files);
    for file in ordered.values() {
        if !index.dot_of.contains_key(file.path.as_str()) {
            continue;
        }
        collect_file_edges(file, index, &names, &modules, &fallback, &mut graph);
    }
    graph
}

/// Outgoing edges of one file into the shared graph.
///
/// Test-module and cfg-test-gated statements stay silent.
///
/// # Arguments
///
/// * `file` - Scanned source file.
/// * `index` - Module lookup with dotted names per file.
/// * `names` - Own crate names per package dir.
/// * `modules` - Top-level modules per package dir and source root.
/// * `fallback` - Empty module set for files outside every source root.
/// * `graph` - Graph receiving adjacency and edge lines.
///
/// # Returns
///
/// Nothing; `graph` gains the file edges.
fn collect_file_edges(
    file: &SourceFile,
    index: &ModuleIndex,
    names: &BTreeMap<String, Option<String>>,
    modules: &BTreeMap<(String, String), BTreeSet<String>>,
    fallback: &BTreeSet<String>,
    graph: &mut EdgeGraph,
) {
    let Some(cur) = file_mod(&file.path) else {
        return;
    };
    let pkg = package_dir_for(&file.path).to_owned();
    let pkg_name = names.get(&pkg).cloned().flatten();
    let ranges = test_ranges(&file.lines, stray_gap);
    let mut scan = EdgeScan {
        index,
        file,
        cur,
        pkg: pkg.clone(),
        pkg_name: pkg_name.as_deref(),
        locals: locals_for(modules, &pkg, &file.path, fallback),
        ranges,
        graph,
    };
    scan.collect();
}
