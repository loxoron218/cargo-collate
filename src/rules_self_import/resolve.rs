//! Module-path mapping and use-path resolution.

/// Map source paths to module paths; `None` outside the source tree.
pub fn file_mod(path: &str) -> Option<Vec<String>> {
    let stripped = path.strip_prefix("./").unwrap_or(path);
    let sub = stripped
        .strip_prefix("src/")
        .or_else(|| stripped.split_once("/src/").map(|(_, rest)| rest))?;
    let mut parts: Vec<String> = sub
        .strip_suffix(".rs")?
        .split('/')
        .map(str::to_owned)
        .collect();
    if parts == ["lib"] || parts == ["main"] {
        return Some(Vec::new());
    }
    if parts.first().is_some_and(|p| p == "bin") {
        let Some(rest) = parts.get(1..) else {
            return Some(Vec::new());
        };
        if rest.len() <= 1
            || rest.get(1..).is_some_and(|tail| tail == ["main"])
            || rest.get(1..).is_some_and(|tail| tail == ["mod"])
        {
            return Some(Vec::new());
        }
        return Some(rest.get(1..).map_or_else(Vec::new, <[String]>::to_vec));
    }
    if parts.last().is_some_and(|p| p == "mod") {
        let kept = parts.len().saturating_sub(1);
        parts.truncate(kept);
    }
    Some(parts)
}

/// Split a use-tree body on its first top-level alias.
#[must_use]
pub fn strip_alias(ep: &str) -> &str {
    let chars: Vec<char> = ep.chars().collect();
    (1..chars.len())
        .find(|&i| {
            chars
                .get(i.saturating_sub(1))
                .is_some_and(|prev| prev.is_whitespace())
                && chars
                    .get(i..)
                    .is_some_and(|rest| rest.starts_with(&['a', 's']))
                && (i.saturating_add(2) >= chars.len()
                    || chars
                        .get(i.saturating_add(2))
                        .is_some_and(|next| next.is_whitespace()))
        })
        .map_or_else(
            || ep.trim(),
            |i| {
                let byte = ep.char_indices().nth(i).map_or(ep.len(), |(b, _)| b);
                ep.get(..byte).map_or("", |prefix| prefix.trim())
            },
        )
}

/// Resolve a rooted import to an absolute module path.
///
/// Returns `None` for external roots handled by other rules.
pub fn resolve_full(segs: &[String], curpath: &[String]) -> Option<Vec<String>> {
    if segs.first().is_some_and(|root| root.as_str() == "crate") {
        return Some(segs.get(1..).map_or_else(Vec::new, <[String]>::to_vec));
    }
    if segs.first().is_some_and(|root| root.as_str() == "self") {
        let tail = segs.get(1..).map_or_else(Vec::new, <[String]>::to_vec);
        return Some(curpath.iter().chain(tail.iter()).cloned().collect());
    }
    if segs.first().is_some_and(|root| root.as_str() == "super") {
        let mut depth: usize = 0;
        while segs.get(depth).is_some_and(|s| *s == "super") {
            depth = depth.saturating_add(1);
        }
        let mut base = curpath.to_vec();
        let kept = base.len().saturating_sub(depth);
        base.truncate(kept);
        let tail = segs.iter().skip(depth).cloned().collect::<Vec<String>>();
        return Some(base.into_iter().chain(tail).collect());
    }
    None
}

/// True if an absolute path resolves into the current module.
#[must_use]
pub fn is_self_hit(full: &[String], curpath: &[String]) -> bool {
    !curpath.is_empty()
        && (full == curpath
            || (!full.is_empty()
                && full
                    .get(..full.len().saturating_sub(1))
                    .is_some_and(|head| head == curpath)))
}

/// Character-offset line starts for `chars`.
#[must_use]
pub fn char_line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            starts.push(i.saturating_add(1));
        }
    }
    starts
}
