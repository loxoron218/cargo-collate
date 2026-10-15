//! Declarative UI and CSS detectors for programmatic widgets.
//!
//! Parent index for the capability. The `builder_pattern` submodule flags
//! widget builders missing styling calls plus direct construction and
//! setters, the `missing_labels` submodule flags builders without
//! accessibility labels, the `missing_css_classes` submodule flags builder
//! and setter lists dropping required default classes, the
//! `missing_underline` submodule flags mnemonic `_X` label text without
//! underline enablement, `ui_files` combines stray files with code
//! references, `ui_scan` holds the byte-level reference scan, and `ui_ext`
//! holds extension matching so widgets stay built programmatically. Label
//! aliases resolve through imports while widget builders resolve through the
//! inventory, and nothing stays exempt.

pub mod builder_pattern;
pub mod missing_css_classes;
pub mod missing_labels;
pub mod missing_underline;
pub mod ui_ext;
pub mod ui_files;
pub mod ui_scan;
