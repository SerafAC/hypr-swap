//! The cached compositor view and the session-scoped activation history.
//!
//! The application owns exactly two pieces of state: the activation history and the switcher
//! session. Everything in [`World`] is a cache of what the compositor reports, rebuilt wholesale
//! on reconnect (FR-026b).

use crate::hypr::events::Event;
use crate::model::{Monitor, Window, Workspace};

/// The session-scoped most-recently-used record (FR-008c).
///
/// Fed **only** from observed compositor activations, never from this application's own commands.
/// That is what makes a switch made by the user's own keybind count, and a cancelled session not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivationHistory {
    order: Vec<i32>,
}

impl ActivationHistory {
    /// Move `id` to the front, removing any earlier occurrence.
    pub fn push(&mut self, id: i32) {
        self.order.retain(|known| *known != id);
        self.order.insert(0, id);
    }

    /// Forget a workspace that no longer exists.
    pub fn remove(&mut self, id: i32) {
        self.order.retain(|known| *known != id);
    }

    /// Most recently active first.
    #[must_use]
    pub fn order(&self) -> &[i32] {
        &self.order
    }

    /// Where `id` sits in the history, or `None` if it has never been active this session
    /// (FR-008d).
    #[must_use]
    pub fn position(&self, id: i32) -> Option<usize> {
        self.order.iter().position(|known| *known == id)
    }

    /// Discarded on connection loss and rebuilt from activations observed afterwards (FR-026c).
    pub fn clear(&mut self) {
        self.order.clear();
    }
}

/// What applying an event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The world now reflects the event.
    Incrementally,
    /// The event carries less than the world needs — a new workspace's monitor binding, a new
    /// window's geometry — so the caller must rebuild from `j/monitors`, `j/workspaces` and
    /// `j/clients`. Rebuilding is three small IPC round trips and always correct, which is why
    /// the alternative (guessing the missing fields) is not attempted.
    ByRebuilding,
}

/// The whole cached compositor view.
#[derive(Debug, Clone, Default)]
pub struct World {
    pub monitors: Vec<Monitor>,
    /// In compositor-reported order — this *is* "compositor order" for FR-008a.
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub history: ActivationHistory,
}

impl World {
    /// Replace the cached view wholesale, keeping the history (which the compositor does not
    /// report and this application must not lose on an ordinary refresh).
    pub fn rebuild(
        &mut self,
        monitors: Vec<Monitor>,
        workspaces: Vec<Workspace>,
        windows: Vec<Window>,
    ) {
        self.monitors = monitors;
        self.workspaces = workspaces;
        self.windows = windows;
        // A workspace that vanished while the application was not looking must not linger in the
        // history and reappear as a phantom entry.
        self.history
            .order
            .retain(|id| self.workspaces.iter().any(|w| w.id == *id));
    }

    #[must_use]
    pub fn focused_monitor(&self) -> Option<&Monitor> {
        self.monitors.iter().find(|monitor| monitor.focused)
    }

    #[must_use]
    pub fn monitor(&self, name: &str) -> Option<&Monitor> {
        self.monitors.iter().find(|monitor| monitor.name == name)
    }

