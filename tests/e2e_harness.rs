//! The R14 spike, kept as a test.
//!
//! Everything the rest of the E2E suite rests on is asserted here, so a compositor upgrade that
//! breaks the harness fails with an obvious message instead of making every scenario mysterious:
//! a nested instance starts with its own IPC socket, headless outputs can be created, `foot`
//! windows appear with known titles, injected key events really do drive compositor binds, and
//! the application registers its named shortcuts.

mod e2e;

use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, Setup};
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};
use hypr_swap::ui::shortcuts::Shortcut;

#[test]
fn nested_instance_starts_with_its_own_ipc_socket() {
    let host = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    let nested = Nested::start();

    assert_ne!(
        nested.signature, host,
        "the nested instance must not address the host session"
    );
    assert!(
        nested.ipc.socket_path().exists(),
        "{}",
        nested.ipc.socket_path().display()
    );
    assert!(
        !nested.monitors().is_empty(),
        "the nested instance reports at least one monitor"
    );
    assert_eq!(
        nested
            .monitors()
            .iter()
            .filter(|monitor| monitor.focused)
            .count(),
        1,
        "exactly one monitor is focused whenever a connection is established"
    );
}

#[test]
fn headless_outputs_stand_in_for_physical_monitors() {
    let nested = Nested::start();
    let before = nested.monitors().len();

    let name = nested.add_headless_output();

    assert!(
        name.starts_with("HEADLESS"),
        "unexpected connector name {name}"
    );
    assert_eq!(nested.monitors().len(), before + 1);
    let created = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.name == name)
        .expect("the new output is reported");
    assert_eq!(created.size, (1920, 1080));
    assert!(
        created.active_workspace != 0,
        "every monitor has exactly one active workspace at all times"
    );
}

#[test]
fn foot_windows_appear_with_known_titles_and_geometry() {
    let nested = Nested::start();

    let client = clients::spawn(&nested, "harness-probe");

    assert!(client.is_open(&nested));
    assert_eq!(client.workspace(&nested), Some(nested.active_workspace()));
    let window = nested
        .clients()
        .into_iter()
        .find(|window| window.address == client.address)
        .expect("the window is reported");
    assert!(window.mapped);
    assert!(
        window.has_area(),
        "the compositor reports real geometry: {:?}",
        window.size
    );
    assert_eq!(
        clients::titles_on(&nested, nested.active_workspace()),
        vec!["harness-probe"]
    );
}

#[test]
fn injected_key_events_drive_real_compositor_binds() {
    // The single most load-bearing fact in the suite: if this fails, every scenario that presses
    // a key is meaningless. Hyprland ignores injected keys unless the virtual keyboard also
    // announces its modifier state, which is what `Keyboard` does around every key.
    let nested = Nested::start_with(
        &Setup::documented().with_compositor_config("bind = ALT, F12, workspace, 7\n"),
    );
    assert_ne!(
        nested.active_workspace(),
        7,
        "the scenario starts somewhere else"
    );

    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    keyboard.press(KEY_LEFTALT);
    keyboard.settle();
    keyboard.tap(88); // F12
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();

    nested.wait_until("the injected ALT+F12 activates workspace 7", || {
        nested.active_workspace() == 7
    });
}

#[test]
fn the_application_registers_both_named_shortcuts() {
    let nested = Nested::start();
    assert!(
        nested.registered_shortcuts().is_empty(),
        "nothing is registered before it runs"
    );

    let daemon = nested.start_daemon();

    assert_eq!(
        nested.registered_shortcuts(),
        vec![
            "hypr-swap:new-workspace".to_owned(),
            "hypr-swap:switcher".to_owned()
        ],
        "both names appear to the compositor exactly as contracts/shortcuts.md documents"
    );
    assert_eq!(daemon.terminate(), Some(0), "SIGTERM is a clean shutdown");
}

#[test]
fn the_generated_configuration_carries_the_documented_bind_lines() {
    // FR-022b: the suite proves the documentation rather than a private arrangement.
    let nested = Nested::start();
    let binds = nested.hyprctl(&["binds"]);
    for shortcut in Shortcut::ALL {
        assert!(
            binds.contains(&shortcut.qualified_name()),
            "{} is not bound in the nested compositor:\n{binds}",
            shortcut.qualified_name()
        );
    }
}

#[test]
fn a_held_modifier_with_taps_is_delivered_as_one_gesture() {
    // The shape every switcher scenario uses. Asserted through a compositor bind so it stands on
    // its own, without the application running.
    let nested = Nested::start_with(
        &Setup::documented().with_compositor_config("bind = ALT, TAB, workspace, 9\n"),
    );

    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 2, Duration::from_millis(80));

    nested.wait_until("the held-modifier gesture reaches the bind", || {
        nested.active_workspace() == 9
    });
}
