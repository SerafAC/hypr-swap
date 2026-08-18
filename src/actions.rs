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

use std::fmt::Write as _;

use crate::model::{Monitor, MonitorName, Workspace};
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

impl ExpectedState {
    /// How the compositor's reported state differs from this one, or `None` if it matches.
    ///
    /// Only the monitors and workspaces a plan named are checked: a swap says nothing about the
    /// rest of the session, and a verification that failed on an unrelated change would roll back
    /// a swap that actually worked.
    ///
    /// This is also where FR-013's post-condition is enforced. Every monitor a plan touches
    /// appears in `active`, so requiring each of them to report the expected workspace *is* the
    /// requirement that neither monitor is left showing nothing.
    #[must_use]
    pub fn mismatch(&self, monitors: &[Monitor], workspaces: &[Workspace]) -> Option<String> {
        for (workspace, monitor) in &self.bindings {
            match workspaces.iter().find(|known| known.id == *workspace) {
                Some(known) if known.monitor == *monitor => {}
                Some(known) => {
                    return Some(format!(
                        "workspace {workspace} is on {} rather than {monitor}",
                        known.monitor
                    ));
                }
                None => return Some(format!("workspace {workspace} no longer exists")),
            }
        }
        for (monitor, workspace) in &self.active {
            match monitors.iter().find(|known| known.name == *monitor) {
                Some(known) if known.active_workspace == *workspace => {}
                Some(known) => {
                    return Some(format!(
                        "{monitor} is showing workspace {} rather than {workspace}",
                        known.active_workspace
                    ));
                }
                None => return Some(format!("monitor {monitor} no longer exists")),
            }
        }
        match monitors.iter().find(|known| known.focused) {
            Some(focused) if focused.name == self.focused => None,
            Some(focused) => Some(format!(
                "keyboard focus is on {} rather than {}",
                focused.name, self.focused
            )),
            None => Some("no monitor holds keyboard focus".to_owned()),
        }
    }

    /// Where the workspaces this state names have actually ended up, phrased for the FR-013c
    /// report: `workspace 4 is on HEADLESS-2 and workspace 2 is on eDP-1`.
    #[must_use]
    pub fn describe_actual(&self, workspaces: &[Workspace]) -> String {
        let mut described = String::new();
        for (index, (workspace, _)) in self.bindings.iter().enumerate() {
            let where_it_is = workspaces
                .iter()
                .find(|known| known.id == *workspace)
                .map_or_else(
                    || "gone".to_owned(),
                    |known| format!("on {}", known.monitor),
                );
            if index > 0 {
                described.push_str(" and ");
            }
            let _ = write!(described, "workspace {workspace} is {where_it_is}");
        }
        described
    }
}

/// What to dispatch for a committed selection, and what would undo it.
///
/// The commands go out as one batch, so the compositor applies them in a single pass and no
/// intermediate frame is ever presented (SC-010). A batch is not a transaction, though — a step
/// that fails leaves its predecessors applied (research.md R8 spike outcome) — which is why every
/// plan carries a rollback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    /// Dispatcher invocations without the `dispatch` keyword, e.g. `workspace 3`.
    pub commands: Vec<String>,
    pub expected: ExpectedState,
    /// How to undo it, computed from the pre-state *before* anything is sent (FR-013a).
    pub rollback: RollbackPlan,
}

impl CommandPlan {
    /// Whether this plan moves workspaces between monitors, as opposed to activating one on the
    /// monitor it is already bound to. Two monitors in the expectation is exactly that.
    #[must_use]
    pub fn is_swap(&self) -> bool {
        self.expected.active.len() > 1
    }
}

/// The undo for a [`CommandPlan`], and the state it must restore.
///
/// Not the literal inverse of the forward commands: the rollback runs precisely when the batch
/// half-applied, so an inverse that assumed the whole plan had landed would be wrong exactly when
/// it matters. These commands drive the compositor to the recorded pre-state from *any*
/// intermediate state, and re-running them changes nothing (research.md R8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPlan {
    pub commands: Vec<String>,
    /// The pre-state, which is what the rollback is verified against (FR-013c).
    pub expected: ExpectedState,
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

    // The selected workspace's monitor, resolved from the current world. `None` — because the
    // workspace is on the origin monitor already, or because its monitor has been unplugged —
    // is the same-monitor case, which is FR-009 and FR-027's degradation in one branch.
    let other = world
        .monitor(&workspace.monitor)
        .filter(|monitor| monitor.name != origin.name);

    Some(match other {
        None => activate(origin, selected),
        Some(other) => swap(world, origin, other, selected),
    })
}

