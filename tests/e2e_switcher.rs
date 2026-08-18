//! User Story 1 — switching workspaces with a hold-and-release hotkey.
//!
//! Every test here drives the real external interface: a nested Hyprland running the documented
//! bind lines, real key events through `virtual-keyboard-unstable-v1`, and assertions read back
//! from that compositor's own IPC (research.md R14).
//!
//! Two conventions make the assertions legible. First, the highlight cannot be read from outside
//! — the overlay is pixels — so what a test asserts is *which workspace a release activates*,
//! which is the user-visible consequence and the only honest external observation. Second, every
//! scenario that depends on entry order calls [`assert_compositor_order`] first, so a compositor
//! that reports its workspaces differently fails with that fact rather than with a mysterious
//! off-by-one somewhere else.

mod e2e;

use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, Setup};
use e2e::keyboard::{KEY_DOWN, KEY_ESC, KEY_LEFT, KEY_LEFTALT, KEY_LEFTSHIFT, KEY_TAB, Keyboard};

/// Long enough for the overlay to map and take keyboard focus between taps. The R4 spike measured
/// `pressed` → first frame at 4 ms, so this is generous rather than tuned.
const SETTLE: Duration = Duration::from_millis(200);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert the compositor lists its ordinary workspaces in the order the scenario assumes.
///
/// Entry order is `ordering.rs`'s business and is unit-tested there; what this pins down is the
/// *input* those tests take for granted, so an unexpected compositor order is reported as itself.
fn assert_compositor_order(nested: &Nested, expected: &[i32]) {
    let ids: Vec<i32> = nested
        .workspaces()
        .into_iter()
        .filter(|workspace| !workspace.is_special())
        .map(|workspace| workspace.id)
        .collect();
    assert_eq!(ids, expected, "the compositor's reported workspace order");
}

/// Set up `count` workspaces numbered from 1, each holding one `foot` window, and leave the
/// compositor on workspace 1.
fn workspaces_with_windows(nested: &Nested, count: i32) -> Vec<clients::Client> {
    let clients: Vec<clients::Client> = (1..=count)
        .map(|id| clients::spawn_on(nested, id, &format!("window-{id}")))
        .collect();
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    clients
}

/// Switch workspace the way the user's own compositor binds do — which is what the application
/// must observe in order to keep its MRU history (FR-008c).
fn switch_externally(nested: &Nested, workspace: i32) {
    nested.dispatch(&format!("workspace {workspace}"));
    nested.wait_until(&format!("workspace {workspace} becomes active"), || {
        nested.active_workspace() == workspace
    });
}

fn wait_for_overlay(nested: &Nested) {
    nested.wait_until("the overlay maps with exclusive keyboard focus", || {
        !nested.overlay_monitors().is_empty()
    });
}

fn wait_for_no_overlay(nested: &Nested) {
    nested.wait_until("the overlay closes", || {
        nested.overlay_monitors().is_empty()
    });
}

/// The address of the window holding keyboard focus, or `None` when nothing does.
fn focused_window(nested: &Nested) -> Option<String> {
    let json = nested.hyprctl(&["-j", "activewindow"]);
    let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
    parsed
        .get("address")
        .and_then(serde_json::Value::as_str)
        .filter(|address| !address.is_empty())
        .map(ToOwned::to_owned)
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[test]
fn e2e_activate_same_monitor() {
    // FR-001, FR-002, FR-005, FR-009, US1-AS4.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Hold ALT, tap TAB once, release ALT. With an empty activation history the MRU order is the
    // compositor's, so the highlight opens on the second entry — workspace 2 (FR-008b).
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 1, SETTLE);

    nested.wait_until("releasing ALT activates the highlighted workspace", || {
        nested.active_workspace() == 2
    });
    wait_for_no_overlay(&nested);
}

