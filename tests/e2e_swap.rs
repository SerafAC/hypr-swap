//! User Story 2 — swapping workspaces between monitors.
//!
//! The second monitor is a headless output, the documented substitute for a physical one
//! (research.md R14); everything else is real, and every assertion is read back from the nested
//! compositor's own IPC.
//!
//! What a test can observe is the layout — which workspace each monitor is showing and where
//! keyboard focus is — so that is what these scenarios assert on, before and after a release, and
//! in [`e2e_swap_active_workspaces`] continuously *during* one (SC-010).

mod e2e;

use std::time::Duration;

use e2e::clients;
use e2e::harness::{Layout, Nested, Setup};
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};
use hypr_swap::hypr::ipc::FAULT_INJECTION_VAR;

/// Long enough for the overlay to map and take keyboard focus between taps (see
/// `tests/e2e_switcher.rs`).
const SETTLE: Duration = Duration::from_millis(200);

/// The monitor the generated harness configuration always creates.
const PRIMARY: &str = "WAYLAND-1";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert the compositor lists its ordinary workspaces in the order the scenario assumes.
///
/// With an empty activation history, MRU order *is* compositor order, so this is what fixes which
/// entry the overlay opens highlighting — and therefore which workspace a bare hold-and-release
/// selects (FR-008b).
fn assert_compositor_order(nested: &Nested, expected: &[i32]) {
    let ids: Vec<i32> = nested
        .workspaces()
        .into_iter()
        .filter(|workspace| !workspace.is_special())
        .map(|workspace| workspace.id)
        .collect();
    assert_eq!(ids, expected, "the compositor's reported workspace order");
}

/// Move a workspace onto a monitor and leave it showing there.
fn show(nested: &Nested, monitor: &str, workspace: i32) {
    nested.dispatch(&format!("focusmonitor {monitor}"));
    nested.dispatch(&format!("focusworkspaceoncurrentmonitor {workspace}"));
    nested.wait_until(&format!("{monitor} shows workspace {workspace}"), || {
        nested.active_workspace_on(monitor) == Some(workspace)
    });
}

/// Hold the switcher combination, tap once to open on the initial highlight, and release.
fn hold_tap_release(nested: &Nested, keyboard: &mut Keyboard) {
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 1, SETTLE);
    nested.wait_until("the overlay closes again", || {
        nested.overlay_monitors().is_empty()
    });
}

