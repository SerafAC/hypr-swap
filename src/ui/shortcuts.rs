//! `hyprland-global-shortcuts-v1`: the two named actions the user binds in their own
//! `hyprland.conf` (`contracts/shortcuts.md`, FR-022).
//!
//! A global shortcut is **anonymous** — the compositor never tells the client which keys trigger
//! it. That single fact is why commit-on-release has to discover the held modifiers from the
//! overlay's own keyboard focus rather than watching a configured key (research.md R3, R4).

/// Client bindings generated from the vendored protocol XML.
///
/// `wayland-scanner` is a procedural macro, so this expands at compile time; `build.rs` declares
/// the XML as a rebuild trigger.
#[allow(clippy::pedantic, clippy::all, missing_docs, unreachable_patterns)]
pub mod protocol {
    #![allow(unused_imports)]
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        // The generated interface tables name `wayland_backend`; wayland-client re-exports it,
        // which keeps it out of this crate's dependency list.
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-global-shortcuts-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-global-shortcuts-v1.xml");
}

pub use protocol::hyprland_global_shortcut_v1::HyprlandGlobalShortcutV1;
pub use protocol::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1;

/// Wayland user data for protocol objects whose events this application does not act on.
#[derive(Debug, Clone, Copy)]
pub struct NoData;

/// Wayland user data attached to each registered `hyprland_global_shortcut_v1`.
///
/// The shortcut's identity travels with the object, so the dispatcher knows which action fired
/// without keeping a side table to consult.
#[derive(Debug, Clone, Copy)]
pub struct ShortcutData(pub Shortcut);

/// Which of the two named shortcuts an event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// `hypr-swap:switcher` — hold to browse, release to switch.
    Switcher,
    /// `hypr-swap:new-workspace` — press.
    NewWorkspace,
}

impl Shortcut {
    /// Every shortcut this application registers, in registration order.
    pub const ALL: [Self; 2] = [Self::Switcher, Self::NewWorkspace];

    /// The `id` half of the `app_id` + `id` pair the compositor addresses.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Switcher => "switcher",
            Self::NewWorkspace => "new-workspace",
        }
    }

    /// Shown to the user by `hyprctl globalshortcuts`.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Switcher => "Open the workspace switcher",
            Self::NewWorkspace => "Switch to a new empty workspace",
        }
    }

    /// How the user triggers it, for the compositor to render.
    #[must_use]
    pub fn trigger_description(self) -> &'static str {
        match self {
            Self::Switcher => "Hold to browse, release to switch",
            Self::NewWorkspace => "Press",
        }
    }

    /// How the compositor's configuration names it: `hypr-swap:switcher`.
    #[must_use]
    pub fn qualified_name(self) -> String {
        format!("{}:{}", crate::APP_ID, self.id())
    }

    /// The exact bind line `docs/binds.md` documents for this shortcut (FR-022b).
    ///
    /// Kept here so `--help`, the documentation and the E2E harness cannot drift apart.
    #[must_use]
    pub fn suggested_bind(self) -> String {
        let combination = match self {
            Self::Switcher => "ALT, TAB",
            Self::NewWorkspace => "SUPER, N",
        };
        format!("bind = {combination}, global, {}", self.qualified_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_shortcut_ids_are_the_documented_ones() {
        assert_eq!(Shortcut::Switcher.id(), "switcher");
        assert_eq!(Shortcut::NewWorkspace.id(), "new-workspace");
        assert_eq!(Shortcut::Switcher.qualified_name(), "hypr-swap:switcher");
        assert_eq!(
            Shortcut::NewWorkspace.qualified_name(),
            "hypr-swap:new-workspace"
        );
    }

    #[test]
    fn bind_lines_use_bind_not_binde() {
        // A repeating bind fires the shortcut continuously while held, which the application
        // would read as continuous navigation (contracts/shortcuts.md).
        for shortcut in Shortcut::ALL {
            let line = shortcut.suggested_bind();
            assert!(line.starts_with("bind = "), "{line}");
            assert!(!line.starts_with("binde"), "{line}");
            assert!(line.contains(", global, "), "{line}");
            assert!(line.ends_with(&shortcut.qualified_name()), "{line}");
        }
    }

    #[test]
    fn the_documented_suggestions_are_alt_tab_and_super_n() {
        assert_eq!(
            Shortcut::Switcher.suggested_bind(),
            "bind = ALT, TAB, global, hypr-swap:switcher"
        );
        assert_eq!(
            Shortcut::NewWorkspace.suggested_bind(),
            "bind = SUPER, N, global, hypr-swap:new-workspace"
        );
    }

    #[test]
    fn every_shortcut_has_a_description_and_a_trigger_description() {
        for shortcut in Shortcut::ALL {
            assert!(!shortcut.description().is_empty());
            assert!(!shortcut.trigger_description().is_empty());
        }
    }
}
