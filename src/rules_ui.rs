//! Declarative UI and CSS detectors for programmatic widgets.
//!
//! Parent index for the capability. The `ui_files` submodule combines stray
//! files with code references, `ui_scan` holds the byte-level reference
//! scan, and `ui_ext` holds extension matching so widgets stay built
//! programmatically.

pub mod ui_ext;
pub mod ui_files;
pub mod ui_scan;
