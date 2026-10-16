//! Init-time focus defaults for widget builders.
//!
//! Lists inventoried widget basenames whose C instance init sets
//! `can-focus` to `FALSE` on itself, so an explicit `.can_focus(false)`
//! restates the implementation there while staying load-bearing everywhere
//! else. Researched against GTK `main` (`ce6b23c`, 2026-10-15) and
//! libadwaita `main` (`0531b4c`, 2026-10-12):
//!
//! - `GtkWidget` base init sets `can-focus = TRUE`, `focusable = FALSE` (`gtk/gtkwidget.c`,
//!   `gtk_widget_init`).
//! - The only inventoried widget overriding itself to `FALSE` in init is `GtkWindowControls`
//!   (`gtk/gtkwindowcontrols.c`, `gtk_window_controls_init`). All other `set_can_focus` sites
//!   target child widgets (spin-button arrows, swatches, attention dots) or run conditionally at
//!   runtime (dialog hosts, split-view bins).
//! - `AdwSheetControls` also sets itself `FALSE` in init (`src/adw-sheet-controls.c`,
//!   `adw_sheet_controls_init`), but it exposes no public builder, so it stays out of the table.
//!
//! Refresh this table when the pinned versions move: grep upstream for
//! `gtk_widget_set_can_focus` and keep entries whose target is the widget
//! itself inside its `*_init` function.

/// Widget basenames whose init sets `can-focus = FALSE` on itself.
pub const UNFOCUSABLE_INIT: &[&str] = &["WindowControls"];

/// True if `name` sets `can-focus = FALSE` on itself in init.
///
/// # Arguments
///
/// * `name` - Widget basename such as `WindowControls`.
///
/// # Returns
///
/// * `bool` - True for init-unfocusable widgets.
#[must_use]
pub fn is_unfocusable_init(name: &str) -> bool {
    UNFOCUSABLE_INIT.contains(&name)
}

#[cfg(test)]
mod tests {
    use crate::rules_ui::builder_pattern::focus_defaults::is_unfocusable_init;

    #[test]
    fn init_unfocusable_known() {
        assert!(
            is_unfocusable_init("WindowControls"),
            "window controls restate init"
        );
        assert!(
            !is_unfocusable_init("Button"),
            "base default stays focusable"
        );
        assert!(!is_unfocusable_init("Box"), "containers stay silent");
    }
}