/// Where every window is, so a swap can be shown to have moved none of them (FR-012).
fn assert_windows_unmoved(nested: &Nested, before: &[(String, i32)]) {
    assert_eq!(
        clients::inventory(nested),
        before,
        "a swap moves workspaces between monitors, never windows between workspaces"
    );
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[test]
fn e2e_swap_active_workspaces() {
    // FR-010, FR-012, FR-013, US2-AS1, and compositor-ipc assumption 4.
    let nested = Nested::start();
    let other = nested.add_headless_output();

    let _w1 = clients::spawn_on(&nested, 1, "on-1");
    show(&nested, &other, 2);
    let _w2 = clients::spawn(&nested, "on-2");
    show(&nested, PRIMARY, 1);
    assert_compositor_order(&nested, &[1, 2]);

    let before = nested.layout();
    let windows = clients::inventory(&nested);
    assert_eq!(before.on(PRIMARY), Some(1));
    assert_eq!(before.on(&other), Some(2));

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // The highlight opens on the second entry, which is workspace 2 — active on the other
    // monitor, so this is the `swapactiveworkspaces` shape.
    let sampler = nested.sample_layout();
    hold_tap_release(&nested, &mut keyboard);

    nested.wait_until("workspace 2 arrives on the primary monitor", || {
        nested.active_workspace_on(PRIMARY) == Some(2)
    });
    let after = nested.layout();
    let observed = sampler.stop();

    // FR-010: the two workspaces have exchanged monitors and focus is on the selection.
    assert_eq!(after.on(&other), Some(1), "the displaced workspace moved");
    assert_eq!(nested.monitor_of(2).as_deref(), Some(PRIMARY));
    assert_eq!(nested.monitor_of(1).as_deref(), Some(other.as_str()));
    assert_eq!(after.focused.as_deref(), Some(PRIMARY));

    // The sampler has to have been awake across the change, or "nothing bad was seen" would be
    // a claim about nothing at all.
    assert!(
        observed.contains(&before) && observed.contains(&after),
        "the sampler missed the transition entirely: {observed:?}"
    );

    // FR-013 / SC-010: every layout that existed while this happened was either the one before
    // or the one after — never a monitor showing nothing, never half a swap.
    let unexpected: Vec<&Layout> = observed
        .iter()
        .filter(|layout| **layout != before && **layout != after)
        .collect();
    assert!(
        unexpected.is_empty(),
        "a half-swapped layout was observable: {unexpected:?} (before {before:?}, after {after:?})"
    );

    assert_windows_unmoved(&nested, &windows);
}

#[test]
fn e2e_swap_inactive_target() {
    // FR-010, US2-AS2, and the research.md R8 [spike] behaviour of `moveworkspacetomonitor`
    // followed by an explicit refocus.
    let nested = Nested::start();
    let other = nested.add_headless_output();

    // Workspace 3 is bound to the other monitor but shown nowhere: created there, then displaced
    // by workspace 2. Creation order also fixes the compositor order, and therefore that the
    // overlay opens highlighting 3.
    let _w1 = clients::spawn_on(&nested, 1, "on-1");
    show(&nested, &other, 3);
    let _w3 = clients::spawn(&nested, "on-3");
    show(&nested, &other, 2);
    let _w2 = clients::spawn(&nested, "on-2");
    show(&nested, PRIMARY, 1);
    assert_compositor_order(&nested, &[1, 3, 2]);

    assert_eq!(nested.monitor_of(3).as_deref(), Some(other.as_str()));
    assert_eq!(nested.active_workspace_on(&other), Some(2));
    let windows = clients::inventory(&nested);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    hold_tap_release(&nested, &mut keyboard);

    nested.wait_until("workspace 3 arrives on the primary monitor", || {
        nested.active_workspace_on(PRIMARY) == Some(3)
    });
    let after = nested.layout();

    assert_eq!(nested.monitor_of(3).as_deref(), Some(PRIMARY));
    assert_eq!(
        nested.monitor_of(1).as_deref(),
        Some(other.as_str()),
        "the workspace it displaced went the other way"
    );
    assert_eq!(
        after.on(&other),
        Some(1),
        "FR-013: the other monitor is showing something, and it is the displaced workspace"
    );
    assert_eq!(
        after.focused.as_deref(),
        Some(PRIMARY),
        "FR-010 leaves keyboard focus on the selection, which the R8 spike showed needs an \
         explicit focusmonitor before the final activation"
    );
    assert_eq!(
        nested.monitor_of(2).as_deref(),
        Some(other.as_str()),
        "the workspace that was merely displaced from view is not dragged along"
    );
    assert_windows_unmoved(&nested, &windows);
}

#[test]
fn e2e_swap_single_monitor_degrades() {
    // FR-009, US2-AS5: with one output there is nothing to swap with, so the same gesture is a
    // plain activation — and specifically not an error.
    let nested = Nested::start();
    let _w1 = clients::spawn_on(&nested, 1, "on-1");
    let _w2 = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    assert_compositor_order(&nested, &[1, 2]);
    let windows = clients::inventory(&nested);

    let daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    hold_tap_release(&nested, &mut keyboard);

    nested.wait_until("workspace 2 becomes active", || {
        nested.active_workspace() == 2
    });
    assert_eq!(nested.monitor_of(1).as_deref(), Some(PRIMARY));
    assert_eq!(nested.monitor_of(2).as_deref(), Some(PRIMARY));
    assert_windows_unmoved(&nested, &windows);

    let stderr = daemon.stderr();
    assert!(
        !stderr.contains("ERROR"),
        "a single-monitor selection is an ordinary activation:\n{stderr}"
    );
}

#[test]
fn e2e_swap_rollback_on_failure() {
    // FR-013a, FR-013b, SC-010, US2-AS6. The failure is injected because a dispatcher cannot be
    // made to fail from outside the compositor — the one place an E2E test reaches past the real
    // interface (research.md R14).
    let nested = Nested::start();
    let other = nested.add_headless_output();

    let _w1 = clients::spawn_on(&nested, 1, "on-1");
    show(&nested, &other, 3);
    let _w3 = clients::spawn(&nested, "on-3");
    show(&nested, &other, 2);
    let _w2 = clients::spawn(&nested, "on-2");
    show(&nested, PRIMARY, 1);
    assert_compositor_order(&nested, &[1, 3, 2]);

    let before = nested.layout();
    let bindings: Vec<(i32, String)> = nested
        .workspaces()
        .into_iter()
        .map(|workspace| (workspace.id, workspace.monitor))
        .collect();
    let windows = clients::inventory(&nested);

    // Step 2 of the four-command plan is the move that carries the displaced workspace across,
    // so dropping it leaves a genuinely half-applied swap for the rollback to repair.
    let daemon = nested.start_daemon_with_env(&[], &[(FAULT_INJECTION_VAR, "2")]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    hold_tap_release(&nested, &mut keyboard);

    // FR-013a: the layout the user had back, in full — bindings, active workspaces and focus.
    nested.wait_until("the layout is restored", || nested.layout() == before);
    let restored: Vec<(i32, String)> = nested
        .workspaces()
        .into_iter()
        .map(|workspace| (workspace.id, workspace.monitor))
        .collect();
    assert_eq!(restored, bindings, "every monitor binding is back");
    assert_windows_unmoved(&nested, &windows);

    // FR-013b: the user asked for a change that did not happen, so they are told.
    let stderr = daemon.stderr();
    assert!(stderr.contains("ERROR swap:"), "{stderr}");
    assert!(stderr.contains("rolled back"), "{stderr}");
    assert!(
        !stderr.contains("rollback failed"),
        "the rollback itself worked, so FR-013c must not be reported:\n{stderr}"
    );
}

#[test]
#[ignore = "soak: a hundred swaps, minutes rather than seconds"]
fn soak() {
    // SC-003, US2-AS4: repeated swapping neither loses a window nor drifts the layout.
    //
    // `order = "compositor"` is what makes a hundred passes deterministic. The entry list is
    // then fixed at [1, 2] and the highlight opens on whatever the primary monitor is showing,
    // so "open, advance once, release" always selects the workspace on the *other* monitor. MRU
    // cannot do that: after a swap both workspaces have just been activated, and which of them
    // the history puts first decides whether the next gesture swaps or is an FR-011 no-op.
    let nested =
        Nested::start_with(&Setup::documented().with_app_config("order = \"compositor\"\n"));
    let other = nested.add_headless_output();

    let _w1 = clients::spawn_on(&nested, 1, "on-1");
    show(&nested, &other, 2);
    let _w2 = clients::spawn(&nested, "on-2");
    show(&nested, PRIMARY, 1);
    assert_compositor_order(&nested, &[1, 2]);

    let windows = clients::inventory(&nested);
    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    for pass in 1..=100 {
        let expected = nested
            .active_workspace_on(&other)
            .unwrap_or_else(|| panic!("pass {pass}: the other monitor shows nothing"));

        keyboard.hold(KEY_LEFTALT);
        for _ in 0..2 {
            keyboard.tap_while_held(KEY_TAB);
            std::thread::sleep(SETTLE);
        }
        keyboard.release(KEY_LEFTALT);
        keyboard.settle();

        nested.wait_until(
            &format!("pass {pass} puts workspace {expected} on the primary"),
            || nested.active_workspace_on(PRIMARY) == Some(expected),
        );
        assert_eq!(
            clients::inventory(&nested),
            windows,
            "pass {pass} lost or moved a window"
        );
    }
}
