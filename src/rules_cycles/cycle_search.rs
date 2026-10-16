//! Cycle search and hit formatting for module dependencies.
//!
//! Prunes acyclic files from the import graph, extracts one deterministic
//! cycle per remaining participant, and formats one hit per file.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    rules_cycles::{cycle_graph::EdgeGraph, cycle_resolve::ModuleIndex},
    scan::SourceFile,
};

/// Files remaining after pruning acyclic nodes.
///
/// Nodes without incoming or outgoing edges inside the live set cannot sit on
/// a cycle, so repeated removal leaves exactly the cycle participants.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
///
/// # Returns
///
/// * `BTreeSet<String>` - Paths participating in a dependency cycle.
#[must_use]
pub fn cycle_members(adj: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let mut live: BTreeSet<String> = adj.keys().cloned().collect();
    loop {
        let incoming = incoming_counts(adj, &live);
        let removable = removable_nodes(adj, &live, &incoming);
        if removable.is_empty() {
            break;
        }
        for node in removable {
            let _: bool = live.remove(&node);
        }
    }
    live
}

/// Nodes inside the live set that cannot sit on a cycle.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
/// * `live` - Candidate paths still under consideration.
/// * `incoming` - Incoming edge counts per live node.
///
/// # Returns
///
/// * `Vec<String>` - Live nodes missing an incoming or outgoing edge.
#[must_use]
pub fn removable_nodes(
    adj: &BTreeMap<String, BTreeSet<String>>,
    live: &BTreeSet<String>,
    incoming: &BTreeMap<&str, usize>,
) -> Vec<String> {
    let mut out = Vec::new();
    for node in live {
        let friendless = incoming.get(node.as_str()).copied().unwrap_or(0) == 0;
        if outgoing_count(adj, live, node) == 0 || friendless {
            out.push(node.clone());
        }
    }
    out
}

/// Incoming edge counts per live node from live peers.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
/// * `live` - Candidate paths still under consideration.
///
/// # Returns
///
/// * `BTreeMap<&str, usize>` - Incoming counts keyed by node path.
#[must_use]
pub fn incoming_counts<'a>(
    adj: &BTreeMap<String, BTreeSet<String>>,
    live: &'a BTreeSet<String>,
) -> BTreeMap<&'a str, usize> {
    let mut incoming: BTreeMap<&str, usize> = BTreeMap::new();
    for node in live {
        incoming.extend([(node.as_str(), 0)]);
    }
    for node in live {
        let Some(peers) = adj.get(node) else {
            continue;
        };
        for peer in peers {
            count_peer(live, &mut incoming, peer);
        }
    }
    incoming
}

/// Count one incoming edge from a live peer, if live.
///
/// # Arguments
///
/// * `live` - Candidate paths still under consideration.
/// * `incoming` - Incoming edge counts per live node.
/// * `peer` - Edge target whose count grows.
///
/// # Returns
///
/// Nothing; `incoming` gains the peer edge when live.
fn count_peer(live: &BTreeSet<String>, incoming: &mut BTreeMap<&str, usize>, peer: &str) {
    if !live.contains(peer) {
        return;
    }
    if let Some(count) = incoming.get_mut(peer) {
        *count = count.saturating_add(1);
    }
}

/// Outgoing edge count of one node toward live peers.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
/// * `live` - Candidate paths still under consideration.
/// * `node` - Source path whose edges count.
///
/// # Returns
///
/// * `usize` - Live outgoing edge count.
#[must_use]
pub fn outgoing_count(
    adj: &BTreeMap<String, BTreeSet<String>>,
    live: &BTreeSet<String>,
    node: &str,
) -> usize {
    adj.get(node).map_or(0, |peers| {
        peers.iter().filter(|peer| live.contains(*peer)).count()
    })
}