/// Same-monitor activation: `workspace <id>` (research.md R8, first row).
fn activate(origin: &Monitor, selected: i32) -> CommandPlan {
    let previous = origin.active_workspace;
    let state = |workspace| ExpectedState {
        bindings: vec![(selected, origin.name.clone())],
        active: vec![(origin.name.clone(), workspace)],
        focused: origin.name.clone(),
    };
    CommandPlan {
        commands: vec![format!("workspace {selected}")],
        expected: state(selected),
        rollback: RollbackPlan {
            // One command cannot half-apply, so this really is the literal inverse — the general
            // pre-state restore below would be four commands saying the same thing.
            commands: vec![format!("workspace {previous}")],
            expected: state(previous),
        },
    }
}

/// Cross-monitor swap: the selected workspace comes to `origin` and the workspace it displaces
/// goes to `other`, with focus left on the selected workspace (FR-010, research.md R8 rows 2–3).
fn swap(world: &World, origin: &Monitor, other: &Monitor, selected: i32) -> CommandPlan {
    let displaced = origin.active_workspace;
    let (origin_name, other_name) = (origin.name.clone(), other.name.clone());

    let pre = ExpectedState {
        bindings: vec![
            (selected, other_name.clone()),
            (displaced, origin_name.clone()),
        ],
        active: vec![
            (origin_name.clone(), displaced),
            (other_name.clone(), other.active_workspace),
        ],
        // The monitor holding focus now, which need not be the one the overlay opened on.
        focused: world
            .focused_monitor()
            .map_or_else(|| origin_name.clone(), |monitor| monitor.name.clone()),
    };
    let expected = ExpectedState {
        bindings: vec![
            (selected, origin_name.clone()),
            (displaced, other_name.clone()),
        ],
        active: vec![
            (origin_name.clone(), selected),
            (other_name.clone(), displaced),
        ],
        focused: origin_name.clone(),
    };

    let commands = if other.active_workspace == selected {
        // The target is what the other monitor is showing, so Hyprland can exchange the two
        // itself rather than this application simulating one with a pair of moves.
        vec![
            format!("swapactiveworkspaces {origin_name} {other_name}"),
            format!("focusmonitor {origin_name}"),
        ]
    } else {
        // Moving the origin's *active* workspace away carries keyboard focus to the destination,
        // so focus has to be brought back before the last step — without it,
        // `focusworkspaceoncurrentmonitor` drags the selection to the wrong monitor
        // (research.md R8 spike outcome, finding 3).
        vec![
            format!("moveworkspacetomonitor {selected} {origin_name}"),
            format!("moveworkspacetomonitor {displaced} {other_name}"),
            format!("focusmonitor {origin_name}"),
            format!("focusworkspaceoncurrentmonitor {selected}"),
        ]
    };

    CommandPlan {
        commands,
        expected,
        rollback: RollbackPlan {
            commands: restore(&pre),
            expected: pre,
        },
    }
}

