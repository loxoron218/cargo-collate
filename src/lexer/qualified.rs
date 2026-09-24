//! Qualified-path and enum-variant detection.

use crate::lexer::word::{ident_end_before, ident_start_at};

/// Primitive type roots exempt from path rules.
///
/// Complete primitive list from `doc.rust-lang.org/nightly/std/all.html`
/// `#primitives` (nightly 1.100.0, 2026-09-20): `array`, `bool`, `char`,
/// `f16`, `f32`, `f64`, `f128`, `fn`, `i8`–`i128`, `isize`, `u8`–`u128`,
/// `usize`, `never` (`!`), `pointer` (`*const`/`*mut`), `reference` (`&`),
/// `slice` (`[T]`), `str`, `tuple` (`(...)`), `unit` (`()`).
const PRIMITIVES: &[&str] = &[
    "array",
    "bool",
    "char",
    "f16",
    "f32",
    "f64",
    "f128",
    "fn",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "isize",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "usize",
    "never",
    "pointer",
    "reference",
    "slice",
    "str",
    "tuple",
    "unit",
];

/// Constructor heads exempt from path rules.
///
/// Common associated constructors/factories (never enum variants): `new`,
/// `default`, `from` family (`from`, `try_from`, `from_str`, `from_iter`),
/// capacity/empty factories (`with_capacity`, `empty`), and builder-pattern
/// heads (`builder`, `build`). Conservative: only names that are unambiguous
/// value constructors across `std/all.html`, so enum variants stay flagged.
const CTORS: &[&str] = &[
    "new",
    "builder",
    "build",
    "default",
    "from",
    "try_from",
    "from_str",
    "from_iter",
    "with_capacity",
    "empty",
];

/// True if `c` can continue an ASCII identifier.
const fn is_ident_char_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Collect the identifier chain after the marker at `pos`.
fn path_tail(chars: &[char], pos: usize) -> Option<Vec<String>> {
    let mut segs = Vec::new();
    let mut j = pos.saturating_add(2);
    loop {
        let Some(end) = ident_start_at(chars, j) else {
            return (!segs.is_empty()).then_some(segs);
        };
        let Some(segment) = chars.get(j..end) else {
            return (!segs.is_empty()).then_some(segs);
        };
        segs.push(segment.iter().collect());
        if chars.get(end) == Some(&':') && chars.get(end.saturating_add(1)) == Some(&':') {
            j = end.saturating_add(2);
        } else {
            return Some(segs);
        }
    }
}

/// True if `left` can root a qualified call path.
fn is_path_root(left: &str) -> bool {
    left == "crate"
        || left == "super"
        || left == "self"
        || left.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

/// True if a double-colon marker opens at `i`.
fn double_colon_at(chars: &[char], i: usize) -> bool {
    chars.get(i) == Some(&':') && chars.get(i.saturating_add(1)) == Some(&':')
}

/// True if `code` holds a fully qualified path at call sites.
///
/// Constructor heads and primitive roots stay exempt.
#[must_use]
pub fn has_qualified_path(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while chars.get(i).is_some() {
        if double_colon_at(&chars, i) && qualified_at(&chars, i) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// Evaluate one marker at `pos` for the qualified-path rule.
fn qualified_at(chars: &[char], pos: usize) -> bool {
    let Some(left_start) = ident_end_before(chars, pos) else {
        return false;
    };
    if left_start > 0
        && chars
            .get(left_start.saturating_sub(1))
            .is_some_and(|c| is_ident_char_local(*c))
    {
        return false;
    }
    let left: String = chars
        .get(left_start..pos)
        .map_or_else(String::new, |head| head.iter().collect());
    if !is_path_root(&left) || left == "Self" || PRIMITIVES.contains(&left.as_str()) {
        return false;
    }
    let Some(tail) = path_tail(chars, pos) else {
        return false;
    };
    tail.first()
        .is_some_and(|head| !CTORS.contains(&head.as_str()))
}

/// First enum-style path in `code`, returning its head type.
#[must_use]
pub fn enum_variant_hit(code: &str) -> Option<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while chars.get(i).is_some() {
        if double_colon_at(&chars, i)
            && let Some(left) = enum_variant_at(&chars, i)
        {
            return Some(left);
        }
        i = i.saturating_add(1);
    }
    None
}

/// Evaluate one marker at `pos` for the enum-variant rule.
fn enum_variant_at(chars: &[char], pos: usize) -> Option<String> {
    let left_start = ident_end_before(chars, pos)?;
    if left_start > 0
        && chars
            .get(left_start.saturating_sub(1))
            .is_some_and(|c| is_ident_char_local(*c))
    {
        return None;
    }
    let left: String = chars
        .get(left_start..pos)
        .map_or_else(String::new, |head| head.iter().collect());
    if !left.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if left == "Self" || (left.len() == 1 && left.chars().all(|c| c.is_ascii_uppercase())) {
        return None;
    }
    let right_end = ident_start_at(chars, pos.saturating_add(2))?;
    let segment = chars.get(pos.saturating_add(2)..right_end)?;
    let right: String = segment.iter().collect();
    if !right.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if CTORS.contains(&right.as_str()) {
        return None;
    }
    if right
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    if right_end < chars.len()
        && chars
            .get(right_end)
            .is_some_and(|c| is_ident_char_local(*c))
    {
        return None;
    }
    Some(left)
}

#[cfg(test)]
mod tests {
    use crate::lexer::qualified::{CTORS, PRIMITIVES, enum_variant_hit, has_qualified_path};

    #[test]
    fn primitives_cover_nightly() {
        assert!(
            PRIMITIVES.len() == 27,
            "PRIMITIVES drifted from the 27 nightly primitive types"
        );
        for name in [
            "f16",
            "f128",
            "array",
            "slice",
            "tuple",
            "unit",
            "never",
            "pointer",
            "reference",
            "fn",
        ] {
            assert!(
                PRIMITIVES.contains(&name),
                "PRIMITIVES missing nightly primitive `{name}`"
            );
        }
        assert!(
            !has_qualified_path("let x = str::from_utf8(b);"),
            "primitive roots stay exempt"
        );
        assert!(
            !has_qualified_path("let x = f16::from_f32(v);"),
            "new primitives stay exempt"
        );
    }

    #[test]
    fn ctors_cover_from_family() {
        for ctor in [
            "from",
            "try_from",
            "from_str",
            "from_iter",
            "with_capacity",
            "empty",
        ] {
            assert!(CTORS.contains(&ctor), "CTORS missing `{ctor}`");
        }
        assert!(
            enum_variant_hit("let x = String::from(y);").is_none(),
            "`from` stays a constructor, not a variant"
        );
    }
}
