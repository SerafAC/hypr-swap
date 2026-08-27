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
use e2e::harness::Nested;
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};

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
