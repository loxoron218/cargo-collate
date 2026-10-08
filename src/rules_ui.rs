//! Declarative UI and CSS detectors for programmatic widgets.
//!
//! Parent index for the capability. The `missing_labels` submodule flags
//! builders without accessibility labels, `ui_files` combines stray files
//! with code references, `ui_scan` holds the byte-level reference scan, and
//! `ui_ext` holds extension matching so widgets stay built programmatically.
//! Label aliases resolve through imports while widget builders resolve
//! through the inventory, and nothing stays exempt.

pub mod missing_labels;
pub mod ui_ext;
pub mod ui_files;
pub mod ui_scan;
