//! Entry order and initial highlight (FR-007, FR-008a, FR-008b, FR-008d).
//!
//! A pure function of the world and one setting. Special and scratchpad workspaces are filtered
//! out before ordering, so nothing downstream has to remember they exist.

use crate::config::Order;
use crate::model::MonitorName;
use crate::state::World;

/// One window as it appears inside an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryWindow {
    /// The window's title, or its class when the title is empty.
    pub label: String,
    /// The program that owns the window, as the compositor reports it — the key the icon cache
    /// is built on (FR-040, research.md R21). Carried here rather than looked up at paint time
    /// so the renderer never reaches back into the world.
    pub class: String,
    /// Layout coordinates, global — miniatures subtract the entry's monitor origin.
    pub at: (i32, i32),
    pub size: (u32, u32),
    /// Painted above tiled windows.
    pub floating: bool,
}

/// One row or cell in the overlay. Derived; never stored across sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub workspace_id: i32,
    /// The workspace name, as the compositor reports it.
    pub label: String,
    pub windows: Vec<EntryWindow>,
    /// The monitor this workspace was bound to when the session opened.
    pub monitor: MonitorName,
    /// Geometry of that monitor. Miniatures normalise against the monitor the *workspace* is
    /// bound to, not the one the overlay is shown on, which is what makes an off-screen
    /// workspace's miniature as accurate as a visible one (FR-015a).
    pub monitor_position: (i32, i32),
    pub monitor_size: (u32, u32),
    /// The active workspace of its monitor — rendered distinctly (FR-008).
    pub is_active: bool,
}

/// Build the overlay's entries and the index the highlight starts on.
///
/// The highlight rule is the whole of FR-008b: MRU opens on the second entry so that one tap and
/// release returns the user to where they were; every other order opens on the active workspace.
#[must_use]
pub fn entries(world: &World, order: Order) -> (Vec<Entry>, usize) {
    let listed: Vec<i32> = match order {
        Order::Mru => mru_order(world),
        Order::Compositor => compositor_order(world),
        Order::Monitor => monitor_order(world),
    };

    let entries: Vec<Entry> = listed.iter().filter_map(|id| entry(world, *id)).collect();
    if entries.is_empty() {
        return (entries, 0);
    }

    let highlight = match order {
        // The second entry, or the only entry when that is all there is.
        Order::Mru => 1.min(entries.len() - 1),
        Order::Compositor | Order::Monitor => world
            .focused_monitor()
            .and_then(|monitor| {
                entries
                    .iter()
                    .position(|entry| entry.workspace_id == monitor.active_workspace)
            })
            .unwrap_or(0),
    };
    (entries, highlight)
}

/// Ordinary workspaces in the compositor's reported order (FR-007 filters the rest).
fn ordinary(world: &World) -> impl Iterator<Item = i32> {
    world
        .workspaces
        .iter()
        .filter(|workspace| !workspace.is_special())
        .map(|workspace| workspace.id)
}

fn compositor_order(world: &World) -> Vec<i32> {
    ordinary(world).collect()
}

/// History order first, then workspaces never active this session in compositor order (FR-008d).
fn mru_order(world: &World) -> Vec<i32> {
    let mut ids: Vec<i32> = ordinary(world).collect();
    ids.sort_by_key(|id| world.history.position(*id).unwrap_or(usize::MAX));
    ids
}

/// Grouped by monitor in the compositor's monitor order, stable within each group.
fn monitor_order(world: &World) -> Vec<i32> {
    let rank = |id: i32| {
        world
            .workspace(id)
            .and_then(|workspace| {
                world
                    .monitors
                    .iter()
                    .position(|monitor| monitor.name == workspace.monitor)
            })
            // A workspace whose monitor is not connected sorts after every group rather than
            // vanishing: FR-027 relies on it still being selectable.
            .unwrap_or(usize::MAX)
    };
    let mut ids: Vec<i32> = ordinary(world).collect();
    ids.sort_by_key(|id| rank(*id));
    ids
}

