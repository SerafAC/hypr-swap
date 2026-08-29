//! Phase 8 — the latency budgets (SC-001, SC-002) and the idle-cost claim.
//!
//! Every number here is measured from outside the process, by asking the compositor what it can
//! see, so each figure is an over-estimate: the truth lies somewhere inside the final sampling
//! interval. Over-estimating is the right direction for a budget check — a conservative
//! measurement that fits still fits. The two intervals differ by two orders of magnitude, which is
//! why the sample cost is printed alongside: whether a layer surface has mapped is only knowable
//! through `hyprctl`, a process spawn per observation, while the active workspace comes back over
//! the IPC socket. The R4 spike measured the SC-001 path from inside the client at 4 ms; this is
//! the external confirmation that nothing built on top of it has spent the headroom.

mod e2e;

use std::time::{Duration, Instant};

use e2e::clients;
use e2e::harness::{Nested, Setup};
use e2e::keyboard::{KEY_ESC, KEY_LEFTALT, KEY_TAB, Keyboard};
use e2e::overlay::paint_records;

use hypr_swap::diag::PAINT_RECORDS_VAR;

/// SC-001: shortcut pressed to overlay on screen.
const OVERLAY_BUDGET: Duration = Duration::from_millis(150);
/// SC-002: modifier released to the target workspace being the active one.
const COMMIT_BUDGET: Duration = Duration::from_millis(300);
/// Long enough that a daemon polling anything at all would be unmistakable.
const IDLE_WINDOW: Duration = Duration::from_secs(5);
/// When to stop waiting and call the measurement a failure.
const MEASURE_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll `condition` as fast as the compositor will answer, returning how long it took to hold.
fn time_until(what: &str, mut condition: impl FnMut() -> bool) -> Duration {
    let started = Instant::now();
    while started.elapsed() < MEASURE_TIMEOUT {
        if condition() {
            return started.elapsed();
        }
    }
    panic!("{what} never happened within {MEASURE_TIMEOUT:?}");
}

