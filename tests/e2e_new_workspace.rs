//! User Story 4 — the new-workspace shortcut.
//!
//! The second of the two named shortcuts is a plain press with no overlay: it switches to the
//! lowest workspace number not currently in use, bound to the focused monitor (FR-020), and does
//! nothing at all when the workspace already on screen is empty (FR-021).
//!
//! Both halves are observed the same way as the rest of the suite — real key events through
//! `virtual-keyboard-unstable-v1` against a nested Hyprland running the documented `SUPER, N` bind
//! line, with every assertion read back from that compositor's own IPC (research.md R14). The
//! no-op is the harder claim to make honestly: "nothing happened" is only meaningful after the
//! compositor has had a fair chance to act, so [`e2e_new_workspace_noop_when_empty`] waits before
//! it compares.

mod e2e;

use std::time::{Duration, Instant};

use e2e::clients;
use e2e::harness::{Nested, OVERLAY_LEVEL};
use e2e::keyboard::{KEY_ESC, KEY_LEFTALT, KEY_LEFTMETA, KEY_N, KEY_TAB, Keyboard};

use hypr_swap::config::Order;
use hypr_swap::ordering;
use hypr_swap::state::World;

/// Long enough for the overlay to map and take keyboard focus, and therefore long enough for one
/// to have appeared if the shortcut wrongly opened one (see `tests/e2e_switcher.rs`).
const SETTLE: Duration = Duration::from_millis(200);

/// The monitor the generated harness configuration always creates.
const PRIMARY: &str = "WAYLAND-1";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Press the documented new-workspace combination, `SUPER, N`.
///
/// The modifier is released afterwards like any human press. The application must ignore the
/// resulting `released` event entirely (`contracts/shortcuts.md`), which is part of what the
/// no-op scenario proves.
fn fire_new_workspace(keyboard: &mut Keyboard) {
    keyboard.hold(KEY_LEFTMETA);
    keyboard.tap_while_held(KEY_N);
    keyboard.release(KEY_LEFTMETA);
    keyboard.settle();
}

/// Move a workspace onto a monitor and leave it showing there, focused.
fn show(nested: &Nested, monitor: &str, workspace: i32) {
    nested.dispatch(&format!("focusmonitor {monitor}"));
    nested.dispatch(&format!("focusworkspaceoncurrentmonitor {workspace}"));
    nested.wait_until(&format!("{monitor} shows workspace {workspace}"), || {
        nested.active_workspace_on(monitor) == Some(workspace)
    });
}

