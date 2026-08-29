//! The overlay's appearance as resolved style values, asserted against the compositor and the
//! daemon's own stderr (research.md R22).
//!
//! Nothing here compares pixels — feature 001's R14 rejected screenshot comparison as brittle
//! across fonts and scaling, and that reasoning is unchanged. The two real interfaces carry the
//! assertions instead: `hyprctl layers` for geometry, and the env-gated paint records for what the
//! renderer actually drew.

mod e2e;

use std::path::{Path, PathBuf};
use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, Setup};
use e2e::keyboard::{KEY_ESC, KEY_LEFTALT, KEY_TAB, Keyboard};

use hypr_swap::diag::PAINT_RECORDS_VAR;

/// The same panel the baseline was recorded on, so the numbers are comparable at all
/// (`tests/fixtures/baseline/README.md`).
const PANEL_MODE: &str = "1920x1080@60,auto,1";

const SETTLE: Duration = Duration::from_millis(200);

/// The application configuration that selects the grid presentation (FR-016).
const GRID: &str = "presentation = \"grid\"\n";

/// The committed pre-feature baseline for one presentation.
fn baseline(name: &str) -> serde_json::Value {
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
fn pinned_panel(nested: &Nested) -> String {
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
fn stage_scenario(nested: &Nested, panel: &str) -> Vec<clients::Client> {
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
            windows.push(clients::spawn_on(nested, workspace, title));
        }
    }
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    windows
}

/// The paint records the daemon emitted, in the order it emitted them.
fn paint_records(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| line.split_once("paint: "))
        .map(|(_, record)| record.to_owned())
        .collect()
}

/// Open the overlay, measure it, close it — and return the surface geometry the compositor
/// reported while it was up.
fn measure(nested: &Nested, keyboard: &mut Keyboard) -> (i32, i32, u32, u32) {
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());

    let geometry = nested.overlay_xywh().expect("the overlay surface");

    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay unmaps", || {
        nested.overlay_surfaces().is_empty()
    });
    geometry
}

/// The foundational phase's checkpoint: every colour and dimension now comes from a resolved
/// `Style`, and the overlay is unchanged (FR-049a, SC-018).
///
/// Asserts on both interfaces the research decisions allow. The geometry is compared against the
/// numbers recorded from the pre-feature build in `tests/fixtures/baseline/`, which can never be
/// re-recorded — if this fails, the refactor changed something no requirement asked it to.
///
/// Only geometry and the drawn entries are compared, deliberately: icons do not exist yet, so
/// "the icons are the only difference" (SC-018) is not this test's claim. It is closed by
/// `e2e_icons_disabled_matches_pre_feature` once the icon stories land.
fn refactor_is_pixel_neutral(presentation: &str, app_config: Option<&str>) {
    let recorded = baseline(&format!("{presentation}.json"));

    let setup = match app_config {
        // No configuration file at all in the default case, which is the case FR-049a is about.
        Some(toml) => Setup::documented().with_app_config(toml),
        None => Setup::documented(),
    };
    let nested = Nested::start_with(&setup);
    let panel = pinned_panel(&nested);
    let _windows = stage_scenario(&nested, &panel);

    let daemon = nested.start_daemon_with_env(&[], &[(PAINT_RECORDS_VAR, "1")]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let (x, y, width, height) = measure(&nested, &mut keyboard);

    let expected = &recorded["surface"];
    assert_eq!(
        (
            i64::from(x),
            i64::from(y),
            i64::from(width),
            i64::from(height)
        ),
        (
            expected["x_on_monitor"].as_i64().expect("x_on_monitor"),
            expected["y_on_monitor"].as_i64().expect("y_on_monitor"),
            expected["w"].as_i64().expect("w"),
            expected["h"].as_i64().expect("h"),
        ),
        "the {presentation} overlay's geometry has moved since before the refactor"
    );

    let stderr = daemon.stderr();
    let records = paint_records(&stderr);
    let entries = recorded["scenario"]["entries"]
        .as_array()
        .expect("the baseline records the entries");
    let visible = usize::try_from(
        recorded["metrics"]["visible_entries"]
            .as_u64()
            .expect("the baseline records the visible entry count"),
    )
    .expect("a plausible entry count");

    assert!(
        !records.is_empty(),
        "the gate produced no paint records at all; stderr was:\n{stderr}"
    );

    // One record per entry on screen, per *paint*. The overlay repaints several times over one
    // opening — a `configure` and its commits — so the records arrive as whole passes rather than
    // as one pass, and the assertion is that every pass drew the same thing. Chunking rather than
    // deduplicating is what would catch a pass that painted a different set of entries.
    let on_screen = visible.min(entries.len());
    assert_eq!(
        records.len() % on_screen,
        0,
        "the records do not divide into whole paints of {on_screen} entries: {records:?}"
    );

    for (pass, chunk) in records.chunks(on_screen).enumerate() {
        for (index, record) in chunk.iter().enumerate() {
            let entry = &entries[index];
            let label = entry["label"].as_str().expect("a label");
            let windows = entry["windows"].as_array().expect("windows").len();
            assert!(
                record.starts_with(&format!("entry {index} {presentation}:")),
                "pass {pass} record {index} names the wrong entry or presentation: {record}"
            );
            assert!(
                record.contains(&format!("label={label:?}")),
                "pass {pass} record {index} drew {record}, expected the baseline's {label:?}"
            );
            assert!(
                record.contains(&format!("windows={windows}")),
                "pass {pass} record {index} drew {record}, expected {windows} windows"
            );
        }
    }
}

#[test]
fn e2e_refactor_is_pixel_neutral() {
    refactor_is_pixel_neutral("list", None);
}

#[test]
fn e2e_refactor_is_pixel_neutral_in_the_grid() {
    refactor_is_pixel_neutral("grid", Some(GRID));
}

/// Silence is the normal case: without the gate the daemon says nothing about what it painted,
/// so the hook costs a user nothing (research.md R22).
#[test]
fn e2e_paint_records_are_silent_without_the_gate() {
    let nested = Nested::start_with(&Setup::documented());
    let panel = pinned_panel(&nested);
    let _windows = stage_scenario(&nested, &panel);

    let daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let _ = measure(&nested, &mut keyboard);

    let stderr = daemon.stderr();
    assert!(
        paint_records(&stderr).is_empty(),
        "an ordinary run emitted paint records:\n{stderr}"
    );
}

/// Keeps the import honest: the baseline directory is where every "unchanged from before this
/// feature" comparison reads from, and a missing file must fail loudly rather than silently
/// skipping the comparison above.
#[test]
fn the_recorded_baseline_is_present() {
    for name in ["list.json", "grid.json", "style.json"] {
        let recorded = baseline(name);
        assert!(
            recorded.is_object(),
            "{name} is not the recorded baseline object"
        );
    }
    let _: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
}