#[test]
fn e2e_meets_the_latency_budgets() {
    // SC-001 and SC-002, measured end to end through the real gesture.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "budget-1");
    let _two = clients::spawn_on(&nested, 2, "budget-2");
    let _three = clients::spawn_on(&nested, 3, "budget-3");
    nested.dispatch("workspace 1");
    nested.wait_until("the run starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    // What one `hyprctl` observation costs, which is the resolution of the SC-001 figure. The
    // SC-002 figure is sampled over the IPC socket instead and resolves far finer.
    let sample = {
        let started = Instant::now();
        for _ in 0..10 {
            let _ = nested.overlay_monitors();
        }
        started.elapsed() / 10
    };

    // SC-001. The bind fires on the TAB press with ALT already down, so that press is the instant
    // the user has asked for the overlay.
    keyboard.hold(KEY_LEFTALT);
    keyboard.press(KEY_TAB);
    let to_overlay = time_until("the overlay appears", || {
        !nested.overlay_monitors().is_empty()
    });
    keyboard.release(KEY_TAB);
    keyboard.settle();

    // SC-002. Nothing has been activated since the daemon started, so the MRU order is the
    // compositor's and the overlay opened on entry 1 — workspace 2.
    keyboard.release(KEY_LEFTALT);
    keyboard.flush();
    let to_commit = time_until("the target workspace becomes active", || {
        nested.active_workspace() == 2
    });

    println!("SC-001 shortcut → overlay:   {to_overlay:?} (budget {OVERLAY_BUDGET:?})");
    println!("SC-002 release  → workspace: {to_commit:?} (budget {COMMIT_BUDGET:?})");
    println!("one `hyprctl` observation costs {sample:?}: SC-001 is an over-estimate by that much");

    assert!(
        to_overlay <= OVERLAY_BUDGET,
        "SC-001: {to_overlay:?} against a {OVERLAY_BUDGET:?} budget"
    );
    assert!(
        to_commit <= COMMIT_BUDGET,
        "SC-002: {to_commit:?} against a {COMMIT_BUDGET:?} budget"
    );
}

#[test]
fn e2e_idle_costs_nothing() {
    // contracts/cli.md: one event loop over two file descriptors and no polling anywhere, so a
    // daemon with no overlay open must not accumulate CPU time worth measuring. A clock tick is
    // 10 ms, so this window would have to spend 500 of them to reach 1 % of one core.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "idle-1");
    let daemon = nested.start_daemon();

    // Let start-up finish, so the window covers the idle daemon and nothing else.
    std::thread::sleep(Duration::from_millis(500));
    let before = daemon.cpu_ticks();
    std::thread::sleep(IDLE_WINDOW);
    let spent = daemon.cpu_ticks() - before;

    println!("idle CPU over {IDLE_WINDOW:?}: {spent} clock ticks");
    assert!(
        spent <= 1,
        "an idle daemon polls nothing; this one spent {spent} clock ticks over {IDLE_WINDOW:?}"
    );
    assert!(
        nested.overlay_monitors().is_empty(),
        "the measurement covered an idle daemon, with no overlay open"
    );
}

/// SC-011: the SC-001 budget again, but under everything this feature added — a real icon set
/// being resolved and decoded, a built-in theme, and a full set of overrides — at the scale the
/// criterion names: 20 workspaces, 60 windows, 10 distinct programs.
///
/// This is the measurement T095 asks for, kept as a test rather than a one-off because it is the
/// guard on FR-043's whole reason for existing. Icons are resolved at start-up and on a world
/// rebuild, never on the open path (research.md R27), so the figure here should sit beside the
/// icon-less one rather than above it. A regression that moved resolution onto the open path
/// would show up here as a figure that scales with the number of distinct programs.
///
/// The daemon inherits the developer's real `XDG_DATA_*`, so the desktop entries and the icon
/// files are the machine's own — a fixture set of two icons would not measure decoding at all.
/// Ten programs with desktop entries on any ordinary system. `foot`'s `--app-id` stands in for
/// each one, which is the identity resolution is keyed on (research.md R21).
const SC011_PROGRAMS: [&str; 10] = [
    "firefox",
    "chromium",
    "org.gnome.Nautilus",
    "vlc",
    "gimp",
    "org.inkscape.Inkscape",
    "blender",
    "foot",
    "steam",
    "code",
];
const SC011_WORKSPACES: i32 = 20;
/// Three per workspace: 60 windows across 20 workspaces, as SC-011 specifies.
const SC011_PER_WORKSPACE: usize = 3;

/// A built-in theme and a valid override of every kind — a colour, both font values and two
/// dimensions — because SC-011 claims the budget holds under *any* of them.
const SC011_CONFIG: &str = "\
theme = \"light\"
icon_set = \"Papirus-Dark\"

[style]
highlight        = \"#c04a2f\"
font_family      = \"JetBrains Mono\"
text_size        = 0.85
text_line_height = 24
width_fraction   = 0.95
";

/// Fill the instance with the scale SC-011 names, leaving it on workspace 1.
fn stage_sc011(nested: &Nested) -> Vec<clients::Client> {
    let mut windows = Vec::new();
    for workspace in 1..=SC011_WORKSPACES {
        for slot in 0..SC011_PER_WORKSPACE {
            #[allow(clippy::cast_sign_loss)]
            let index = workspace as usize * SC011_PER_WORKSPACE + slot;
            windows.push(clients::spawn_as_on(
                nested,
                Some(SC011_PROGRAMS[index % SC011_PROGRAMS.len()]),
                workspace,
                &format!("sc011-{workspace}-{slot}"),
            ));
        }
    }
    nested.wait_until("every workspace exists", || {
        nested
            .workspaces()
            .iter()
            .filter(|workspace| workspace.id >= 1)
            .count()
            >= SC011_WORKSPACES as usize
    });
    nested.dispatch("workspace 1");
    nested.wait_until("the run starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    windows
}

#[test]
fn e2e_meets_the_latency_budget_with_icons() {
    let nested = Nested::start_with(&Setup::documented().with_app_config(SC011_CONFIG));
    let windows = stage_sc011(&nested);
    assert_eq!(
        windows.len(),
        SC011_WORKSPACES as usize * SC011_PER_WORKSPACE
    );

    // The paint records are on so the run can say what it actually drew, rather than leaving
    // "icons were enabled" as an assumption about a figure.
    let daemon = nested.start_daemon_with_env(&[], &[(PAINT_RECORDS_VAR, "1")]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let sample = {
        let started = Instant::now();
        for _ in 0..10 {
            let _ = nested.overlay_monitors();
        }
        started.elapsed() / 10
    };

    keyboard.hold(KEY_LEFTALT);
    keyboard.press(KEY_TAB);
    let to_overlay = time_until("the overlay appears", || {
        !nested.overlay_monitors().is_empty()
    });
    keyboard.release(KEY_TAB);
    keyboard.settle();
    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay unmaps", || {
        nested.overlay_surfaces().is_empty()
    });

    let stderr = daemon.stderr();
    let records = paint_records(&stderr);
    let drawn: usize = records
        .iter()
        .map(|record| record.matches("icons=[").count())
        .sum();

    println!(
        "SC-011 shortcut → overlay: {to_overlay:?} (budget {OVERLAY_BUDGET:?}) with \
         {SC011_WORKSPACES} workspaces, {} windows, {} programs, icons on, light theme + \
         overrides",
        windows.len(),
        SC011_PROGRAMS.len()
    );
    println!(
        "one `hyprctl` observation costs {sample:?}: the figure is an over-estimate by that much"
    );
    println!(
        "{} paint records, {drawn} carrying an icon list",
        records.len()
    );

    assert!(
        !records.is_empty(),
        "no paint records, so this measured an overlay that may not have drawn icons at all:\n\
         {stderr}"
    );
    assert!(
        to_overlay <= OVERLAY_BUDGET,
        "SC-011: {to_overlay:?} against a {OVERLAY_BUDGET:?} budget"
    );
}