    #[must_use]
    pub fn workspace(&self, id: i32) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    #[must_use]
    pub fn workspace_by_name(&self, name: &str) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.name == name)
    }

    /// The windows on a workspace, in the compositor's order — which is the order miniatures
    /// paint in, so floating windows land on top of the tiled ones beneath them.
    pub fn windows_on(&self, workspace_id: i32) -> impl Iterator<Item = &Window> {
        self.windows
            .iter()
            .filter(move |window| window.workspace == workspace_id)
    }

    /// Apply one compositor event, per the state-transition table in `data-model.md`.
    pub fn apply(&mut self, event: &Event) -> Applied {
        match event {
            Event::WorkspaceActivated { id, name } => {
                let Some(id) = id.or_else(|| self.workspace_by_name(name).map(|w| w.id)) else {
                    return Applied::ByRebuilding;
                };
                self.activate(id);
                Applied::Incrementally
            }
            Event::MonitorFocused { monitor, workspace_name } => {
                if self.monitor(monitor).is_none() {
                    return Applied::ByRebuilding;
                }
                for known in &mut self.monitors {
                    known.focused = known.name == *monitor;
                }
                match self.workspace_by_name(workspace_name).map(|w| w.id) {
                    Some(id) => {
                        self.activate(id);
                        Applied::Incrementally
                    }
                    None => Applied::ByRebuilding,
                }
            }
            // Neither carries what the world needs: a new workspace's monitor binding, or a new
            // window's geometry.
            Event::WorkspaceCreated { .. }
            | Event::WindowOpened { .. }
            // A window that changed workspace changed geometry with it.
            | Event::WindowMoved { .. }
            // Monitor changes reshuffle workspace bindings across the whole layout.
            | Event::MonitorsChanged => Applied::ByRebuilding,
            Event::WorkspaceDestroyed { name } => {
                let Some(id) = self.workspace_by_name(name).map(|w| w.id) else {
                    return Applied::Incrementally;
                };
                self.workspaces.retain(|workspace| workspace.id != id);
                self.windows.retain(|window| window.workspace != id);
                self.history.remove(id);
                Applied::Incrementally
            }
            Event::WorkspaceMoved { name, monitor } => {
                if self.monitor(monitor).is_none() {
                    return Applied::ByRebuilding;
                }
                match self.workspaces.iter_mut().find(|workspace| workspace.name == *name) {
                    Some(workspace) => {
                        workspace.monitor.clone_from(monitor);
                        Applied::Incrementally
                    }
                    None => Applied::ByRebuilding,
                }
            }
            Event::WindowClosed { address } => {
                self.windows.retain(|window| window.address != *address);
                self.recount_windows();
                Applied::Incrementally
            }
            Event::WindowTitleChanged { address, title } => {
                let Some(title) = title else {
                    return Applied::ByRebuilding;
                };
                match self.windows.iter_mut().find(|window| window.address == *address) {
                    Some(window) => {
                        window.title.clone_from(title);
                        Applied::Incrementally
                    }
                    // A title for a window this world has never seen means the world is behind.
                    None => Applied::ByRebuilding,
                }
            }
        }
    }

    /// Record an activation observed from the compositor: the focused monitor now shows this
    /// workspace, and the workspace goes to the front of the history.
    fn activate(&mut self, id: i32) {
        let monitor = self
            .workspace(id)
            .map(|workspace| workspace.monitor.clone());
        if let Some(monitor) = monitor
            && let Some(monitor) = self.monitors.iter_mut().find(|m| m.name == monitor)
        {
            monitor.active_workspace = id;
        }
        // Special and scratchpad workspaces never appear in the overlay, so they have no place
        // in the order it is built from.
        if self
            .workspace(id)
            .is_some_and(|workspace| !workspace.is_special())
        {
            self.history.push(id);
        }
    }

    /// Keep `window_count` — the FR-021 emptiness check — consistent after an incremental change.
    fn recount_windows(&mut self) {
        for workspace in &mut self.workspaces {
            workspace.window_count = u32::try_from(
                self.windows
                    .iter()
                    .filter(|window| window.workspace == workspace.id)
                    .count(),
            )
            .unwrap_or(u32::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, active: i32, focused: bool) -> Monitor {
        Monitor {
            id: 0,
            name: name.to_owned(),
            position: (0, 0),
            size: (1920, 1080),
            scale: 1.0,
            active_workspace: active,
            focused,
        }
    }

    fn workspace(id: i32, name: &str, monitor: &str, windows: u32) -> Workspace {
        Workspace {
            id,
            name: name.to_owned(),
            monitor: monitor.to_owned(),
            window_count: windows,
        }
    }

    fn window(address: &str, workspace: i32) -> Window {
        Window {
            address: address.to_owned(),
            title: "t".to_owned(),
            class: "c".to_owned(),
            workspace,
            at: (0, 0),
            size: (100, 100),
            floating: false,
            mapped: true,
        }
    }

    /// Two monitors, three ordinary workspaces and a scratchpad.
    fn world() -> World {
        let mut world = World::default();
        world.rebuild(
            vec![monitor("eDP-1", 1, true), monitor("HEADLESS-2", 2, false)],
            vec![
                workspace(1, "1", "eDP-1", 2),
                workspace(2, "2", "HEADLESS-2", 1),
                workspace(4, "mail", "HEADLESS-2", 0),
                workspace(-99, "special:scratchpad", "eDP-1", 1),
            ],
            vec![window("0xa", 1), window("0xb", 1), window("0xc", 2)],
        );
        world
    }

    // --- Activation history ------------------------------------------------

    #[test]
    fn push_moves_an_id_to_the_front_without_duplicating_it() {
        let mut history = ActivationHistory::default();
        history.push(1);
        history.push(2);
        history.push(3);
        assert_eq!(history.order(), &[3, 2, 1]);

        history.push(1);
        assert_eq!(
            history.order(),
            &[1, 3, 2],
            "1 moved to the front, it did not appear twice"
        );
        assert_eq!(history.order().len(), 3);
    }

    #[test]
    fn destroyed_ids_are_removed_from_the_history() {
        let mut history = ActivationHistory::default();
        history.push(1);
        history.push(2);
        history.remove(1);
        assert_eq!(history.order(), &[2]);
        history.remove(999);
        assert_eq!(history.order(), &[2], "removing an unknown id is harmless");
    }

    #[test]
    fn a_workspace_never_active_this_session_has_no_position() {
        let mut history = ActivationHistory::default();
        history.push(7);
        assert_eq!(history.position(7), Some(0));
        assert_eq!(
            history.position(9),
            None,
            "FR-008d: never active, so it sorts last"
        );
    }

    #[test]
    fn history_is_cleared_on_connection_loss() {
        // FR-026c: events missed while disconnected would leave a confidently wrong history.
        let mut world = world();
        world.apply(&Event::WorkspaceActivated {
            id: Some(2),
            name: "2".to_owned(),
        });
        assert!(!world.history.order().is_empty());
        world.history.clear();
        assert!(world.history.order().is_empty());
    }

    // --- Event transitions, one per row of the data-model table ------------

    #[test]
    fn workspace_activation_sets_the_active_workspace_and_pushes_history() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WorkspaceActivated {
                id: Some(4),
                name: "mail".into()
            }),
            Applied::Incrementally
        );
        assert_eq!(world.monitor("HEADLESS-2").unwrap().active_workspace, 4);
        assert_eq!(world.history.order(), &[4]);
    }

    #[test]
    fn workspace_activation_without_an_id_resolves_the_name() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WorkspaceActivated {
                id: None,
                name: "mail".into()
            }),
            Applied::Incrementally
        );
        assert_eq!(world.history.order(), &[4]);
    }

    #[test]
    fn an_activation_naming_an_unknown_workspace_asks_for_a_rebuild() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WorkspaceActivated {
                id: None,
                name: "brand-new".into()
            }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn special_workspaces_never_enter_the_history() {
        // FR-007: they are excluded from the overlay, so they have no place in its ordering.
        let mut world = world();
        world.apply(&Event::WorkspaceActivated {
            id: Some(-99),
            name: "special:scratchpad".into(),
        });
        assert!(world.history.order().is_empty());
    }

    #[test]
    fn focusing_a_monitor_moves_focus_and_pushes_its_workspace() {
        let mut world = world();
        let applied = world.apply(&Event::MonitorFocused {
            monitor: "HEADLESS-2".to_owned(),
            workspace_name: "2".to_owned(),
        });
        assert_eq!(applied, Applied::Incrementally);
        assert_eq!(world.focused_monitor().unwrap().name, "HEADLESS-2");
        assert_eq!(world.monitors.iter().filter(|m| m.focused).count(), 1);
        assert_eq!(world.history.order(), &[2]);
    }

    #[test]
    fn focusing_an_unknown_monitor_asks_for_a_rebuild() {
        let mut world = world();
        let applied = world.apply(&Event::MonitorFocused {
            monitor: "DP-9".to_owned(),
            workspace_name: "2".to_owned(),
        });
        assert_eq!(applied, Applied::ByRebuilding);
        assert_eq!(
            world.focused_monitor().unwrap().name,
            "eDP-1",
            "focus is unchanged"
        );
    }

    #[test]
    fn creating_a_workspace_asks_for_a_rebuild() {
        // The event carries no monitor binding, and guessing one would be wrong on the second
        // monitor.
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WorkspaceCreated { name: "5".into() }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn destroying_a_workspace_removes_it_its_windows_and_its_history_entry() {
        let mut world = world();
        world.apply(&Event::WorkspaceActivated {
            id: Some(2),
            name: "2".into(),
        });
        assert_eq!(world.history.order(), &[2]);

        let applied = world.apply(&Event::WorkspaceDestroyed {
            name: "2".to_owned(),
        });
        assert_eq!(applied, Applied::Incrementally);
        assert!(world.workspace(2).is_none());
        assert_eq!(world.windows_on(2).count(), 0);
        assert!(
            world.history.order().is_empty(),
            "no phantom entry survives"
        );
    }

    #[test]
    fn destroying_an_unknown_workspace_is_harmless() {
        let mut world = world();
        let before = world.workspaces.len();
        assert_eq!(
            world.apply(&Event::WorkspaceDestroyed {
                name: "nope".into()
            }),
            Applied::Incrementally
        );
        assert_eq!(world.workspaces.len(), before);
    }

    #[test]
    fn moving_a_workspace_rebinds_it_to_the_new_monitor() {
        let mut world = world();
        let applied = world.apply(&Event::WorkspaceMoved {
            name: "mail".to_owned(),
            monitor: "eDP-1".to_owned(),
        });
        assert_eq!(applied, Applied::Incrementally);
        assert_eq!(world.workspace(4).unwrap().monitor, "eDP-1");
    }

    #[test]
    fn moving_a_workspace_to_an_unknown_monitor_asks_for_a_rebuild() {
        let mut world = world();
        let applied = world.apply(&Event::WorkspaceMoved {
            name: "mail".to_owned(),
            monitor: "DP-9".to_owned(),
        });
        assert_eq!(applied, Applied::ByRebuilding);
        assert_eq!(
            world.workspace(4).unwrap().monitor,
            "HEADLESS-2",
            "binding is unchanged"
        );
    }

    #[test]
    fn opening_a_window_asks_for_a_rebuild_because_the_event_carries_no_geometry() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WindowOpened {
                address: "0xz".into()
            }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn closing_a_window_removes_it_and_updates_the_window_count() {
        let mut world = world();
        assert_eq!(world.workspace(1).unwrap().window_count, 2);
        let applied = world.apply(&Event::WindowClosed {
            address: "0xa".to_owned(),
        });
        assert_eq!(applied, Applied::Incrementally);
        assert_eq!(world.windows_on(1).count(), 1);
        assert_eq!(
            world.workspace(1).unwrap().window_count,
            1,
            "FR-021 reads this count"
        );
    }

    #[test]
    fn closing_the_last_window_leaves_an_empty_but_present_workspace() {
        let mut world = world();
        world.apply(&Event::WindowClosed {
            address: "0xc".to_owned(),
        });
        assert_eq!(world.workspace(2).unwrap().window_count, 0);
        assert!(
            world.workspace(2).is_some(),
            "FR-007: empty workspaces are still listed"
        );
    }

    #[test]
    fn moving_a_window_asks_for_a_rebuild() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WindowMoved {
                address: "0xa".into()
            }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn a_title_change_carrying_the_title_is_applied_in_place() {
        let mut world = world();
        let applied = world.apply(&Event::WindowTitleChanged {
            address: "0xa".to_owned(),
            title: Some("vim: main.rs".to_owned()),
        });
        assert_eq!(applied, Applied::Incrementally);
        assert_eq!(world.windows_on(1).next().unwrap().title, "vim: main.rs");
    }

    #[test]
    fn a_title_change_without_the_title_asks_for_a_rebuild() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WindowTitleChanged {
                address: "0xa".into(),
                title: None
            }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn a_title_change_for_an_unknown_window_asks_for_a_rebuild() {
        let mut world = world();
        assert_eq!(
            world.apply(&Event::WindowTitleChanged {
                address: "0xz".into(),
                title: Some("t".into())
            }),
            Applied::ByRebuilding
        );
    }

    #[test]
    fn monitor_changes_ask_for_a_rebuild() {
        let mut world = world();
        assert_eq!(world.apply(&Event::MonitorsChanged), Applied::ByRebuilding);
    }

    // --- Rebuild -----------------------------------------------------------

    #[test]
    fn a_rebuild_replaces_the_view_and_drops_history_for_workspaces_that_vanished() {
        let mut world = world();
        world.apply(&Event::WorkspaceActivated {
            id: Some(2),
            name: "2".into(),
        });
        world.apply(&Event::WorkspaceActivated {
            id: Some(1),
            name: "1".into(),
        });
        assert_eq!(world.history.order(), &[1, 2]);

        world.rebuild(
            vec![monitor("eDP-1", 1, true)],
            vec![workspace(1, "1", "eDP-1", 0)],
            vec![],
        );
        assert_eq!(world.monitors.len(), 1);
        assert_eq!(world.history.order(), &[1], "workspace 2 no longer exists");
    }

    #[test]
    fn windows_on_a_workspace_come_back_in_compositor_order() {
        let world = world();
        let addresses: Vec<&str> = world
            .windows_on(1)
            .map(|window| window.address.as_str())
            .collect();
        assert_eq!(addresses, vec!["0xa", "0xb"]);
    }
}
