//! How the overlay presents itself: what an entry says, how the overlay is sized and stacked.
//!
//! The overlay's pixels are deliberately not asserted on — research.md R14 rejects screenshot
//! comparison as brittle across fonts and scaling. What these tests assert instead is everything
//! about the presentation that *is* externally observable: the surface's stacking level and
//! geometry as the compositor reports them, checked against the same `ui::layout` arithmetic the
//! application used to ask for it, and the entry content derived from the live compositor state
//! by the same `ordering` function the overlay is built from.

mod e2e;

use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, OVERLAY_LEVEL, OverlaySurface};
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};

use hypr_swap::config::Order;
use hypr_swap::ordering;
use hypr_swap::state::World;
use hypr_swap::ui::layout;

const SETTLE: Duration = Duration::from_millis(200);

/// The entries the overlay is built from, derived from the compositor's live state exactly as the
/// application derives them.
fn entries(nested: &Nested, order: Order) -> Vec<ordering::Entry> {
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    ordering::entries(&world, order).0
}

/// Open the overlay and leave the modifier held, so the surface can be inspected.
fn open_overlay(nested: &Nested, keyboard: &mut Keyboard) -> OverlaySurface {
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());
    nested
        .overlay_surfaces()
        .into_iter()
        .next()
        .expect("the overlay surface")
}

#[test]
fn e2e_list_shows_window_names() {
    // FR-014, US3-AS6: a row is the workspace name followed by the titles of its windows.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "alpha-window");
    let _two = clients::spawn_on(&nested, 2, "beta-window");
    let _also_two = clients::spawn(&nested, "gamma-window");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = open_overlay(&nested, &mut keyboard);
    assert_eq!(surface.level, OVERLAY_LEVEL);

    let entries = entries(&nested, Order::Mru);
    let two = entries
        .iter()
        .find(|entry| entry.workspace_id == 2)
        .expect("workspace 2 is listed");
    assert_eq!(two.label, "2", "the row leads with the workspace name");
    assert_eq!(
        two.windows
            .iter()
            .map(|window| window.label.as_str())
            .collect::<Vec<_>>(),
        vec!["beta-window", "gamma-window"],
        "then every window on it, in the compositor's order"
    );

    let one = entries
        .iter()
        .find(|entry| entry.workspace_id == 1)
        .expect("workspace 1 is listed");
    assert_eq!(
        one.windows
            .iter()
            .map(|window| window.label.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha-window"],
        "each row shows only its own workspace's windows"
    );

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
}

#[test]
fn e2e_scrolls_many_workspaces() {
    // FR-019, SC-005: twenty workspaces do not shrink the entries. The overlay stays inside its
    // cap and scrolls instead, and an entry past the bottom of the viewport is still selectable.
    //
    // The overlay is opened on a deliberately small output. Twenty rows fit comfortably inside
    // 80 % of a full-size monitor, so a scenario run there would assert the "never shrinks" half
    // of FR-019 without ever exercising the scrolling half.
    let nested = Nested::start();
    let mut windows = Vec::new();
    for id in 1..=20 {
        windows.push(clients::spawn_on(&nested, id, &format!("window-{id}")));
    }
    nested.wait_until("twenty workspaces exist", || {
        nested
            .workspaces()
            .iter()
            .filter(|workspace| !workspace.is_special())
            .count()
            >= 20
    });

    let small = nested.add_headless_output();
    nested.hyprctl(&["keyword", "monitor", &format!("{small},640x480@60,auto,1")]);
    nested.dispatch(&format!("focusmonitor {small}"));
    nested.wait_until("the small output is focused at 640x480", || {
        nested
            .monitors()
            .iter()
            .any(|monitor| monitor.name == small && monitor.focused && monitor.size == (640, 480))
    });

    let monitor = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .expect("a focused monitor");

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let listed = entries(&nested, Order::Mru);
    let expected = layout::list_metrics(monitor.size, monitor.scale, listed.len());
    assert!(
        expected.scrolls(listed.len()),
        "{} entries must exceed the cap on a {:?} monitor",
        listed.len(),
        monitor.size
    );

    let surface = open_overlay(&nested, &mut keyboard);
    assert_eq!(
        surface.size,
        (expected.width, expected.height),
        "the mapped surface is the size ui::layout asked for"
    );

    // The properties FR-019 requires, asserted against the real surface rather than against the
    // computation that produced it.
    assert!(
        surface.size.0 <= monitor.size.0 * 4 / 5 && surface.size.1 <= monitor.size.1 * 4 / 5,
        "the overlay stays inside 80 % of {:?}, got {:?}",
        monitor.size,
        surface.size
    );
    let rows = (surface.size.1 - expected.padding * 2) / expected.row_height;
    assert_eq!(
        u32::try_from(expected.visible_rows).unwrap(),
        rows,
        "the surface holds whole rows of the fixed entry height, not shrunken ones"
    );
    assert_eq!(
        expected.row_height,
        layout::list_metrics(monitor.size, monitor.scale, 1).row_height,
        "twenty entries are the same height as one"
    );

    // Walk past the bottom of the viewport. With an empty history the entries are in compositor
    // order and the highlight opens on index 1, so the target index is known exactly.
    let taps = expected.visible_rows + 2;
    let target = listed[1 + taps].workspace_id;
    assert!(
        1 + taps >= expected.visible_rows,
        "the target must lie beyond the first viewport"
    );
    for _ in 0..taps {
        keyboard.tap_while_held(KEY_TAB);
        std::thread::sleep(Duration::from_millis(30));
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("an entry beyond the viewport is still selectable", || {
        nested.active_workspace() == target
    });
}

#[test]
fn e2e_above_fullscreen() {
    // FR-018: the overlay layer is what puts it above a fullscreen client.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "fullscreen-window");
    let _two = clients::spawn_on(&nested, 2, "other-window");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    nested.dispatch("fullscreen 0");
    nested.wait_until("the window is fullscreen", || {
        nested.clients().iter().any(|window| {
            window.title == "fullscreen-window" && window.size == nested_size(&nested)
        })
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = open_overlay(&nested, &mut keyboard);

    assert_eq!(
        surface.level, OVERLAY_LEVEL,
        "the surface is on the overlay layer, above a fullscreen client"
    );

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    assert_eq!(
        nested.active_workspace(),
        1,
        "cancelling over a fullscreen client changes nothing"
    );
}

/// The focused monitor's pixel size — what a fullscreen window fills.
fn nested_size(nested: &Nested) -> (u32, u32) {
    nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .map_or((0, 0), |monitor| monitor.size)
}