/// One dependency cycle starting and ending at `start`.
///
/// Depth-first search inside the live set finds a path back to `start`;
/// neighbors stay sorted so the result is deterministic.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
/// * `live` - Paths participating in a dependency cycle.
/// * `start` - Cycle participant the path starts from.
///
/// # Returns
///
/// * `Vec<String>` - Cycle paths from `start` back to itself, if any.
#[must_use]
pub fn find_cycle(
    adj: &BTreeMap<String, BTreeSet<String>>,
    live: &BTreeSet<String>,
    start: &str,
) -> Vec<String> {
    let mut path = vec![start.to_owned()];
    let mut at: BTreeMap<String, usize> = BTreeMap::new();
    at.extend([(start.to_owned(), 0)]);
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<(String, Vec<String>, usize)> =
        vec![(start.to_owned(), live_neighbors(adj, live, start), 0)];
    while let Some((node, next)) = stack_next(&mut stack) {
        let Some(next) = next else {
            let _: bool = done.insert(node.clone());
            drop(stack.pop());
            drop(path.pop());
            let _: Option<usize> = at.remove(&node);
            continue;
        };
        if next == start && !path.is_empty() {
            path.push(start.to_owned());
            return path;
        }
        if at.contains_key(&next) || done.contains(&next) {
            continue;
        }
        at.extend([(next.clone(), path.len())]);
        path.push(next.clone());
        stack.push((next.clone(), live_neighbors(adj, live, &next), 0));
    }
    Vec::new()
}

/// Next search frame with its following neighbor, if the stack stays live.
///
/// Advances the top frame past the yielded neighbor without popping it.
///
/// # Arguments
///
/// * `stack` - Search frames with neighbor lists and resume indices.
///
/// # Returns
///
/// * `Option<(String, Option<String>)>` - Frame node with its next neighbor.
fn stack_next(stack: &mut [(String, Vec<String>, usize)]) -> Option<(String, Option<String>)> {
    let top = stack.last_mut()?;
    let next = top.1.get(top.2).cloned();
    if next.is_some() {
        top.2 = top.2.saturating_add(1);
    }
    Some((top.0.clone(), next))
}

/// Sorted live neighbors of one node.
///
/// # Arguments
///
/// * `adj` - File adjacency per source path.
/// * `live` - Paths participating in a dependency cycle.
/// * `node` - Source path whose neighbors resolve.
///
/// # Returns
///
/// * `Vec<String>` - Neighboring paths inside the live set.
pub fn live_neighbors(
    adj: &BTreeMap<String, BTreeSet<String>>,
    live: &BTreeSet<String>,
    node: &str,
) -> Vec<String> {
    adj.get(node).map_or_else(Vec::new, |peers| {
        peers
            .iter()
            .filter(|peer| live.contains(*peer))
            .cloned()
            .collect()
    })
}

/// Display name of one dotted module path.
///
/// # Arguments
///
/// * `dot` - Dotted module path, empty for the crate root.
///
/// # Returns
///
/// * `String` - `crate`-qualified display name.
#[must_use]
pub fn display_of(dot: &str) -> String {
    if dot.is_empty() {
        "crate".to_owned()
    } else {
        format!("crate::{dot}")
    }
}

/// One hit per cycle participant with its cycle path.
///
/// # Arguments
///
/// * `files` - Scanned source files with original and blanked lines.
/// * `index` - Module lookup with dotted names per file.
/// * `graph` - Adjacency with first statement lines per edge.
/// * `live` - Paths participating in a dependency cycle.
///
/// # Returns
///
/// * `Vec<String>` - Hit lines, sorted and deduplicated.
#[must_use]
pub fn format_hits(
    files: &[SourceFile],
    index: &ModuleIndex,
    graph: &EdgeGraph,
    live: &BTreeSet<String>,
) -> Vec<String> {
    let mut by_path: BTreeMap<&str, &SourceFile> = BTreeMap::new();
    for file in files {
        if !by_path.contains_key(file.path.as_str()) {
            by_path.extend([(file.path.as_str(), file)]);
        }
    }
    let mut hits = Vec::new();
    for node in live {
        let cycle = find_cycle(&graph.adj, live, node);
        if cycle.len() < 2 {
            continue;
        }
        let Some(file) = by_path.get(node.as_str()) else {
            continue;
        };
        let Some(second) = cycle.get(1) else {
            continue;
        };
        let line_no = graph
            .edge_lines
            .get(&(node.clone(), second.clone()))
            .copied()
            .unwrap_or(1);
        let path = file.path.as_str();
        let content = file
            .lines
            .get(line_no.saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        let mut shown = Vec::new();
        for step in &cycle {
            let dot = index.dot_of.get(step).cloned().unwrap_or_default();
            shown.push(display_of(&dot));
        }
        let trail = shown.join(" -> ");
        hits.push(format!(
            "{path}:{line_no}:{content} (circular dependency: {trail})"
        ));
    }
    hits.sort();
    hits.dedup();
    hits
}
