//! A committed selection turned into compositor commands (FR-009, FR-011, research.md R8).
//!
//! Pure: this module decides *what* to dispatch and what would undo it; `hypr::ipc` does the
//! dispatching and `main.rs` decides when. That split is what lets every plan shape be asserted
//! without a compositor.
//!
//! The one rule that makes FR-027 fall out rather than needing a special case: a plan resolves the
//! selected workspace's monitor from the **current** world, never from the `Entry` snapshot the
//! overlay was built from. A snapshot monitor that has since been unplugged therefore lands in
//! the same-monitor activation shape — a degradation, not a cancellation.

use crate::model::MonitorName;
use crate::state::World;

/// The state a plan is expected to leave the compositor in.
///
/// Recorded before dispatch so the result can be read back and compared (FR-013a). A same-monitor
/// activation is not at risk of a half-applied state, but it carries its expectation for the same
/// reason the cross-monitor shapes do: one plan type, one verification path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedState {
    /// Workspace id → the monitor it must be bound to afterwards.
    pub bindings: Vec<(i32, MonitorName)>,
    /// Monitor → the workspace it must be showing afterwards.
    pub active: Vec<(MonitorName, i32)>,
    /// The monitor that must hold keyboard focus afterwards.
    pub focused: MonitorName,
}

/// What to dispatch for a committed selection, and what would undo it.
///
/// The commands go out as one batch, so the compositor applies them in a single pass and no
/// intermediate state is ever presented (SC-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    /// Dispatcher invocations without the `dispatch` keyword, e.g. `workspace 3`.
    pub commands: Vec<String>,
    pub expected: ExpectedState,
    /// The inverse batch, computed from the pre-state *before* anything is sent (FR-013a).
    pub rollback: Vec<String>,
}

