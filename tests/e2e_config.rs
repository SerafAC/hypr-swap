//! User Story 5 — binding the shortcuts and configuring how the overlay presents itself, plus
//! the start-up and reconnection contract the daemon lives under.
//!
//! Every setting here is exercised the way a user changes it: a file at the location the daemon
//! reads, a bind line in the compositor's own configuration, and then the real gesture. What the
//! tests assert on is what the compositor reports — which monitors carry a layer surface, how big
//! each one is, which workspace ended up active — plus the daemon's stderr, which
//! `contracts/diagnostics.md` makes its complete diagnostic record.
//!
//! Notifications are observed through a recording `notify-send` stub on the daemon's `PATH`
//! (`e2e::notify`), because spawning that binary is exactly what raising a notification *is*.

mod e2e;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use e2e::clients;
use e2e::harness::{Nested, OVERLAY_LEVEL, Setup};
use e2e::keyboard::{
    KEY_ENTER, KEY_ESC, KEY_F12, KEY_LEFTALT, KEY_LEFTMETA, KEY_N, KEY_TAB, Keyboard,
};
use e2e::notify::NotifyLog;

use hypr_swap::config::Order;
use hypr_swap::ordering;
use hypr_swap::state::World;
use hypr_swap::ui::layout;
use hypr_swap::ui::shortcuts::Shortcut;
use hypr_swap::{APP_ID, VERSION};

/// Long enough for the overlay to map and take keyboard focus between taps.
const SETTLE: Duration = Duration::from_millis(200);

/// The monitor the generated harness configuration always creates.
const PRIMARY: &str = "WAYLAND-1";

/// Long enough that a daemon retrying in a hot loop would be unmistakable.
const IDLE_WINDOW: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The entries the overlay is built from, derived from the compositor's live state exactly as the
/// application derives them.
fn entries(nested: &Nested, order: Order) -> Vec<ordering::Entry> {
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    ordering::entries(&world, order).0
}

/// The workspace a bare hold-and-release would commit, under `order`.
fn initial_highlight(nested: &Nested, order: Order) -> i32 {
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    let (entries, highlight) = ordering::entries(&world, order);
    entries[highlight].workspace_id
}

/// Open the overlay and leave the modifier held, so the surfaces can be inspected.
fn open_overlay(nested: &Nested, keyboard: &mut Keyboard) {
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());
}

/// Cancel an open overlay and let go, leaving the compositor exactly as it was (FR-006).
fn cancel(nested: &Nested, keyboard: &mut Keyboard) {
    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay closes again", || {
        nested.overlay_monitors().is_empty()
    });
}

/// Hold the switcher combination, tap `times`, and release — the whole gesture.
fn gesture(nested: &Nested, keyboard: &mut Keyboard, times: usize) {
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, times, SETTLE);
    nested.wait_until("the overlay closes again", || {
        nested.overlay_monitors().is_empty()
    });
}

/// Assert the compositor lists its ordinary workspaces in the order the scenario assumes.
fn assert_compositor_order(nested: &Nested, expected: &[i32]) {
    let ids: Vec<i32> = nested
        .workspaces()
        .into_iter()
        .filter(|workspace| !workspace.is_special())
        .map(|workspace| workspace.id)
        .collect();
    assert_eq!(ids, expected, "the compositor's reported workspace order");
}

/// The size `ui::layout` derives for the list presentation on a named monitor.
fn expected_list_size(nested: &Nested, monitor: &str, entry_count: usize) -> (u32, u32) {
    let monitor = nested
        .monitors()
        .into_iter()
        .find(|candidate| candidate.name == monitor)
        .expect("the monitor is connected");
    layout::list_metrics(monitor.size, monitor.scale, entry_count).surface_size()
}

/// Give the compositor a moment to do something, for the assertions that nothing happened.
fn quiesce() {
    std::thread::sleep(Duration::from_millis(500));
}

/// Switch to a workspace and wait until it is showing — one activation for the daemon to observe.
fn activate(nested: &Nested, workspace: i32) {
    nested.dispatch(&format!("workspace {workspace}"));
    nested.wait_until(&format!("workspace {workspace} is active"), || {
        nested.active_workspace() == workspace
    });
}

