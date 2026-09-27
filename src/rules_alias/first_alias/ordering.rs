//! First-occurrence alias tests.
//!
//! Regression coverage for scope approximation, gated imports, local
//! collisions, and the thiserror division of labor.

use crate::{rules_alias::first_alias::first_occurrence_alias, scan::support::test_file};

#[test]
fn correct_order_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["use crate::a::Error;", "use crate::b::Error as BError;"],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "plain first import stays silent"
    );
}

#[test]
fn reversed_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &["use crate::a::Error as AError;", "use crate::b::Error;"],
    )];
    let hits = first_occurrence_alias(&files);
    assert_eq!(hits.len(), 1);
    assert!(
        hits.first()
            .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
        "reversed alias points at first line"
    );
}

#[test]
fn both_aliased_flags_first() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use crate::a::Error as AError;",
            "use crate::b::Error as BError;",
        ],
    )];
    let hits = first_occurrence_alias(&files);
    assert_eq!(hits.len(), 1);
    assert!(
        hits.first()
            .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
        "double alias flags first line only"
    );
}

#[test]
fn singleton_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["use crate::a::Error as AError;", "fn f() {}"],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "singleton alias belongs to the unnecessary-alias lint"
    );
}

#[test]
fn grouped_and_pub_flagged() {
    let grouped = vec![test_file(
        "src/a.rs",
        &[
            "use {crate::a::Error as AError, crate::b::Error as BError};",
            "fn f() {}",
        ],
    )];
    assert_eq!(first_occurrence_alias(&grouped).len(), 1);
    let multiline = vec![test_file(
        "src/a.rs",
        &[
            "use crate::a::{",
            "    Error as AError,",
            "};",
            "use crate::b::Error;",
        ],
    )];
    let hits: Vec<String> = first_occurrence_alias(&multiline);
    assert!(
        hits.len() == 1
            && hits
                .first()
                .is_some_and(|hit| hit.starts_with("src/a.rs:2:")),
        "multiline must point at alias line, got {hits:?}"
    );
    let public = vec![test_file(
        "src/a.rs",
        &["pub use crate::a::Error as AError;", "use crate::b::Error;"],
    )];
    assert_eq!(first_occurrence_alias(&public).len(), 1);
}

#[test]
fn prelude_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["use std::boxed::Box as StdBox;", "use foo::Box;"],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "prelude origins stay exempt"
    );
}

#[test]
fn underscore_and_same_name_silent() {
    let underscore = vec![test_file(
        "src/a.rs",
        &["use crate::a::Bar as _;", "use crate::b::Bar;"],
    )];
    assert!(
        first_occurrence_alias(&underscore).is_empty(),
        "anonymous imports claim no slot"
    );
    let same_name = vec![test_file(
        "src/a.rs",
        &["use crate::a::Bar as Bar;", "use crate::b::Bar;"],
    )];
    assert!(
        first_occurrence_alias(&same_name).is_empty(),
        "same-name aliases belong to the unnecessary-alias lint"
    );
}

#[test]
fn nested_same_statement_flagged() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "mod tests {",
            "    use crate::playback::{",
            "        devices::OutputMode::{BitPerfect as ModeBitPerfect, Resampled},",
            "        signal_path::QualityVerdict::{BitPerfect, Limited},",
            "    };",
            "}",
        ],
    )];
    let hits = first_occurrence_alias(&files);
    assert_eq!(hits.len(), 1);
    assert!(
        hits.first()
            .is_some_and(|hit| hit.starts_with("src/a.rs:3:")),
        "nested same-statement alias points at alias line"
    );
}

#[test]
fn nested_cross_statement_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "fn first() {",
            "    use crate::sys::Item as Alias;",
            "}",
            "fn second() {",
            "    use crate::sys::Item as Alias;",
            "}",
        ],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "sibling scopes share no collision"
    );
}

#[test]
fn cross_depth_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use crate::a::Error as AError;",
            "fn f() {",
            "    use crate::b::Error;",
            "}",
        ],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "nested imports share no scope with top-level ones"
    );
}

#[test]
fn gated_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "#[cfg(feature = \"jack\")]",
            "pub use crate::host::jack::Host as JackHost;",
            "#[cfg(feature = \"pipewire\")]",
            "pub use crate::host::pipewire::Host as PipeWireHost;",
        ],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "gated imports live in different compilation worlds"
    );
}

#[test]
fn local_collision_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use std::{fs::read_to_string as fs_read_to_string, path::Path};",
            "fn read_to_string(path: &Path) -> bool {",
            "    true",
            "}",
        ],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "aliases colliding with locals stay required"
    );
}

#[test]
fn thiserror_collision_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["use std::io::Error as IoError;", "use thiserror::Error;"],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "thiserror collisions belong to the dedicated lint"
    );
}

#[test]
fn thiserror_ignored_among_others() {
    let files = vec![test_file(
        "src/a.rs",
        &[
            "use foo::Error as FooError;",
            "use bar::Error as BarError;",
            "use thiserror::Error;",
        ],
    )];
    let hits = first_occurrence_alias(&files);
    assert_eq!(hits.len(), 1);
    assert!(
        hits.first()
            .is_some_and(|hit| hit.starts_with("src/a.rs:1:")),
        "remaining double alias still flags first line"
    );
}

#[test]
fn aliased_thiserror_silent() {
    let files = vec![test_file(
        "src/a.rs",
        &["use thiserror::Error as ThisError;", "use foo::Error;"],
    )];
    assert!(
        first_occurrence_alias(&files).is_empty(),
        "aliased thiserror belongs to the dedicated lint"
    );
}
