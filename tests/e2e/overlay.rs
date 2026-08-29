//! Opening the overlay, measuring it, and reading what it painted.
//!
//! The two interfaces research.md R22 allows, in one place: `hyprctl layers` for the surface
//! geometry, and the env-gated paint records on the daemon's stderr for everything else. Shared
//! by every test file that asserts on the overlay's appearance, so the panel the pre-feature
//! baseline was recorded on and the way a record is parsed are each written once.

use std::path::Path;
use std::time::Duration;

use super::clients;
use super::harness::Nested;
use super::keyboard::{KEY_ESC, KEY_LEFTALT, KEY_TAB, Keyboard};

/// The same panel the baseline was recorded on, so the numbers are comparable at all
/// (`tests/fixtures/baseline/README.md`).
pub const PANEL_MODE: &str = "1920x1080@60,auto,1";

/// Long enough for the compositor to have configured and the daemon to have painted. The
/// `wait_until` calls below do the real waiting; this only keeps a paint from being sampled
/// mid-`configure`.
pub const SETTLE: Duration = Duration::from_millis(200);

/// The application configuration that selects the grid presentation (FR-016).
pub const GRID: &str = "presentation = \"grid\"\n";

/// The committed pre-feature baseline for one presentation.
///
/// # Panics
/// If the file is missing or not JSON — a missing baseline must fail loudly rather than silently
/// skip every "unchanged from before this feature" comparison.
#[must_use]
pub fn baseline(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("baseline")
        .join(name);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&source).expect("the baseline is valid JSON")
}

/// Pin the focused output to the mode the baseline was recorded on.
///
/// # Panics
/// If the output never reports the pinned mode.
#[must_use]
pub fn pinned_panel(nested: &Nested) -> String {
    let panel = nested.add_headless_output();
    nested.hyprctl(&["keyword", "monitor", &format!("{panel},{PANEL_MODE}")]);
    nested.dispatch(&format!("focusmonitor {panel}"));
    nested.wait_until("the pinned panel is focused at 1920x1080 scale 1", || {
        nested.monitors().iter().any(|monitor| {
            monitor.name == panel
                && monitor.focused
                && monitor.size == (1920, 1080)
                && (monitor.scale - 1.0).abs() < 0.01
        })
    });
    panel
}

/// The scenario the baseline was recorded against: four workspaces, one of them crowded.
///
/// # Panics
/// If a window never appears.
#[must_use]
pub fn stage_scenario(nested: &Nested, panel: &str) -> Vec<clients::Client> {
    stage_classified(nested, panel, None)
}

/// The same scenario, optionally spawning every window under one chosen class — which is how the
/// icon tests give the baseline's windows a program identity to resolve (FR-040).
///
/// # Panics
/// If a window never appears.
#[must_use]
pub fn stage_classified(nested: &Nested, panel: &str, class: Option<&str>) -> Vec<clients::Client> {
    let mut windows = Vec::new();
    for (workspace, titles) in [
        (1, &["alpha-window"][..]),
        (2, &["beta-window", "gamma-window"][..]),
        (3, &[][..]),
        (
            4,
            &[
                "crowded-window-one",
                "crowded-window-two",
                "crowded-window-three",
                "crowded-window-four",
                "crowded-window-five",
            ][..],
        ),
    ] {
        nested.dispatch(&format!("moveworkspacetomonitor {workspace} {panel}"));
        for title in titles {
            windows.push(clients::spawn_as_on(nested, class, workspace, title));
        }
    }
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    windows
}

/// The per-entry paint records the daemon emitted, in the order it emitted them.
#[must_use]
pub fn paint_records(stderr: &str) -> Vec<String> {
    records_of(stderr, "entry ")
}

/// The colour records — one per paint, each the distinct `#rrggbbaa` values that paint handed to
/// cairo, in first-use order (T058).
///
/// A separate list rather than a filter at the call site: the two record shapes share the `paint:`
/// subject, and a test asserting on entries must not have to skip colour lines by hand.
#[must_use]
pub fn paint_colours(stderr: &str) -> Vec<Vec<String>> {
    records_of(stderr, "colours ")
        .iter()
        .filter_map(|record| record.split_once('['))
        .filter_map(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list.split_whitespace().map(str::to_owned).collect())
        .collect()
}

/// Every `paint:` record of one shape, with the subject stripped.
fn records_of(stderr: &str, shape: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| line.split_once("paint: "))
        .map(|(_, record)| record)
        .filter(|record| record.starts_with(shape))
        .map(str::to_owned)
        .collect()
}

/// One field of a paint record, as the renderer wrote it — `icons=[…]` yields the bracketed part.
///
/// Records are `key=value` pairs separated by spaces, with two values that contain spaces of
/// their own: a quoted label and a bracketed icon list. Both are read by their delimiters rather
/// than by splitting on whitespace, so a window title with a space in it cannot break the parse.
#[must_use]
pub fn field<'a>(record: &'a str, key: &str) -> Option<&'a str> {
    let after = record.split_once(&format!("{key}="))?.1;
    match after.as_bytes().first() {
        Some(b'[') => after[1..].split_once(']').map(|(value, _)| value),
        Some(b'"') => after[1..].split_once('"').map(|(value, _)| value),
        _ => Some(after.split_whitespace().next().unwrap_or_default()),
    }
}

/// The icon sources one record names, in the order they were drawn (research.md R22).
#[must_use]
pub fn icons_of(record: &str) -> Vec<&str> {
    field(record, "icons")
        .unwrap_or_default()
        .split_whitespace()
        .collect()
}

/// What each window rectangle in a grid miniature had room for, in the order they were drawn —
/// `icon+title`, `icon`, `title` or `none` (FR-038, research.md R22).
#[must_use]
pub fn rects_of(record: &str) -> Vec<&str> {
    field(record, "rects")
        .unwrap_or_default()
        .split_whitespace()
        .collect()
}

/// Open the overlay, read the compositor while it is up, then cancel it.
///
/// The one place the open-inspect-close gesture is written, so a test that wants to see the
/// overlay in some other way than by its geometry does not have to repeat the key sequence.
///
/// # Panics
/// If the overlay never maps or never unmaps.
pub fn open_while<T>(nested: &Nested, keyboard: &mut Keyboard, inspect: impl FnOnce() -> T) -> T {
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());

    let seen = inspect();

    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay unmaps", || {
        nested.overlay_surfaces().is_empty()
    });
    seen
}

/// Open the overlay, hold it open long enough to be measured and painted, then close it.
///
/// # Panics
/// If the overlay never maps or never unmaps.
pub fn open_and_close(nested: &Nested, keyboard: &mut Keyboard) {
    open_while(nested, keyboard, || ());
}

/// Open the overlay, measure it, close it — and return the surface geometry the compositor
/// reported while it was up.
///
/// # Panics
/// If the overlay never maps, never unmaps, or is not reported by `hyprctl layers`.
pub fn measure(nested: &Nested, keyboard: &mut Keyboard) -> (i32, i32, u32, u32) {
    open_while(nested, keyboard, || {
        nested.overlay_xywh().expect("the overlay surface")
    })
}