#[test]
fn e2e_mru_order_and_highlight() {
    // FR-008a, FR-008b, FR-008d, US1-AS1, US1-AS2.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 4);
    assert_compositor_order(&nested, &[1, 2, 3, 4]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Only activations the daemon observes enter the history, so this is what builds it.
    switch_externally(&nested, 4);
    switch_externally(&nested, 2);
    switch_externally(&nested, 1);
    // History is now [1, 2, 4]; workspace 3 has never been active this session.

    // One tap and release returns the user to where they were — the whole point of the MRU
    // default's highlight sitting on the second entry (FR-008b, US1-AS2).
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until("the second MRU entry is the previous workspace", || {
        nested.active_workspace() == 2
    });

    // History is now [2, 1, 4], so the entries are 2, 1, 4, then never-active 3 last (FR-008d).
    // Three taps past the opening highlight lands on that last entry.
    keyboard.hold(KEY_LEFTALT);
    for _ in 0..3 {
        keyboard.tap_while_held(KEY_TAB);
        std::thread::sleep(SETTLE);
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until(
        "a never-active workspace sorts after every used one",
        || nested.active_workspace() == 3,
    );
}

#[test]
fn e2e_configured_order() {
    // FR-008a, FR-008b, US1-AS3, US5-AS7.
    let nested =
        Nested::start_with(&Setup::documented().with_app_config("order = \"compositor\"\n"));
    let _windows = workspaces_with_windows(&nested, 4);
    assert_compositor_order(&nested, &[1, 2, 3, 4]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    switch_externally(&nested, 4);
    switch_externally(&nested, 2);
    switch_externally(&nested, 1);
    // History is [1, 2, 4], so MRU would order the entries 1, 2, 4, 3 and open on 2.

    // Compositor order lists 1, 2, 3, 4 and opens the highlight on the active workspace, which is
    // 1 — so two taps commit workspace 2. Under MRU the same gesture would commit workspace 4,
    // which is what makes this assertion prove the setting took effect.
    keyboard.hold(KEY_LEFTALT);
    for _ in 0..2 {
        keyboard.tap_while_held(KEY_TAB);
        std::thread::sleep(SETTLE);
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until("compositor order opens on the active workspace", || {
        nested.active_workspace() == 2
    });
}

#[test]
fn e2e_external_switch_tracked() {
    // FR-008c, US1-AS9: a switch the user makes with their own bind counts as an activation.
    let nested = Nested::start_with(
        &Setup::documented().with_compositor_config("bind = ALT, F12, workspace, 2\n"),
    );
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // A compositor bind this application knows nothing about.
    keyboard.press(KEY_LEFTALT);
    keyboard.settle();
    keyboard.tap(88); // F12
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the user's own bind switches to workspace 2", || {
        nested.active_workspace() == 2
    });

    // History is [2], so the entries are 2, then never-active 1 and 3 in compositor order, and
    // the highlight opens on 1. Had the external switch gone unnoticed the history would still be
    // empty, the order would be 1, 2, 3, and this gesture would commit workspace 2 — a no-op.
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 1, SETTLE);

    nested.wait_until(
        "the externally-activated workspace leads the history",
        || nested.active_workspace() == 1,
    );
}

#[test]
fn e2e_cancel_leaves_state() {
    // FR-006, US1-AS5, US1-AS6.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);

    keyboard.tap_while_held(KEY_ESC);
    keyboard.settle();
    wait_for_no_overlay(&nested);

    // Releasing the modifier after cancelling must not resurrect the commit (US1-AS5).
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    assert_eq!(
        nested.active_workspace(),
        1,
        "Escape left the workspace alone"
    );

    // And the cancelled session left no trace in the history: the next gesture behaves exactly as
    // the first one would have, committing the second entry of an untouched MRU order.
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 1, SETTLE);
    nested.wait_until("a cancelled session did not disturb the ordering", || {
        nested.active_workspace() == 2
    });
}

#[test]
fn e2e_navigation_wraps_and_reverses() {
    // FR-003, FR-004, FR-004a, US1-AS8.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Entries 1, 2, 3 with the highlight opening on 2.
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);

    // Forward past the last entry, which wraps to the first (FR-004).
    keyboard.tap_while_held(KEY_TAB); // → 3
    std::thread::sleep(SETTLE);
    keyboard.tap_while_held(KEY_TAB); // → 1, wrapped
    std::thread::sleep(SETTLE);

    // Shift+Tab does not match the `ALT, TAB` bind, so the compositor forwards it to the overlay
    // as an ordinary key — which is how backwards navigation reaches us at all (research.md R5).
    keyboard.press(KEY_LEFTSHIFT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.release(KEY_LEFTSHIFT);
    keyboard.settle();
    std::thread::sleep(SETTLE);

    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until(
        "stepping back from the first entry wraps to the last",
        || nested.active_workspace() == 3,
    );
}

#[test]
fn e2e_select_active_is_noop() {
    // FR-011, US1-AS7.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);
    let before = nested.workspaces().len();

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // The highlight opens on entry 1; Left steps back onto entry 0, the active workspace.
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);
    keyboard.tap_while_held(KEY_LEFT);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    wait_for_no_overlay(&nested);
    std::thread::sleep(SETTLE);
    assert_eq!(
        nested.active_workspace(),
        1,
        "selecting the workspace already on screen changed nothing"
    );
    assert_eq!(
        nested.workspaces().len(),
        before,
        "and created nothing either"
    );
}

