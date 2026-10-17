//! Argument-shape scan for two-argument calls.
//!
//! Counts top-level commas with nesting depths for `()`, `[]`, and `{}` plus
//! turbofish `<>` pairs, so `Box::new` calls split into their gtk and smart
//! pointer shapes on one spelling of argument boundaries.

/// Nesting depths for the argument-shape scan.
struct ArgDepths {
    /// Open `(` levels including the call opener.
    paren: i32,
    /// Open `[` levels.
    square: i32,
    /// Open `{` levels.
    brace: i32,
    /// Open turbofish `<` levels.
    angle: i32,
}

/// True if `Box::new(` at `open` takes two arguments.
///
/// Gtk boxes take `(orientation, spacing)` while `std::boxed::Box` takes a
/// single value, so calls with exactly one top-level comma count. Nesting
/// levels for `()`, `[]`, and `{}` plus turbofish `<>` pairs hide their
/// commas; a comma directly before the matching closer counts as trailing
/// rather than separating. Any pipe before the first comma disqualifies the
/// shape since orientations never contain closures.
///
/// # Arguments
///
/// * `body` - Joined code text with strings blanked and tails cut.
/// * `open` - Byte index of the call opener.
///
/// # Returns
///
/// * `bool` - True for two-argument calls.
#[must_use]
pub fn is_gtk_box_new(body: &str, open: usize) -> bool {
    let bytes = body.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return false;
    }
    let mut depths = default_depths();
    let mut commas: i32 = 0;
    let mut last_comma = 0;
    let mut saw_pipe = false;
    let mut idx = open.saturating_add(1);
    while idx < bytes.len() && depths.paren > 0 {
        step_arg_byte(
            bytes,
            idx,
            &mut depths,
            &mut commas,
            &mut last_comma,
            &mut saw_pipe,
        );
        idx = idx.saturating_add(1);
    }
    if depths.paren != 0 || saw_pipe {
        return false;
    }
    let close = idx.saturating_sub(1);
    let mut effective = commas;
    if commas > 0 && gaps_only(bytes, last_comma.saturating_add(1), close) {
        effective = effective.saturating_sub(1);
    }
    effective == 1
}

/// Zero depths for a fresh argument scan.
///
/// # Returns
///
/// * `ArgDepths` - Depths with the call opener counted.
const fn default_depths() -> ArgDepths {
    ArgDepths {
        paren: 1,
        square: 0,
        brace: 0,
        angle: 0,
    }
}

/// True if `depths` sits at the call argument level.
///
/// # Arguments
///
/// * `depths` - Current nesting depths.
///
/// # Returns
///
/// * `bool` - True with no open nested delimiters.
const fn is_top_arg(depths: &ArgDepths) -> bool {
    depths.paren == 1 && depths.square == 0 && depths.brace == 0 && depths.angle == 0
}

/// Advance the argument-shape scan past the byte at `idx`.
///
/// Counts top-level commas and tracks nesting so nested delimiters hide
/// theirs. Angle depth covers turbofish pairs only; other `<` and `>`
/// bytes stay inert so comparisons never unbalance the scan.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `idx` - Current index.
/// * `depths` - Mutable nesting depths.
/// * `commas` - Mutable top-level comma count.
/// * `last_comma` - Mutable index of the latest top-level comma.
/// * `saw_pipe` - Mutable pipe flag before the first comma.
fn step_arg_byte(
    bytes: &[u8],
    idx: usize,
    depths: &mut ArgDepths,
    commas: &mut i32,
    last_comma: &mut usize,
    saw_pipe: &mut bool,
) {
    match bytes.get(idx).copied() {
        Some(b',') if is_top_arg(depths) => {
            *commas = commas.saturating_add(1);
            *last_comma = idx;
        }
        Some(b')') if is_top_arg(depths) => {
            depths.paren = depths.paren.saturating_sub(1);
        }
        Some(b'|') if *commas == 0 => {
            *saw_pipe = true;
        }
        Some(b'(') => depths.paren = depths.paren.saturating_add(1),
        Some(b')') => depths.paren = depths.paren.saturating_sub(1),
        Some(b'[') => depths.square = depths.square.saturating_add(1),
        Some(b']') if depths.square > 0 => {
            depths.square = depths.square.saturating_sub(1);
        }
        Some(b'{') => depths.brace = depths.brace.saturating_add(1),
        Some(b'}') if depths.brace > 0 => {
            depths.brace = depths.brace.saturating_sub(1);
        }
        Some(b'<') if bytes.get(idx.saturating_sub(1)) == Some(&b':') => {
            depths.angle = depths.angle.saturating_add(1);
        }
        Some(b'>') if depths.angle > 0 => {
            depths.angle = depths.angle.saturating_sub(1);
        }
        Some(_) | None => {}
    }
}

/// True if `bytes` holds only gaps from `from` to `to`.
///
/// # Arguments
///
/// * `bytes` - Body bytes under scan.
/// * `from` - Inclusive start index.
/// * `to` - Exclusive end index.
///
/// # Returns
///
/// * `bool` - True for gap-only spans.
fn gaps_only(bytes: &[u8], from: usize, to: usize) -> bool {
    let mut idx = from;
    while idx < to && idx < bytes.len() {
        if bytes
            .get(idx)
            .is_none_or(|next| *next != b' ' && *next != b'\t' && *next != b'\n' && *next != b'\r')
        {
            return false;
        }
        idx = idx.saturating_add(1);
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::{
        rules_ui::builder_pattern::{box_shape::is_gtk_box_new, direct::direct_hits},
        scan::builders::test_file,
    };

    #[test]
    fn boxed_shapes_split() {
        for body in [
            "Box::new(Vertical, 6)",
            "Box::new(gtk::Orientation::Horizontal, 12)",
            "Box::new(orient, spacing)",
            "Box::new(Vertical, 6,)",
            "Box::new(orientation_of(mode), 6)",
        ] {
            assert!(is_gtk_box_new(body, 8), "{body} flags");
        }
        for body in [
            "Box::new(value)",
            "Box::pin(work())",
            "Box::new(move |_| handle())",
            "Box::new(move |_, decoded| f(&tx, decoded))",
            "Box::default()",
            "Box::new()",
            "Box::new((a, b))",
            "Box::new([a, b])",
            "Box::new(Vertical,)",
            "Box::new(maps::<String, i32>::of())",
        ] {
            assert!(!is_gtk_box_new(body, 8), "{body} stays silent");
        }
    }

    #[test]
    fn boxed_direct_hits_split() {
        let gtk_box = vec![test_file(
            "src/a.rs",
            &[
                "use gtk::{Box, Orientation::Vertical};",
                "fn f() {",
                "    let b = Box::new(Vertical, 6);",
                "    let c = Box::new(gtk::Orientation::Horizontal, 12);",
                "}",
            ],
        )];
        assert_eq!(direct_hits(&gtk_box).len(), 2, "orientation shapes flag");
        let std_box = vec![
            test_file(
                "src/b.rs",
                &[
                    "use gtk::Box;",
                    "fn f() {",
                    "    let b = Box::new(value);",
                    "    let p = Box::pin(work());",
                    "}",
                ],
            ),
            test_file(
                "src/c.rs",
                &[
                    "use gtk::Box;",
                    "fn f() {",
                    "    let b = Box::new(move |_| handle());",
                    "    let d: Box<dyn Ink> = Box::default();",
                    "}",
                ],
            ),
        ];
        assert!(
            direct_hits(&std_box).is_empty(),
            "single-value shapes stay silent"
        );
    }
}