/// Plan the outcome of activating `selected`, as seen from `origin_monitor`.
///
/// `origin_monitor` is the monitor that was focused when the overlay opened. If it has since been
/// disconnected, the currently focused monitor stands in for it — which is FR-027's "degrade to
/// plain activation" for the monitor-removed case.
///
/// Returns `None` for a no-op: the selection is already the active workspace of the origin
/// monitor (FR-011), or the workspace no longer exists at all (FR-027).
#[must_use]
pub fn plan(world: &World, origin_monitor: &str, selected: i32) -> Option<CommandPlan> {
    let workspace = world.workspace(selected)?;
    // A workspace that never should have been listed cannot be a target (FR-007).
    if workspace.is_special() {
        return None;
    }

    let origin = world
        .monitor(origin_monitor)
        .or_else(|| world.focused_monitor())?;

    // FR-011: selecting the workspace already on screen does nothing at all — no dispatch, no
    // diagnostic. The user asked for the state they are already in.
    if origin.active_workspace == selected {
        return None;
    }

    // Same-monitor activation: `workspace <id>` (research.md R8, first row). Phase 4 (T056) adds
    // the two cross-monitor shapes; until then every selection takes this path, which is complete
    // and correct for the single-monitor case US1 is specified against.
    Some(CommandPlan {
        commands: vec![format!("workspace {selected}")],
        expected: ExpectedState {
            bindings: vec![(selected, origin.name.clone())],
            active: vec![(origin.name.clone(), selected)],
            focused: origin.name.clone(),
        },
        // Trivial, and in practice never needed: activating a workspace cannot half-succeed.
        // It exists so the verify-then-undo path has one shape to handle rather than two.
        rollback: vec![format!("workspace {}", origin.active_workspace)],
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
            position: (id * 1920, 0),
            size: (1920, 1080),
            scale: 1.0,
            active_workspace: active,
            focused,
        }
    }

    fn workspace(id: i32, monitor: &str) -> Workspace {
        Workspace {
            id,
            name: id.to_string(),
            monitor: monitor.to_owned(),
            window_count: 0,
        }
    }

    /// One monitor, workspaces 1 (active), 2 and 3.
    fn single_monitor() -> World {
        let mut world = World::default();
        world.rebuild(
            vec![monitor(0, "eDP-1", 1, true)],
            vec![
                workspace(1, "eDP-1"),
                workspace(2, "eDP-1"),
                workspace(3, "eDP-1"),
            ],
            vec![],
        );
        world
    }

    /// Two monitors: eDP-1 focused showing 1, HEADLESS-2 showing 2.
    fn two_monitors() -> World {
        let mut world = World::default();
        world.rebuild(
            vec![
                monitor(0, "eDP-1", 1, true),
                monitor(1, "HEADLESS-2", 2, false),
            ],
            vec![
                workspace(1, "eDP-1"),
                workspace(2, "HEADLESS-2"),
                workspace(3, "eDP-1"),
            ],
            vec![],
        );
        world
    }

    #[test]
    fn selecting_another_workspace_on_the_same_monitor_activates_it() {
        // FR-009, research.md R8 row 1.
        let plan = plan(&single_monitor(), "eDP-1", 3).expect("a plan");
        assert_eq!(plan.commands, vec!["workspace 3"]);
    }

    #[test]
    fn the_plan_records_the_state_it_expects_to_produce() {
        let plan = plan(&single_monitor(), "eDP-1", 3).expect("a plan");
        assert_eq!(
            plan.expected,
            ExpectedState {
                bindings: vec![(3, "eDP-1".to_owned())],
                active: vec![("eDP-1".to_owned(), 3)],
                focused: "eDP-1".to_owned(),
            }
        );
    }

    #[test]
    fn the_rollback_returns_to_the_workspace_that_was_showing() {
        let plan = plan(&single_monitor(), "eDP-1", 3).expect("a plan");
        assert_eq!(
            plan.rollback,
            vec!["workspace 1"],
            "computed from the pre-state, before anything is sent"
        );
    }

    #[test]
    fn selecting_the_workspace_already_on_screen_is_a_no_op() {
        // FR-011, US1-AS7: no dispatch and no diagnostic.
        assert_eq!(plan(&single_monitor(), "eDP-1", 1), None);
    }

    #[test]
    fn selecting_a_workspace_that_no_longer_exists_is_a_no_op() {
        // FR-027. `session::target` filters this first; `plan` does not rely on it having done so.
        assert_eq!(plan(&single_monitor(), "eDP-1", 9), None);
    }

    #[test]
    fn a_special_workspace_is_never_a_target() {
        // FR-007: it is excluded from the entries, so it can only arrive here by mistake.
        let mut world = single_monitor();
        world.workspaces.push(workspace(-99, "eDP-1"));
        assert_eq!(plan(&world, "eDP-1", -99), None);
    }

    #[test]
    fn a_snapshot_monitor_that_has_gone_degrades_to_activation_on_the_focused_monitor() {
        // FR-027's other half: the workspace survives, the monitor it was listed under does not.
        // This must activate, not cancel.
        let world = single_monitor();
        let plan = plan(&world, "DP-UNPLUGGED", 3).expect("a plan rather than a cancellation");
        assert_eq!(plan.commands, vec!["workspace 3"]);
        assert_eq!(
            plan.expected.focused, "eDP-1",
            "the currently focused monitor stands in for the one that went away"
        );
    }

    #[test]
    fn the_degraded_case_still_honours_the_already_active_no_op() {
        // Falling back to the focused monitor must not turn a no-op into a dispatch.
        assert_eq!(plan(&single_monitor(), "DP-UNPLUGGED", 1), None);
    }

    #[test]
    fn with_no_monitors_at_all_there_is_nothing_to_plan() {
        let mut world = World::default();
        world.rebuild(vec![], vec![workspace(1, "eDP-1")], vec![]);
        assert_eq!(plan(&world, "eDP-1", 1), None);
    }

    #[test]
    fn a_workspace_active_on_another_monitor_is_not_the_origins_no_op() {
        // Workspace 2 is active on HEADLESS-2, but the user is on eDP-1, so this is a real
        // selection. Phase 3 activates it; T056 turns this case into a swap.
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        assert_eq!(plan.commands, vec!["workspace 2"]);
        assert_eq!(plan.rollback, vec!["workspace 1"]);
    }

    #[test]
    fn windows_on_a_workspace_do_not_affect_its_plan() {
        // The plan is about workspaces and monitors; window contents are the overlay's business.
        let mut world = single_monitor();
        world.windows.push(Window {
            address: "0xa".to_owned(),
            title: "editor".to_owned(),
            class: "foot".to_owned(),
            workspace: 3,
            at: (0, 0),
            size: (960, 1080),
            floating: false,
            mapped: true,
        });
        assert_eq!(
            plan(&world, "eDP-1", 3).map(|plan| plan.commands),
            Some(vec!["workspace 3".to_owned()])
        );
    }
}