#[test]
fn e2e_repeat_trigger_advances() {
    // FR-003, FR-028: the second Alt-Tab of an Alt-Tab-Tab gesture, which the compositor consumes
    // before any client sees it, must move the highlight rather than open a second overlay.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 4);
    assert_compositor_order(&nested, &[1, 2, 3, 4]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    keyboard.hold(KEY_LEFTALT);
    for _ in 0..3 {
        keyboard.tap_while_held(KEY_TAB);
        keyboard.settle();
        std::thread::sleep(SETTLE);
    }
    wait_for_overlay(&nested);
    assert_eq!(
        nested.overlay_monitors().len(),
        1,
        "three triggers, one overlay"
    );

    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    // Opened on entry 1 (workspace 2); two further triggers advanced it to entry 3.
    nested.wait_until("each repeat trigger advanced the highlight by one", || {
        nested.active_workspace() == 4
    });
}

#[test]
fn e2e_fast_tap_commits() {
    // FR-005, SC-001: pressed and released faster than the overlay can map. The gesture still has
    // to work — this is "tap Alt-Tab to bounce back to the last workspace".
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    switch_externally(&nested, 2);
    switch_externally(&nested, 1);
    // History [1, 2]: the entry to bounce back to is workspace 2.

    keyboard.press(KEY_LEFTALT);
    keyboard.settle();
    keyboard.tap_fast(KEY_TAB);
    keyboard.release(KEY_LEFTALT);
    keyboard.flush();

    nested.wait_until("a fast tap commits the initial highlight", || {
        nested.active_workspace() == 2
    });
    assert!(
        nested.overlay_monitors().is_empty(),
        "the overlay never appears on the fast-tap path"
    );
}

#[test]
fn e2e_vanished_target_cancels() {
    // FR-027: the entries are a snapshot, so a workspace really can be destroyed underneath them.
    let nested = Nested::start();
    let mut windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);
    // The highlight is on workspace 2, the second entry.

    // Closing its only window destroys the workspace while the overlay still lists it.
    let doomed = windows.remove(1);
    drop(doomed);
    nested.wait_until("workspace 2 is destroyed while the overlay is open", || {
        !nested.workspaces().iter().any(|w| w.id == 2)
    });

    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    wait_for_no_overlay(&nested);
    std::thread::sleep(SETTLE);

    assert_eq!(
        nested.active_workspace(),
        1,
        "a commit whose target has gone activates nothing"
    );
    let stderr = daemon.stderr();
    assert!(
        stderr.contains("workspace 2 no longer exists"),
        "the dropped selection is reported at INFO:\n{stderr}"
    );
}

#[test]
fn e2e_special_workspaces_excluded() {
    // FR-007: a scratchpad is never an entry, so navigation cannot land on one.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);

    // A special workspace with a window in it, then hidden again.
    nested.dispatch("togglespecialworkspace scratch");
    let _scratch = clients::spawn(&nested, "scratchpad-window");
    nested.dispatch("togglespecialworkspace scratch");
    nested.wait_until("the scratchpad exists and is hidden", || {
        nested
            .workspaces()
            .iter()
            .any(hypr_swap::model::Workspace::is_special)
            && nested.active_workspace() == 1
    });
    assert_compositor_order(&nested, &[1, 2, 3]);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Three ordinary entries, opening on entry 1. Four taps walk 1 → 2 → 0 → 1, landing back on
    // workspace 2. A fourth entry for the scratchpad would change that answer.
    keyboard.hold(KEY_LEFTALT);
    for _ in 0..4 {
        keyboard.tap_while_held(KEY_TAB);
        keyboard.settle();
        std::thread::sleep(SETTLE);
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until("the cycle covers exactly the ordinary workspaces", || {
        nested.active_workspace() == 2
    });
    assert!(
        nested.active_workspace() > 0,
        "a special workspace was never activated"
    );
}

#[test]
fn e2e_focus_returns_on_close() {
    // FR-002a: exclusive keyboard focus is borrowed, not taken. Closing the overlay gives it back
    // to whatever held it, which the compositor does when the layer surface is destroyed.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 2);
    let holder = focused_window(&nested).expect("the window on workspace 1 has focus");

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);

    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    wait_for_no_overlay(&nested);

    nested.wait_until("keyboard focus returns to the client that held it", || {
        focused_window(&nested).as_deref() == Some(holder.as_str())
    });
}

#[test]
fn e2e_monitor_removed_degrades() {
    // FR-027 and the monitor-disconnected edge case: losing the target's *monitor* is not losing
    // the target. The selection degrades to plain activation rather than cancelling.
    let nested = Nested::start();
    let _windows = workspaces_with_windows(&nested, 3);
    assert_compositor_order(&nested, &[1, 2, 3]);

    let headless = nested.add_headless_output();
    nested.dispatch(&format!("moveworkspacetomonitor 3 {headless}"));
    nested.dispatch("focusmonitor WAYLAND-1");
    nested.dispatch("workspace 1");
    nested.wait_until("workspace 3 lives on the headless output", || {
        nested.monitor_of(3).as_deref() == Some(headless.as_str()) && nested.active_workspace() == 1
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Entries 1, 2, 3 opening on entry 1; one more tap highlights workspace 3.
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    wait_for_overlay(&nested);
    keyboard.tap_while_held(KEY_DOWN);
    keyboard.settle();
    std::thread::sleep(SETTLE);

    // The monitor the highlighted entry was listed under goes away mid-session.
    nested.hyprctl(&["output", "remove", &headless]);
    nested.wait_until("the headless output is gone", || {
        !nested.monitors().iter().any(|m| m.name == headless)
    });

    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until(
        "the selection still activates, on the focused monitor",
        || nested.active_workspace() == 3,
    );
}