fn entry(world: &World, id: i32) -> Option<Entry> {
    let workspace = world.workspace(id)?;
    let monitor = world.monitor(&workspace.monitor);
    Some(Entry {
        workspace_id: workspace.id,
        label: workspace.name.clone(),
        windows: world
            .windows_on(id)
            .filter(|window| window.is_listed())
            .map(|window| EntryWindow {
                label: window.label().to_owned(),
                class: window.class.clone(),
                at: window.at,
                size: window.size,
                floating: window.floating,
            })
            .collect(),
        monitor: workspace.monitor.clone(),
        monitor_position: monitor.map_or((0, 0), |monitor| monitor.position),
        monitor_size: monitor.map_or((0, 0), |monitor| monitor.size),
        is_active: monitor.is_some_and(|monitor| monitor.active_workspace == id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Monitor, Window, Workspace};

    fn monitor(id: i32, name: &str, active: i32, focused: bool) -> Monitor {
        Monitor {
            id,
            name: name.to_owned(),
            position: (i32::from(id != 0) * 1920, 0),
            size: (1920, 1080),
            scale: 1.0,
            active_workspace: active,
            focused,
        }
    }

    fn workspace(id: i32, name: &str, monitor: &str) -> Workspace {
        Workspace {
            id,
            name: name.to_owned(),
            monitor: monitor.to_owned(),
            window_count: 0,
        }
    }

    fn window(address: &str, workspace: i32, title: &str, mapped: bool) -> Window {
        Window {
            address: address.to_owned(),
            title: title.to_owned(),
            class: "foot".to_owned(),
            workspace,
            at: (0, 0),
            size: (960, 1080),
            floating: false,
            mapped,
        }
    }

    /// Two monitors; workspaces 1 and 3 on eDP-1 (1 active, focused), 2 and 7 on HEADLESS-2
    /// (2 active), plus a scratchpad.
    fn world() -> World {
        let mut world = World::default();
        world.rebuild(
            vec![
                monitor(0, "eDP-1", 1, true),
                monitor(1, "HEADLESS-2", 2, false),
            ],
            vec![
                workspace(1, "1", "eDP-1"),
                workspace(2, "2", "HEADLESS-2"),
                workspace(3, "3", "eDP-1"),
                workspace(7, "mail", "HEADLESS-2"),
                workspace(-99, "special:scratchpad", "eDP-1"),
            ],
            vec![
                window("0xa", 1, "editor", true),
                window("0xb", 1, "notes", true),
                window("0xc", 2, "browser", true),
                window("0xd", 3, "hidden", false),
            ],
        );
        world
    }

    fn ids(entries: &[Entry]) -> Vec<i32> {
        entries.iter().map(|entry| entry.workspace_id).collect()
    }

    #[test]
    fn compositor_order_is_the_reported_order_with_the_highlight_on_the_active_workspace() {
        let (entries, highlight) = entries(&world(), Order::Compositor);
        assert_eq!(ids(&entries), vec![1, 2, 3, 7]);
        assert_eq!(highlight, 0, "workspace 1 is active on the focused monitor");
    }

    #[test]
    fn compositor_order_highlights_wherever_the_active_workspace_sits() {
        let mut world = world();
        world.monitors[0].active_workspace = 3;
        let (entries, highlight) = entries(&world, Order::Compositor);
        assert_eq!(entries[highlight].workspace_id, 3);
        assert_eq!(highlight, 2);
    }

    #[test]
    fn mru_order_lists_the_most_recent_first_and_highlights_the_second_entry() {
        // FR-008b: one tap and release returns the user to the workspace they were on.
        let mut world = world();
        world.history.push(3);
        world.history.push(7);
        world.history.push(1); // current

        let (entries, highlight) = entries(&world, Order::Mru);
        assert_eq!(ids(&entries), vec![1, 7, 3, 2]);
        assert_eq!(highlight, 1);
        assert_eq!(entries[highlight].workspace_id, 7);
    }

    #[test]
    fn workspaces_never_active_this_session_sort_last_in_compositor_order() {
        // FR-008d.
        let mut world = world();
        world.history.push(7);
        let (entries, _) = entries(&world, Order::Mru);
        assert_eq!(
            ids(&entries),
            vec![7, 1, 2, 3],
            "7 was used; the rest keep compositor order"
        );
    }

    #[test]
    fn with_no_history_at_all_mru_falls_back_to_compositor_order() {
        // The documented behaviour on a fresh start (spec Assumptions).
        let (mru, _) = entries(&world(), Order::Mru);
        let (compositor, _) = entries(&world(), Order::Compositor);
        assert_eq!(ids(&mru), ids(&compositor));
    }

    #[test]
    fn monitor_order_groups_by_monitor_and_highlights_the_active_workspace() {
        let (entries, highlight) = entries(&world(), Order::Monitor);
        assert_eq!(
            ids(&entries),
            vec![1, 3, 2, 7],
            "eDP-1's group, then HEADLESS-2's"
        );
        assert_eq!(entries[highlight].workspace_id, 1);
    }

    #[test]
    fn monitor_order_keeps_compositor_order_within_each_group() {
        let mut world = world();
        world.workspaces.reverse();
        let (entries, _) = entries(&world, Order::Monitor);
        assert_eq!(ids(&entries), vec![3, 1, 7, 2]);
    }

    #[test]
    fn a_workspace_bound_to_a_disconnected_monitor_still_appears_last() {
        let mut world = world();
        world.workspaces.push(workspace(9, "9", "DP-GONE"));
        let (entries, _) = entries(&world, Order::Monitor);
        assert_eq!(ids(&entries).last(), Some(&9));
    }

    #[test]
    fn special_workspaces_are_excluded_from_every_order() {
        // FR-007.
        for order in [Order::Mru, Order::Compositor, Order::Monitor] {
            let (entries, _) = entries(&world(), order);
            assert!(
                !ids(&entries).contains(&-99),
                "{order:?} listed the scratchpad"
            );
        }
    }

    #[test]
    fn a_single_workspace_clamps_the_mru_highlight_to_the_only_entry() {
        // spec Edge Cases: the overlay opens showing that single entry.
        let mut world = World::default();
        world.rebuild(
            vec![monitor(0, "eDP-1", 1, true)],
            vec![workspace(1, "1", "eDP-1")],
            vec![],
        );
        let (entries, highlight) = entries(&world, Order::Mru);
        assert_eq!(entries.len(), 1);
        assert_eq!(highlight, 0);
    }

    #[test]
    fn no_workspaces_at_all_yields_no_entries_and_a_zero_highlight() {
        let (entries, highlight) = entries(&World::default(), Order::Mru);
        assert!(entries.is_empty());
        assert_eq!(highlight, 0);
    }

    #[test]
    fn an_entry_carries_its_windows_labels_and_its_monitors_geometry() {
        let (entries, _) = entries(&world(), Order::Compositor);
        let first = &entries[0];
        assert_eq!(first.label, "1");
        assert_eq!(
            first
                .windows
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>(),
            vec!["editor", "notes"]
        );
        assert_eq!(first.monitor, "eDP-1");
        assert_eq!(first.monitor_position, (0, 0));
        assert_eq!(first.monitor_size, (1920, 1080));
        assert!(first.is_active);

        let second = &entries[1];
        assert_eq!(
            second.monitor_position,
            (1920, 0),
            "normalised against its own monitor"
        );
        assert!(second.is_active, "workspace 2 is active on HEADLESS-2");
        assert!(!entries[2].is_active);
    }

    #[test]
    fn unmapped_windows_are_left_out_of_entries() {
        let (entries, _) = entries(&world(), Order::Compositor);
        let workspace_three = entries
            .iter()
            .find(|e| e.workspace_id == 3)
            .expect("workspace 3");
        assert!(
            workspace_three.windows.is_empty(),
            "its only window is unmapped"
        );
    }

    #[test]
    fn an_empty_workspace_is_listed_like_any_other() {
        // FR-007.
        let (entries, _) = entries(&world(), Order::Compositor);
        let mail = entries
            .iter()
            .find(|e| e.workspace_id == 7)
            .expect("workspace 7 is listed");
        assert!(mail.windows.is_empty());
    }

    #[test]
    fn every_order_lists_exactly_the_same_workspaces() {
        let world = world();
        let mut expected = ids(&entries(&world, Order::Compositor).0);
        expected.sort_unstable();
        for order in [Order::Mru, Order::Monitor] {
            let mut observed = ids(&entries(&world, order).0);
            observed.sort_unstable();
            assert_eq!(observed, expected, "{order:?}");
        }
    }

    #[test]
    fn the_initial_highlight_is_always_inside_the_entry_list() {
        let world = world();
        for order in [Order::Mru, Order::Compositor, Order::Monitor] {
            let (entries, highlight) = entries(&world, order);
            assert!(
                highlight < entries.len(),
                "{order:?} highlighted {highlight}"
            );
        }
    }
}