/// Commands that reach `state` from anywhere: bindings, then each monitor's active workspace,
/// then focus.
///
/// The order matters. `focusworkspaceoncurrentmonitor` *moves* a workspace that is bound to
/// another monitor, so every binding has to be back in place before any workspace is activated.
fn restore(state: &ExpectedState) -> Vec<String> {
    let mut commands = Vec::new();
    for (workspace, monitor) in &state.bindings {
        commands.push(format!("moveworkspacetomonitor {workspace} {monitor}"));
    }
    for (monitor, workspace) in &state.active {
        commands.push(format!("focusmonitor {monitor}"));
        commands.push(format!("focusworkspaceoncurrentmonitor {workspace}"));
    }
    commands.push(format!("focusmonitor {}", state.focused));
    commands
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

    /// Two monitors: eDP-1 focused showing 1, HEADLESS-2 showing 2. Workspace 3 is bound to
    /// eDP-1 and 4 to HEADLESS-2, neither of them on screen — which is what separates the two
    /// cross-monitor plan shapes.
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
                workspace(4, "HEADLESS-2"),
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
            plan.rollback.commands,
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

    // -----------------------------------------------------------------------------------------
    // Cross-monitor swaps (T057): the two plan shapes, their expectations and their rollbacks.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn selecting_the_other_monitors_active_workspace_swaps_the_two_monitors() {
        // FR-010, research.md R8 row 2: Hyprland exchanges them itself.
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        assert_eq!(
            plan.commands,
            vec![
                "swapactiveworkspaces eDP-1 HEADLESS-2",
                "focusmonitor eDP-1"
            ]
        );
        assert!(plan.is_swap());
    }

    #[test]
    fn the_swap_expects_both_workspaces_to_change_monitors_and_focus_to_stay_put() {
        // FR-010: the selection ends up on the origin monitor, focused; the workspace it
        // displaced ends up on the other monitor, which is FR-013's "neither monitor shows
        // nothing".
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        assert_eq!(
            plan.expected,
            ExpectedState {
                bindings: vec![(2, "eDP-1".to_owned()), (1, "HEADLESS-2".to_owned())],
                active: vec![("eDP-1".to_owned(), 2), ("HEADLESS-2".to_owned(), 1)],
                focused: "eDP-1".to_owned(),
            }
        );
    }

    #[test]
    fn selecting_a_workspace_not_shown_on_the_other_monitor_moves_both_and_refocuses() {
        // FR-010, research.md R8 row 3 as corrected by its spike: `focusmonitor` before
        // `focusworkspaceoncurrentmonitor`, because moving the origin's active workspace away
        // takes keyboard focus with it.
        let plan = plan(&two_monitors(), "eDP-1", 4).expect("a plan");
        assert_eq!(
            plan.commands,
            vec![
                "moveworkspacetomonitor 4 eDP-1",
                "moveworkspacetomonitor 1 HEADLESS-2",
                "focusmonitor eDP-1",
                "focusworkspaceoncurrentmonitor 4",
            ]
        );
        assert!(plan.is_swap());
    }

    #[test]
    fn the_inactive_target_shape_expects_the_same_end_state_as_the_active_one() {
        // The two shapes differ in how they get there, never in where they arrive — which is why
        // the verification path does not need to know which was used.
        let plan = plan(&two_monitors(), "eDP-1", 4).expect("a plan");
        assert_eq!(
            plan.expected,
            ExpectedState {
                bindings: vec![(4, "eDP-1".to_owned()), (1, "HEADLESS-2".to_owned())],
                active: vec![("eDP-1".to_owned(), 4), ("HEADLESS-2".to_owned(), 1)],
                focused: "eDP-1".to_owned(),
            }
        );
    }

    #[test]
    fn the_swap_rollback_restores_the_pre_state_rather_than_inverting_the_commands() {
        // FR-013a. Bindings first, then actives, then focus — the order that makes the batch
        // safe to run from a half-applied state (research.md R8).
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        assert_eq!(
            plan.rollback.commands,
            vec![
                "moveworkspacetomonitor 2 HEADLESS-2",
                "moveworkspacetomonitor 1 eDP-1",
                "focusmonitor eDP-1",
                "focusworkspaceoncurrentmonitor 1",
                "focusmonitor HEADLESS-2",
                "focusworkspaceoncurrentmonitor 2",
                "focusmonitor eDP-1",
            ]
        );
    }

    #[test]
    fn the_rollback_restores_the_other_monitors_own_workspace_not_the_selection() {
        // Shape 3's other monitor was showing 2, not the selected 4; a rollback that put 4 back
        // on screen there would be a change the user never asked for.
        let plan = plan(&two_monitors(), "eDP-1", 4).expect("a plan");
        assert_eq!(
            plan.rollback.expected,
            ExpectedState {
                bindings: vec![(4, "HEADLESS-2".to_owned()), (1, "eDP-1".to_owned())],
                active: vec![("eDP-1".to_owned(), 1), ("HEADLESS-2".to_owned(), 2)],
                focused: "eDP-1".to_owned(),
            }
        );
        assert!(
            plan.rollback
                .commands
                .contains(&"focusworkspaceoncurrentmonitor 2".to_owned())
        );
    }

    #[test]
    fn the_rollback_is_idempotent() {
        // It has to be: it runs against a state the application only partly knows.
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        let mut restored = two_monitors();
        restored.rebuild(
            vec![
                monitor(0, "eDP-1", 1, true),
                monitor(1, "HEADLESS-2", 2, false),
            ],
            restored.workspaces.clone(),
            vec![],
        );
        assert_eq!(
            plan.rollback
                .expected
                .mismatch(&restored.monitors, &restored.workspaces),
            None,
            "the pre-state is what the rollback aims at, so it is already satisfied here"
        );
    }

    #[test]
    fn with_one_monitor_a_cross_monitor_selection_degrades_to_plain_activation() {
        // FR-009, US2-AS5: a single-monitor session never swaps and never errors.
        let plan = plan(&single_monitor(), "eDP-1", 3).expect("a plan");
        assert_eq!(plan.commands, vec!["workspace 3"]);
        assert!(!plan.is_swap());
    }

    #[test]
    fn a_selection_bound_to_a_monitor_that_has_gone_degrades_to_activation() {
        // FR-027 again, from the other direction: the workspace still exists, so it is activated
        // on the origin monitor rather than swapped with a monitor that is not there.
        let mut world = two_monitors();
        world.rebuild(
            vec![monitor(0, "eDP-1", 1, true)],
            world.workspaces.clone(),
            vec![],
        );
        let plan = plan(&world, "eDP-1", 2).expect("a plan");
        assert_eq!(plan.commands, vec!["workspace 2"]);
        assert!(!plan.is_swap());
    }

    // -----------------------------------------------------------------------------------------
    // Verification and the FR-013c double failure (T058).
    // -----------------------------------------------------------------------------------------

    /// The world a failed swap of 1 and 2 can leave behind: workspace 2 moved to eDP-1, but
    /// workspace 1 never made it across.
    fn half_swapped() -> World {
        let mut world = World::default();
        world.rebuild(
            vec![
                monitor(0, "eDP-1", 2, true),
                monitor(1, "HEADLESS-2", 4, false),
            ],
            vec![
                workspace(1, "eDP-1"),
                workspace(2, "eDP-1"),
                workspace(3, "eDP-1"),
                workspace(4, "HEADLESS-2"),
            ],
            vec![],
        );
        world
    }

    #[test]
    fn a_state_that_matches_reports_no_mismatch() {
        let world = two_monitors();
        let plan = plan(&world, "eDP-1", 2).expect("a plan");
        assert_eq!(
            plan.rollback
                .expected
                .mismatch(&world.monitors, &world.workspaces),
            None
        );
    }

    #[test]
    fn a_workspace_on_the_wrong_monitor_is_a_mismatch() {
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        let world = half_swapped();
        let mismatch = plan
            .expected
            .mismatch(&world.monitors, &world.workspaces)
            .expect("workspace 1 never reached HEADLESS-2");
        assert!(mismatch.contains("workspace 1"), "{mismatch}");
        assert!(mismatch.contains("HEADLESS-2"), "{mismatch}");
    }

    #[test]
    fn a_monitor_showing_the_wrong_workspace_is_a_mismatch() {
        // FR-013's post-condition lives here: every monitor a plan touches must be showing the
        // workspace the plan put there.
        let expected = ExpectedState {
            bindings: vec![],
            active: vec![("HEADLESS-2".to_owned(), 2)],
            focused: "eDP-1".to_owned(),
        };
        let world = half_swapped();
        let mismatch = expected
            .mismatch(&world.monitors, &world.workspaces)
            .expect("HEADLESS-2 is showing 4");
        assert!(
            mismatch.contains("HEADLESS-2 is showing workspace 4"),
            "{mismatch}"
        );
    }

    #[test]
    fn keyboard_focus_on_the_wrong_monitor_is_a_mismatch() {
        // FR-010 asks for focus on the selection, so a swap that lands the workspaces but leaves
        // focus behind has not done what the user asked.
        let mut world = two_monitors();
        world.rebuild(
            vec![
                monitor(0, "eDP-1", 1, false),
                monitor(1, "HEADLESS-2", 2, true),
            ],
            world.workspaces.clone(),
            vec![],
        );
        let expected = ExpectedState {
            bindings: vec![],
            active: vec![],
            focused: "eDP-1".to_owned(),
        };
        let mismatch = expected
            .mismatch(&world.monitors, &world.workspaces)
            .expect("focus is on the wrong monitor");
        assert!(mismatch.contains("keyboard focus"), "{mismatch}");
    }

    #[test]
    fn a_vanished_monitor_or_workspace_is_a_mismatch_rather_than_a_match() {
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        let empty = World::default();
        assert!(
            plan.expected
                .mismatch(&empty.monitors, &empty.workspaces)
                .is_some()
        );
    }

    #[test]
    fn a_double_failure_leaves_both_the_plan_and_its_rollback_unsatisfied() {
        // FR-013c: when neither the plan nor its rollback describes the compositor's actual
        // state, there is nothing left to do but tell the user where their workspaces are. The
        // point of the test is that this case is *distinguishable* — it never reads as success.
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        let world = half_swapped();

        assert!(
            plan.expected
                .mismatch(&world.monitors, &world.workspaces)
                .is_some(),
            "the swap did not land"
        );
        assert!(
            plan.rollback
                .expected
                .mismatch(&world.monitors, &world.workspaces)
                .is_some(),
            "and neither did the rollback"
        );
        assert_eq!(
            plan.rollback.expected.describe_actual(&world.workspaces),
            "workspace 2 is on eDP-1 and workspace 1 is on eDP-1",
            "the resulting state, in the shape contracts/diagnostics.md reports it"
        );
    }

    #[test]
    fn the_resulting_state_names_a_workspace_that_has_disappeared_entirely() {
        let plan = plan(&two_monitors(), "eDP-1", 2).expect("a plan");
        let empty = World::default();
        assert_eq!(
            plan.rollback.expected.describe_actual(&empty.workspaces),
            "workspace 2 is gone and workspace 1 is gone"
        );
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