/// The ordinary workspace ids the compositor knows about, sorted.
///
/// This is the set FR-020 picks the lowest unused number from, and the set FR-021 forbids the
/// second press from growing.
fn workspace_ids(nested: &Nested) -> Vec<i32> {
    let mut ids: Vec<i32> = nested
        .workspaces()
        .into_iter()
        .filter(|workspace| !workspace.is_special())
        .map(|workspace| workspace.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// The entries the overlay is built from, derived from the compositor's live state by the same
/// `ordering` function the overlay itself uses (the idiom from `tests/e2e_presentation.rs`).
fn entries(nested: &Nested) -> Vec<ordering::Entry> {
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    ordering::entries(&world, Order::Mru).0
}

/// Assert no `hypr-swap` layer surface is mapped at any point during `window`.
///
/// The new-workspace shortcut never opens the overlay, so a single reading after the fact would
/// miss one that mapped and closed again. Polling is the only external way to say "never".
fn assert_no_overlay_during(nested: &Nested, window: Duration) {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        assert!(
            nested.overlay_monitors().is_empty(),
            "the new-workspace shortcut must never map an overlay"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[test]
fn e2e_new_workspace_lowest_unused() {
    // FR-020, US4-AS1, US4-AS3. Workspaces 1, 2 and 4 are in use across two monitors and the
    // second monitor is focused, so the shortcut must land on 3 — the lowest number not in use,
    // bound to the monitor the user is actually on rather than to wherever it would otherwise go.
    let nested = Nested::start();
    let other = nested.add_headless_output();

    let _one = clients::spawn_on(&nested, 1, "on-1");
    show(&nested, &other, 2);
    let _two = clients::spawn(&nested, "on-2");
    show(&nested, PRIMARY, 4);
    let _four = clients::spawn(&nested, "on-4");
    // Leave the user on the second monitor, looking at a workspace that has a window in it — the
    // precondition that separates FR-020 from FR-021.
    show(&nested, &other, 2);

    assert_eq!(
        workspace_ids(&nested),
        vec![1, 2, 4],
        "the scenario's setup"
    );
    assert_eq!(nested.active_workspace_on(PRIMARY), Some(4));
    assert_eq!(nested.active_workspace(), 2, "the focused monitor's view");

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    fire_new_workspace(&mut keyboard);

    nested.wait_until("workspace 3 becomes active on the focused monitor", || {
        nested.active_workspace_on(&other) == Some(3)
    });

    // FR-020: created, bound to the focused monitor, active *and* focused there.
    assert_eq!(
        nested.monitor_of(3).as_deref(),
        Some(other.as_str()),
        "the new workspace is bound to the monitor the shortcut was fired from"
    );
    assert_eq!(
        nested.active_workspace(),
        3,
        "and it is what the focused monitor is showing"
    );
    assert_eq!(
        nested.active_workspace_on(PRIMARY),
        Some(4),
        "the other monitor is untouched"
    );
    // The shortcut is a press, not a session: no overlay, and the `released` event changes nothing.
    assert_no_overlay_during(&nested, SETTLE);

    // US4-AS3: the new workspace is an ordinary entry the next time the overlay opens.
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());
    let surface = nested
        .overlay_surfaces()
        .into_iter()
        .next()
        .expect("the overlay surface");
    assert_eq!(surface.level, OVERLAY_LEVEL);

    let listed = entries(&nested);
    assert!(
        listed.iter().any(|entry| entry.workspace_id == 3),
        "the workspace the shortcut created is listed: {:?}",
        listed
            .iter()
            .map(|entry| entry.workspace_id)
            .collect::<Vec<_>>()
    );

    // Cancel rather than commit, so the scenario ends where it asserted it was.
    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay closes", || {
        nested.overlay_monitors().is_empty()
    });
    assert_eq!(nested.active_workspace(), 3, "cancelling changed nothing");
}

#[test]
fn e2e_new_workspace_noop_when_empty() {
    // FR-021, US4-AS2, SC-007. The second press is the one under test: the workspace on screen is
    // the empty one the first press created, so repeat presses cannot accumulate workspaces.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    assert_eq!(workspace_ids(&nested), vec![1, 2], "the scenario's setup");

    let daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // First press: workspace 1 has a window, so the lowest unused number is switched to.
    fire_new_workspace(&mut keyboard);
    nested.wait_until("workspace 3 becomes active", || {
        nested.active_workspace() == 3
    });

    let before = nested.layout();
    let before_ids = workspace_ids(&nested);
    assert_eq!(
        before_ids,
        vec![1, 2, 3],
        "the first press created exactly one workspace"
    );

    // Second press, on the still-empty workspace 3. A wrong implementation would take 4 here,
    // which would also destroy the empty 3 on the way out — visible in both assertions below.
    fire_new_workspace(&mut keyboard);
    std::thread::sleep(SETTLE);
    assert_no_overlay_during(&nested, SETTLE);

    assert_eq!(
        nested.layout(),
        before,
        "nothing switched and focus is unchanged"
    );
    assert_eq!(
        workspace_ids(&nested),
        before_ids,
        "and nothing was created (SC-007)"
    );

    // FR-021 is a no-op, not a handled failure: the user is told nothing because nothing went
    // wrong (`contracts/diagnostics.md` lists no condition for it).
    let stderr = daemon.stderr();
    assert!(
        !stderr.contains("ERROR") && !stderr.contains("WARN"),
        "the no-op raises no diagnostic:\n{stderr}"
    );
    assert!(
        !stderr.contains("workspace"),
        "and says nothing about workspaces at all:\n{stderr}"
    );
}
