//! Singular/plural stem folding for module names.
//!
//! Maps each file-name word to its stem so singulars and plurals count as the
//! same stem (`album` ≡ `albums`).

/// Normalize one file-name or directory word for singular/plural equivalence.
///
/// Lowercases, maps `ies` to `y`, strips `es` after sibilants (`s`, `x`, `z`,
/// `ch`, `sh`, including `sses`), then strips one trailing `s` unless the word
/// ends in `ss` or `us`. Short words stay untouched. `f` to `v` plurals
/// (`wolf` to `wolves`) stay unhandled; they are rare in code and naive
/// handling would mis-stem common words like `saves` or `moves`.
#[must_use]
pub fn normalize_stem(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    if lower.len() <= 2 {
        return lower;
    }
    if lower.len() > 4
        && lower.ends_with("ies")
        && let Some(base) = lower.strip_suffix("ies")
    {
        return format!("{base}y");
    }
    if lower.len() > 4
        && lower.ends_with("sses")
        && let Some(base) = lower.strip_suffix("es")
    {
        return base.to_owned();
    }
    if lower.len() > 4
        && ends_sibilant_es(&lower)
        && let Some(base) = lower.strip_suffix("es")
    {
        return strip_final_s(base);
    }
    if lower.len() > 3
        && lower.ends_with('s')
        && !lower.ends_with("ss")
        && !lower.ends_with("us")
        && let Some(base) = lower.strip_suffix('s')
    {
        return base.to_owned();
    }
    lower
}

/// True if `lower` ends in `es` after a sibilant needing `es` for plural.
///
/// Covers `ses`, `xes`, `zes`, `ches`, and `shes` so `boxes` maps to `box`
/// and `aliases` maps to `alias` before the final-`s` pass.
fn ends_sibilant_es(lower: &str) -> bool {
    lower.ends_with("ses")
        || lower.ends_with("xes")
        || lower.ends_with("zes")
        || lower.ends_with("ches")
        || lower.ends_with("shes")
}

/// Strip one trailing `s` from an already `es`-stripped base, if plural.
///
/// Lets `aliases` converge with `alias` (both map to `alia`) while `box`
/// from `boxes` stays put.
fn strip_final_s(base: &str) -> String {
    if base.len() > 3
        && base.ends_with('s')
        && !base.ends_with("ss")
        && !base.ends_with("us")
        && let Some(stripped) = base.strip_suffix('s')
    {
        return stripped.to_owned();
    }
    base.to_owned()
}

#[cfg(test)]
mod tests {
    use crate::rules_simple::naming::stem::normalize_stem;

    #[test]
    fn stems_fold_singular_plural() {
        assert_eq!(normalize_stem("album"), "album");
        assert_eq!(normalize_stem("albums"), "album");
        assert_eq!(normalize_stem("Album"), "album");
        assert_eq!(normalize_stem("track"), "track");
        assert_eq!(normalize_stem("tracks"), "track");
        assert_eq!(normalize_stem("type"), "type");
        assert_eq!(normalize_stem("types"), "type");
        assert_eq!(normalize_stem("box"), "box");
        assert_eq!(normalize_stem("boxes"), "box");
        assert_eq!(normalize_stem("story"), "story");
        assert_eq!(normalize_stem("stories"), "story");
        assert_eq!(normalize_stem("class"), "class");
        assert_eq!(normalize_stem("classes"), "class");
        assert_eq!(normalize_stem("status"), "status");
        assert_eq!(normalize_stem("statuses"), "status");
    }
}
