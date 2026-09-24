//! Mixed mod-group separation for file modules.
//!
//! Parent index for the capability. Collection lives in `mod_collect` while
//! grouping checks live in `group`; this file keeps group ranks plus the
//! driver.

pub mod attr;
pub mod group;
pub mod mod_collect;

#[cfg(test)]
mod group_cases;

use crate::{rules_tests::mod_groups::group::check_file, scan::SourceFile};

/// Rank of `#[cfg(test)]` file modules at the end.
pub const CFG_TEST_GROUP: u8 = 2;

/// Rank of `#[macro_use]` file modules before ordinary modules.
pub const MACRO_USE_GROUP: u8 = 0;

/// Rank of ordinary `pub mod` and `mod` file declarations sharing one group.
pub const ORDINARY_GROUP: u8 = 1;

/// Mixed mod groups across every file-module declaration in all files.
///
/// Groups stay ordered `macro_use`, ordinary, `cfg(test)`, separated by blank
/// lines with no blanks inside a group.
pub fn mixed_mod_groups(files: &[SourceFile]) -> Vec<String> {
    files.iter().flat_map(check_file).collect()
}