// ---------------------------------------------------------------------------
// Placement and defaults
// ---------------------------------------------------------------------------

#[test]
fn e2e_placement_all_monitors() {
    // FR-017, US5-AS2/AS3: `placement = "all"` puts a copy on every connected monitor, and all of
    // them show the same highlight because one session drives them all.
    //
    // "The same highlight" is not a claim a test can read off the pixels (research.md R14 rejects
    // screenshot comparison), so it is asserted in the form that matters: two surfaces, one
    // session, and therefore exactly one commit when the modifier is released — the same one the
    // single-monitor configuration would have produced.
    let nested = Nested::start_with(&Setup::documented().with_app_config("placement = \"all\"\n"));
    let other = nested.add_headless_output();

    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch(&format!("focusmonitor {PRIMARY}"));
    nested.dispatch("focusworkspaceoncurrentmonitor 1");
    nested.wait_until("the scenario starts on workspace 1 of the primary", || {
        nested.active_workspace_on(PRIMARY) == Some(1)
            && nested
                .monitors()
                .iter()
                .any(|m| m.name == PRIMARY && m.focused)
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let selected = initial_highlight(&nested, Order::Mru);
    let entry_count = entries(&nested, Order::Mru).len();
    open_overlay(&nested, &mut keyboard);

    // US5-AS2: displayed on every connected monitor simultaneously.
    let mut expected = vec![PRIMARY.to_owned(), other.clone()];
    expected.sort();
    assert_eq!(
        nested.overlay_monitors(),
        expected,
        "one copy per connected monitor"
    );

    // Each copy is sized for the monitor it is on, not for the one the session started from.
    for surface in nested.overlay_surfaces() {
        assert_eq!(
            surface.level, OVERLAY_LEVEL,
            "every copy sits on the overlay layer (FR-018)"
        );
        assert_eq!(
            surface.size,
            expected_list_size(&nested, &surface.monitor, entry_count),
            "the copy on {} is sized for that monitor",
            surface.monitor
        );
    }

    // US5-AS3: one highlight, so releasing commits once — the copies are there to be looked at.
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("every copy of the overlay closes", || {
        nested.overlay_monitors().is_empty()
    });
    nested.wait_until(
        &format!("workspace {selected} is showing on the primary monitor"),
        || nested.active_workspace_on(PRIMARY) == Some(selected),
    );
}

#[test]
fn e2e_defaults_without_config() {
    // FR-023, SC-006, US5-AS4: no configuration file at all is the normal case. The daemon runs
    // on the documented defaults — flat list, active monitor only, MRU order — and its shortcuts
    // work with nothing but the bind lines present.
    let setup = Setup::documented();
    assert!(
        setup.app_config.is_none(),
        "this scenario is defined by there being no configuration file"
    );
    let nested = Nested::start_with(&setup);
    let other = nested.add_headless_output();

    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch(&format!("focusmonitor {PRIMARY}"));
    nested.dispatch("focusworkspaceoncurrentmonitor 1");
    nested.wait_until("the scenario starts on workspace 1 of the primary", || {
        nested.active_workspace_on(PRIMARY) == Some(1)
            && nested
                .monitors()
                .iter()
                .any(|m| m.name == PRIMARY && m.focused)
    });

    let mut daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let selected = initial_highlight(&nested, Order::Mru);
    let entry_count = entries(&nested, Order::Mru).len();
    open_overlay(&nested, &mut keyboard);

    // `placement = "active"`: only the monitor holding the focused workspace (US5-AS3).
    assert_eq!(
        nested.overlay_monitors(),
        vec![PRIMARY.to_owned()],
        "the default placement shows nothing on {other}"
    );
    // `presentation = "list"`: the geometry is the list's, not the grid's.
    let surface = nested.overlay_surfaces().remove(0);
    assert_eq!(
        surface.size,
        expected_list_size(&nested, PRIMARY, entry_count),
        "the default presentation is the flat list"
    );
    assert_ne!(
        surface.size,
        layout::grid_metrics(
            nested
                .monitors()
                .into_iter()
                .find(|m| m.name == PRIMARY)
                .expect("the primary monitor")
                .size,
            1.0,
            entry_count
        )
        .surface_size(),
        "and is distinguishable from the grid it did not choose"
    );

    // `order = "mru"`: the highlight opens on the second entry, so one release goes back.
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until(
        &format!("the default order commits workspace {selected}"),
        || nested.active_workspace_on(PRIMARY) == Some(selected),
    );
    assert_eq!(selected, 2, "MRU opens on the workspace the user came from");

    assert!(
        daemon.is_running(),
        "no configuration file is not a problem"
    );
    let stderr = daemon.stderr();
    assert!(
        !stderr.contains("config"),
        "a missing file at the default location produces no diagnostic: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Invalid configuration
// ---------------------------------------------------------------------------

#[test]
fn e2e_invalid_config_falls_back() {
    // FR-024, FR-029, FR-030, US5-AS5, and the worked example in contracts/config.md: given
    // `presentation = "tiles"` and `order = "compositor"`, the daemon runs with the **list**
    // presentation (fallback, reported by name and notified) and **compositor** order (honoured).
    let notify = NotifyLog::new();
    let nested = Nested::start_with(
        &Setup::documented().with_app_config("presentation = \"tiles\"\norder = \"compositor\"\n"),
    );

    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    let _three = clients::spawn_on(&nested, 3, "on-3");
    assert_compositor_order(&nested, &[1, 2, 3]);
    assert_eq!(nested.active_workspace(), 3);

    let daemon = nested.start_daemon_with_env(&[], &[("PATH", &notify.path())]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let entry_count = entries(&nested, Order::Compositor).len();
    open_overlay(&nested, &mut keyboard);

    // The offending setting fell back: the overlay has the list's geometry, not the grid's.
    let surface = nested.overlay_surfaces().remove(0);
    assert_eq!(
        surface.size,
        expected_list_size(&nested, PRIMARY, entry_count),
        "presentation fell back to its own default"
    );
    cancel(&nested, &mut keyboard);

    // The user's other choice survived. In compositor order the highlight opens on the active
    // workspace (3), so two more taps walk 3 → 1 → 2; MRU would have opened on 2 and walked
    // 2 → 1 → 3, which is already active and would have changed nothing.
    gesture(&nested, &mut keyboard, 3);
    nested.wait_until("the compositor order was honoured", || {
        nested.active_workspace() == 2
    });

    // FR-029: the record names the setting. FR-030: and the user is told.
    let stderr = daemon.stderr();
    assert!(
        stderr.contains(r#"WARN  config.presentation: unknown value "tiles""#),
        "the offending setting is named on stderr: {stderr}"
    );
    assert!(
        stderr.contains(r#"using default "list""#),
        "and so is the default it fell back to: {stderr}"
    );
    assert!(
        !stderr.contains("config.order"),
        "the honoured setting is not reported: {stderr}"
    );
    let raised = notify.wait_for(1);
    assert_eq!(
        raised.len(),
        1,
        "exactly one notification, for the one bad value: {raised:?}"
    );
    assert!(
        raised[0].contains("hypr-swap: configuration problem"),
        "with the documented summary: {raised:?}"
    );
    assert!(
        raised[0].contains("config.presentation") && raised[0].contains("tiles"),
        "US5-AS5: the notification names the offending setting too: {raised:?}"
    );
}

// ---------------------------------------------------------------------------
// Binds
// ---------------------------------------------------------------------------

#[test]
fn e2e_unbound_shortcut_is_harmless() {
    // FR-022b, US5-AS6: with only the new-workspace bind present, the daemon runs normally, that
    // shortcut works, and the unbound switcher causes no error — its combination is simply not a
    // bind, so the keys go wherever they would have gone anyway.
    let nested = Nested::start_with(&Setup::documented().with_binds(&[Shortcut::NewWorkspace]));
    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch("workspace 2");
    nested.wait_until("the scenario starts on workspace 2", || {
        nested.active_workspace() == 2
    });

    let daemon = nested.start_daemon();
    // Both names are still registered: what is missing is the *bind*, which lives entirely in the
    // compositor's configuration and which the application is never told about.
    assert_eq!(
        nested.registered_shortcuts(),
        vec![
            "hypr-swap:new-workspace".to_owned(),
            "hypr-swap:switcher".to_owned()
        ]
    );

    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 2, SETTLE);
    quiesce();
    assert!(
        nested.overlay_monitors().is_empty(),
        "an unbound switcher opens no overlay"
    );
    assert_eq!(
        nested.active_workspace(),
        2,
        "and changes nothing while it is unbound"
    );

    // The other shortcut is unaffected (FR-020: the lowest unused number is 3).
    keyboard.press(KEY_LEFTMETA);
    keyboard.tap_while_held(KEY_N);
    keyboard.release(KEY_LEFTMETA);
    keyboard.settle();
    nested.wait_until("the bound shortcut still works", || {
        nested.active_workspace() == 3
    });
    assert!(
        nested.overlay_monitors().is_empty(),
        "the new-workspace shortcut never shows an overlay"
    );

    let stderr = daemon.stderr();
    assert!(
        !stderr.contains("ERROR"),
        "an unbound shortcut is not an error: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Start-up, reconnection and notifications
// ---------------------------------------------------------------------------

#[test]
fn e2e_no_compositor_at_start() {
    // FR-025: no compositor to reach at start-up is fatal, and says so in both places the user
    // might be looking. No nested instance is needed — the point is the absence of one.
    let notify = NotifyLog::new();
    let output = Command::new(env!("CARGO_BIN_EXE_hypr-swap"))
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env("PATH", notify.path())
        .stdin(Stdio::null())
        .output()
        .expect("the application under test is built");

    assert_eq!(
        output.status.code(),
        Some(3),
        "contracts/cli.md: exit 3 when the compositor cannot be reached at start-up"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "ERROR compositor: cannot connect at start-up: no HYPRLAND_INSTANCE_SIGNATURE in environment"
        ),
        "the record names what is missing: {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "standard output is unused (contracts/cli.md)"
    );

    // FR-030: a condition the user has to act on is also notified.
    let raised = notify.wait_for(1);
    assert_eq!(raised.len(), 1, "exactly one notification: {raised:?}");
    assert!(
        raised[0].contains("hypr-swap: cannot reach Hyprland"),
        "with the documented summary: {raised:?}"
    );
}

#[test]
fn e2e_reconnects_after_restart() {
    // FR-026a, FR-026b, FR-026c, FR-031, SC-009: the compositor is killed outright and a fresh
    // one started in its place. The daemon is not restarted and nothing is reconfigured; the
    // user's existing bind lines have to work again on their own, within ten seconds.
    let notify = NotifyLog::new();
    let mut nested = Nested::start();
    let mut daemon = nested.start_daemon_stable(&[("PATH", &notify.path())]);

    // Activations observed before the crash, most recent first: [1, 2].
    let _two = clients::spawn_on(&nested, 2, "before-2");
    let _one = clients::spawn_on(&nested, 1, "before-1");
    {
        // The switcher works before the crash — otherwise the assertions afterwards prove
        // nothing. Cancelled, so the history stays as it is (FR-006).
        let mut keyboard = Keyboard::attach(&nested.wayland_display);
        open_overlay(&nested, &mut keyboard);
        cancel(&nested, &mut keyboard);
    }

    nested.restart();
    let restarted = Instant::now();
    assert!(
        daemon.is_running(),
        "FR-025: losing an established connection must not end the process"
    );

    // FR-026b, SC-009: the shortcuts are re-registered with the new compositor, with no action
    // from the user.
    nested.wait_until("the daemon re-registers its shortcuts", || {
        nested.registered_shortcuts().len() == 2
    });
    let reconnected = restarted.elapsed();
    assert!(
        reconnected < Duration::from_secs(10),
        "SC-009 gives it ten seconds; it took {reconnected:?}"
    );

    // FR-026c: the history was discarded, so it now holds only what was observed *after* the
    // reconnection — activations of 1 then 2, i.e. [2, 1]. The overlay therefore opens on
    // workspace 1 and releasing goes there. Had the old history survived it would have been
    // [1, 2], the overlay would have opened on the already-active workspace 2, and releasing
    // would have changed nothing (FR-011).
    let _after_one = clients::spawn_on(&nested, 1, "after-1");
    let _after_two = clients::spawn_on(&nested, 2, "after-2");
    assert_compositor_order(&nested, &[1, 2]);

    // The two activations the rebuilt history is read from, driven explicitly. The daemon
    // re-registers its shortcuts a moment before it attaches to the new event socket, so an
    // activation raced against the reconnection could simply not have been observed; these come
    // seconds later, by which time it is certainly listening.
    activate(&nested, 1);
    activate(&nested, 2);

    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    gesture(&nested, &mut keyboard, 1);
    nested.wait_until("the rebuilt history opens on workspace 1", || {
        nested.active_workspace() == 1
    });

    let stderr = daemon.stderr();
    assert!(
        stderr.contains("INFO  compositor: connection lost"),
        "the loss is recorded: {stderr}"
    );
    assert!(
        stderr.contains("INFO  compositor: reconnected, state rebuilt, shortcuts re-registered"),
        "and so is the recovery: {stderr}"
    );
    // FR-031: recovery is stderr-only. Interrupting the user about something that fixed itself
    // is exactly what the notification policy exists to prevent.
    assert!(
        notify.raised().is_empty(),
        "a self-recovering condition raises no notification: {:?}",
        notify.raised()
    );
}

#[test]
fn e2e_no_overlay_while_disconnected() {
    // FR-026d: while disconnected the daemon must neither show an overlay nor spin retrying.
    //
    // The disconnection is staged by taking the Hyprland IPC sockets away from the daemon while
    // leaving the compositor itself running, which is the only arrangement in which the switcher
    // shortcut can actually be *fired* at a disconnected daemon — with the compositor gone there
    // would be nothing to press keys against. The daemon's response is the same either way: it
    // drops the whole client, so the compositor no longer has anything to deliver the bind to.
    let nested = Nested::start();
    let mut daemon = nested.start_daemon_stable(&[]);

    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch("workspace 2");
    nested.wait_until("the scenario starts on workspace 2", || {
        nested.active_workspace() == 2
    });

    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    open_overlay(&nested, &mut keyboard);
    cancel(&nested, &mut keyboard);

    // Take the sockets away, then make the world change so the daemon has reason to read them.
    nested.sever_ipc();
    nested.add_headless_output();
    nested.wait_until("the daemon drops its client", || {
        nested.registered_shortcuts().is_empty()
    });

    // FR-026d, first half: the shortcut fires into a compositor that has nobody registered for
    // it, and no overlay appears.
    keyboard.hold_with_taps(KEY_LEFTALT, KEY_TAB, 2, SETTLE);
    quiesce();
    assert!(
        nested.overlay_surfaces().is_empty(),
        "a disconnected daemon maps no layer surface"
    );
    assert_eq!(
        nested.active_workspace(),
        2,
        "and commits nothing while it is disconnected"
    );

    // FR-026d, second half: the retries are spaced out, not spun. A hot loop would burn a whole
    // core over this window; waiting out a capped backoff costs essentially nothing.
    let before = daemon.cpu_ticks();
    std::thread::sleep(IDLE_WINDOW);
    let spent = daemon.cpu_ticks() - before;
    assert!(
        spent <= 10,
        "FR-026d: {spent} clock ticks of CPU over {IDLE_WINDOW:?} of backoff is a busy loop"
    );
    assert!(
        daemon.is_running(),
        "FR-025: a lost connection is never fatal"
    );

    // And it recovers on its own, which is what makes the assertions above about a *disconnected*
    // daemon rather than a dead one.
    nested.restore_ipc();
    nested.wait_until("the daemon reconnects and re-registers", || {
        nested.registered_shortcuts().len() == 2
    });
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    open_overlay(&nested, &mut keyboard);
    cancel(&nested, &mut keyboard);
}

#[test]
fn e2e_no_notification_daemon() {
    // FR-032: with no `notify-send` to be found, the application says so once and carries on.
    // The configuration below is invalid on purpose, because a condition that *would* notify is
    // the only way to reach the delivery path at all.
    let path = NotifyLog::empty_path();
    let nested = Nested::start_with(
        &Setup::documented().with_app_config("presentation = \"tiles\"\nplacement = \"nowhere\"\n"),
    );
    let _one = clients::spawn_on(&nested, 1, "on-1");
    let _two = clients::spawn_on(&nested, 2, "on-2");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let daemon = nested.start_daemon_with_env(&[], &[("PATH", &path)]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Normal operation, undisturbed by the failed delivery.
    gesture(&nested, &mut keyboard, 1);
    nested.wait_until("the switcher still works", || {
        nested.active_workspace() == 2
    });

    let stderr = daemon.stderr();
    let warnings = stderr
        .lines()
        .filter(|line| line.starts_with("WARN  notify:"))
        .count();
    assert_eq!(
        warnings, 1,
        "reported at most once per process, for two notifying conditions: {stderr}"
    );
    assert!(
        stderr.contains("notify-send unavailable"),
        "and says what is missing: {stderr}"
    );
    // The underlying diagnostics still reach stderr every time (contracts/diagnostics.md).
    assert!(stderr.contains("WARN  config.presentation:"), "{stderr}");
    assert!(stderr.contains("WARN  config.placement:"), "{stderr}");
}

// ---------------------------------------------------------------------------
// Sticky mode
// ---------------------------------------------------------------------------

#[test]
fn e2e_sticky_mode_commits_on_enter() {
    // FR-022c: bound to a key carrying no modifier there is no release to commit on, so the
    // overlay must stay up after the shortcut is released and commit on Enter instead — with the
    // same navigation and the same cancel key as every other mode.
    let nested = Nested::start_with(
        &Setup::documented()
            .with_binds(&[Shortcut::NewWorkspace])
            .with_compositor_config("bind = , F12, global, hypr-swap:switcher\n"),
    );
    let _one = clients::spawn_on(&nested, 1, "sticky-1");
    let _two = clients::spawn_on(&nested, 2, "sticky-2");
    let _three = clients::spawn_on(&nested, 3, "sticky-3");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // Tap and let go. Held with a modifier this gesture would commit on the release; with no
    // modifier in the bind, the release must leave the overlay standing.
    keyboard.tap(KEY_F12);
    keyboard.settle();
    nested.wait_until("the overlay opens", || {
        !nested.overlay_monitors().is_empty()
    });
    quiesce();
    assert!(
        !nested.overlay_monitors().is_empty(),
        "FR-022c: no modifier release can ever arrive, so the overlay stays open"
    );
    assert_eq!(
        nested.active_workspace(),
        1,
        "and nothing is committed while it stands"
    );

    // The cancel key behaves as it does everywhere else (FR-006).
    keyboard.tap(KEY_ESC);
    keyboard.settle();
    nested.wait_until("Escape closes the overlay", || {
        nested.overlay_monitors().is_empty()
    });
    assert_eq!(nested.active_workspace(), 1, "cancelling commits nothing");

    // Open again and navigate. Nothing has been activated since the daemon started, so the MRU
    // order is still the compositor's and the overlay opens on entry 1 — workspace 2. One repeat
    // trigger advances it to workspace 3 (FR-003, FR-028).
    keyboard.tap(KEY_F12);
    keyboard.settle();
    nested.wait_until("the overlay reopens", || {
        !nested.overlay_monitors().is_empty()
    });
    keyboard.tap(KEY_F12);
    keyboard.settle();
    std::thread::sleep(SETTLE);

    keyboard.tap(KEY_ENTER);
    keyboard.settle();
    nested.wait_until("Enter commits the highlighted entry", || {
        nested.active_workspace() == 3
    });
    nested.wait_until("and the overlay closes with it", || {
        nested.overlay_monitors().is_empty()
    });

    let stderr = daemon.stderr();
    assert!(
        !stderr.contains("ERROR"),
        "sticky mode is ordinary operation, not a failure: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Process interface
// ---------------------------------------------------------------------------

#[test]
fn e2e_second_instance_refuses_to_start() {
    // FR-025a: registering the same shortcut ids twice is not an error at the protocol level, so
    // the second process has to notice the collision itself and refuse, rather than sit there
    // competing for deliveries with the instance already running.
    let notify = NotifyLog::new();
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "first-1");
    let _two = clients::spawn_on(&nested, 2, "first-2");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let _first = nested.start_daemon();
    assert_eq!(
        nested.registered_shortcuts().len(),
        2,
        "the first instance holds both names"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_hypr-swap"));
    nested.env(&mut command);
    let output = command
        .env("PATH", notify.path())
        .stdin(Stdio::null())
        .output()
        .expect("the application under test is built");

    assert_eq!(
        output.status.code(),
        Some(3),
        "contracts/cli.md: exit 3 rather than run as a second instance"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ERROR shortcut:") && stderr.contains("already registered"),
        "the record names the collision: {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "standard output is unused (contracts/cli.md)"
    );

    // FR-030: the user has to act on this one — two daemons is a configuration mistake.
    let raised = notify.wait_for(1);
    assert_eq!(raised.len(), 1, "exactly one notification: {raised:?}");
    assert!(
        raised[0].contains("hypr-swap: shortcut not registered"),
        "with the documented summary: {raised:?}"
    );

    // The instance that was already running is untouched by the one that refused to start.
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    gesture(&nested, &mut keyboard, 1);
    nested.wait_until("the first instance still switches", || {
        nested.active_workspace() == 2
    });
}

#[test]
fn e2e_version_and_help() {
    // FR-033: two options that print and exit successfully without becoming a daemon, and help
    // text carrying the bind lines — so a user who has the binary has the binding instructions
    // (FR-022b). No compositor is involved: all three cases answer before one is looked for.
    let notify = NotifyLog::new();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_hypr-swap"))
            .args(args)
            .env("PATH", notify.path())
            .stdin(Stdio::null())
            .output()
            .expect("the application under test is built")
    };

    let version = run(&["--version"]);
    assert_eq!(version.status.code(), Some(0), "contracts/cli.md: exit 0");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("{APP_ID} {VERSION}"),
        "--version prints the version and nothing else"
    );

    let help = run(&["--help"]);
    assert_eq!(help.status.code(), Some(0), "contracts/cli.md: exit 0");
    let text = String::from_utf8_lossy(&help.stdout);
    for shortcut in Shortcut::ALL {
        assert!(
            text.contains(&shortcut.suggested_bind()),
            "the usage text carries {:?}: {text}",
            shortcut.suggested_bind()
        );
    }

    // An unusable command line is a usage error, not a compositor problem. It must not put
    // "cannot reach Hyprland" on the user's screen for a mistyped flag (FR-030).
    let bad = run(&["--bogus"]);
    assert_eq!(
        bad.status.code(),
        Some(2),
        "contracts/cli.md: exit 2 on an invalid command line"
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains(r#"ERROR usage: unknown argument "--bogus""#),
        "the record names the argument: {stderr}"
    );
    assert!(
        stderr.contains("USAGE:"),
        "and the usage text follows it: {stderr}"
    );
    quiesce();
    assert!(
        notify.raised().is_empty(),
        "a command-line mistake raises no desktop notification: {:?}",
        notify.raised()
    );
}

#[test]
fn e2e_explicit_config_path_is_used_and_must_exist() {
    // FR-034: a configuration can be exercised without touching the user's own. The file named is
    // really the one read, and — unlike the default location, where absence is normal (FR-023) —
    // naming one that does not exist is an error.
    let nested = Nested::start();

    let absent = std::env::temp_dir().join(format!("hypr-swap-absent-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&absent);
    let mut command = Command::new(env!("CARGO_BIN_EXE_hypr-swap"));
    nested.env(&mut command);
    let output = command
        .arg("--config")
        .arg(&absent)
        .stdin(Stdio::null())
        .output()
        .expect("the application under test is built");

    assert_eq!(
        output.status.code(),
        Some(2),
        "contracts/cli.md: exit 2 when --config names a file that does not exist"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&absent.display().to_string()),
        "the record names the file it could not use: {stderr}"
    );

    // Named and present, it is the file the daemon actually reads. This setup writes nothing to
    // the default location, so a diagnostic about `presentation` can only have come from here.
    let explicit =
        std::env::temp_dir().join(format!("hypr-swap-explicit-{}.toml", std::process::id()));
    std::fs::write(&explicit, "presentation = \"tiles\"\n").expect("write the configuration");
    let daemon = nested.start_daemon_with(&[
        "--config",
        explicit.to_str().expect("a UTF-8 temporary path"),
    ]);
    quiesce();
    let stderr = daemon.stderr();
    let _ = std::fs::remove_file(&explicit);
    assert!(
        stderr.contains("WARN  config.presentation:"),
        "the explicitly named file was the one read: {stderr}"
    );
}
