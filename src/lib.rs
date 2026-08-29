//! Alt-Tab style workspace switcher with cross-monitor swapping for Hyprland.
//!
//! The modules below are the seam that matters for testing: `config`, `model`, `state`,
//! `ordering`, `actions`, `session`, `theme` and `ui::layout` are I/O-free and unit-tested
//! directly; `hypr::ipc`, `hypr::events` and `icons::*` do I/O but keep their decision rules
//! separable and are unit-tested too — the icon modules against a fixture root rather than
//! whatever the developer has installed. Only `main.rs` and `ui::{mod, shortcuts, render}` are the
//! thin Wayland/cairo shell, covered by the nested-compositor E2E suite instead (plan.md →
//! Complexity Tracking).

pub mod actions;
pub mod config;
pub mod diag;
pub mod hypr;
pub mod icons;
pub mod model;
pub mod ordering;
pub mod session;
pub mod state;
pub mod theme;
pub mod ui;

/// The application's own name, used as the global-shortcut `app_id`, the layer-shell namespace,
/// and the notification application name.
pub const APP_ID: &str = "hypr-swap";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
