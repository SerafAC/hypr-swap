//! The overlay's appearance as resolved style values, asserted against the compositor and the
//! daemon's own stderr (research.md R22).
//!
//! Nothing here compares pixels — feature 001's R14 rejected screenshot comparison as brittle
//! across fonts and scaling, and that reasoning is unchanged. The two real interfaces carry the
//! assertions instead: `hyprctl layers` for geometry, and the env-gated paint records for what the
//! renderer actually drew.

mod e2e;

use std::path::{Path, PathBuf};

use e2e::harness::{Nested, Setup};
use e2e::keyboard::Keyboard;
use e2e::overlay::{
    GRID, baseline, measure, open_while, paint_colours, paint_records, pinned_panel, stage_scenario,
};
use e2e::style::{
    GRID_ELEMENTS, LIST_ELEMENTS, assert_drawn_in, assert_every_paint, colour_of, painted,
};

use hypr_swap::diag::PAINT_RECORDS_VAR;
use hypr_swap::theme;

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

// ---------------------------------------------------------------------------
// User Story 2 — one setting recolours the whole overlay (T058–T062)
// ---------------------------------------------------------------------------

/// The whole of what a user writes to change the overlay's appearance (SC-020, US2-AS1).
const LIGHT: &str = "theme = \"light\"\n";

/// US2-AS1/AS2, SC-020, SC-021: naming a built-in theme recolours every themed element of both
/// presentations, and the naming is one line of configuration (FR-045, FR-048, FR-053).
#[test]
fn e2e_builtin_theme_applies() {
    assert_eq!(LIGHT.lines().count(), 1, "SC-020: one line, not a palette");

    let list = painted(Some(LIGHT));
    assert_every_paint(&list, &theme::LIGHT, LIST_ELEMENTS, &theme::DARK);

    let grid = painted(Some(&format!("{LIGHT}{GRID}")));
    assert_every_paint(&grid, &theme::LIGHT, GRID_ELEMENTS, &theme::DARK);
}

/// US2-AS3, FR-048: with a copy on every monitor, every copy is the same theme — one resolved
/// style drives them all rather than one per surface.
#[test]
fn e2e_theme_on_all_monitors() {
    let nested = Nested::start_with(
        &Setup::documented().with_app_config(&format!("{LIGHT}placement = \"all\"\n")),
    );
    let panel = pinned_panel(&nested);
    let other = nested.add_headless_output();
    let _windows = stage_scenario(&nested, &panel);

    let daemon = nested.start_daemon_with_env(&[], &[(PAINT_RECORDS_VAR, "1")]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let monitors = open_while(&nested, &mut keyboard, || nested.overlay_monitors());
    let mut expected: Vec<String> = nested
        .monitors()
        .iter()
        .map(|monitor| monitor.name.clone())
        .collect();
    expected.sort();
    assert!(
        expected.len() >= 2 && expected.contains(&panel) && expected.contains(&other),
        "the scenario did not bring up two outputs to theme: {expected:?}"
    );
    assert_eq!(monitors, expected, "one copy per connected monitor");

    let stderr = daemon.stderr();
    let paints = paint_colours(&stderr);
    assert!(
        paints.len() >= 2,
        "two surfaces painted fewer than two passes between them:\n{stderr}"
    );
    for drawn in &paints {
        assert_drawn_in(drawn, &theme::LIGHT, LIST_ELEMENTS, &theme::DARK);
    }
}

/// US2-AS4, FR-049a, SC-018: with no configuration at all the overlay's colours and geometry are
/// the ones recorded from the pre-feature build.
///
/// Colours and geometry only — icons are not this test's business, which is what keeps US2
/// independent of US1 (T060).
#[test]
fn e2e_default_appearance_unchanged() {
    let recorded = baseline("style.json");
    let palette = recorded["palette"]
        .as_object()
        .expect("the baseline records a palette");

    let painted = painted(None);

    // Geometry: the surface is exactly where and how big the pre-feature build put it.
    let surface = &baseline("list.json")["surface"];
    assert_eq!(
        (
            i64::from(painted.surface.0),
            i64::from(painted.surface.1),
            i64::from(painted.surface.2),
            i64::from(painted.surface.3),
        ),
        (
            surface["x_on_monitor"].as_i64().expect("x_on_monitor"),
            surface["y_on_monitor"].as_i64().expect("y_on_monitor"),
            surface["w"].as_i64().expect("w"),
            surface["h"].as_i64().expect("h"),
        ),
        "the default overlay's geometry has moved since before this feature"
    );

    // Colours: every value that reached cairo is one the pre-feature renderer used.
    let recorded_colours: Vec<String> = palette
        .values()
        .map(|colour| {
            colour["hex"]
                .as_str()
                .expect("the baseline records a hex form")
                .to_owned()
        })
        .collect();
    assert!(
        !painted.colours.is_empty(),
        "the gate produced no colour records:\n{}",
        painted.stderr
    );
    for drawn in &painted.colours {
        for colour in drawn {
            assert!(
                recorded_colours.contains(colour),
                "{colour} was never used by the pre-feature renderer; the paint drew {drawn:?}"
            );
        }
        for element in LIST_ELEMENTS {
            let expected = palette[*element]["hex"]
                .as_str()
                .expect("the baseline records a hex form")
                .to_owned();
            assert!(
                drawn.contains(&expected),
                "the {element} was not drawn in the recorded {expected}; the paint drew {drawn:?}"
            );
        }
    }
}

/// US2-AS5, FR-049, SC-023: a theme is a palette, so switching one cannot move anything. Asserted
/// on the compositor's own geometry rather than on the type — the structural half is the unit test
/// in `theme.rs`.
#[test]
fn e2e_theme_switch_does_not_move_layout() {
    for presentation in ["", GRID] {
        let dark = painted(Some(&format!("theme = \"dark\"\n{presentation}")));
        let light = painted(Some(&format!("{LIGHT}{presentation}")));
        assert_eq!(
            dark.surface,
            light.surface,
            "the overlay moved between themes in the {} presentation",
            if presentation.is_empty() {
                "list"
            } else {
                "grid"
            }
        );
        // And the switch really happened, so the geometry above was not identical for the dull
        // reason that both runs drew the same theme.
        assert_drawn_in(&dark.colours[0], &theme::DARK, LIST_ELEMENTS, &theme::LIGHT);
        assert_drawn_in(
            &light.colours[0],
            &theme::LIGHT,
            LIST_ELEMENTS,
            &theme::DARK,
        );
    }
}

/// US2-AS6, FR-058: an unknown theme name is reported, the default applies, every other setting
/// still applies, and the daemon carries on.
#[test]
fn e2e_unknown_theme_falls_back() {
    let accent = "#ff0000";
    let painted = painted(Some(&format!(
        "theme = \"dracula\"\n{GRID}\n[style]\nhighlight = \"{accent}\"\n"
    )));

    assert!(
        painted.stderr.contains("WARN  config.theme:"),
        "the unknown name was not reported:\n{}",
        painted.stderr
    );
    assert!(
        painted.stderr.contains(r#"unknown theme "dracula""#)
            && painted.stderr.contains(r#"using "dark""#),
        "the report does not say what was wrong and what was used instead:\n{}",
        painted.stderr
    );

    // Every other setting still applies: the grid was drawn, and the override took effect.
    let records = paint_records(&painted.stderr);
    assert!(
        !records.is_empty() && records.iter().all(|record| record.contains(" grid:")),
        "the presentation setting was dropped along with the theme: {records:?}"
    );
    assert!(
        !painted.colours.is_empty(),
        "the daemon painted nothing at all:\n{}",
        painted.stderr
    );
    for drawn in &painted.colours {
        // The fallback palette, with the one overridden value in place of the theme's own.
        let overridden = format!("{accent}ff");
        assert!(
            drawn.contains(&overridden),
            "the override was dropped along with the theme; the paint drew {drawn:?}"
        );
        assert!(
            !drawn.contains(&colour_of(&theme::DARK, "highlight")),
            "the overridden highlight was drawn in the theme's own colour: {drawn:?}"
        );
        for element in GRID_ELEMENTS {
            if *element == "highlight" {
                continue;
            }
            let expected = colour_of(&theme::DARK, element);
            assert!(
                drawn.contains(&expected),
                "the {element} did not fall back to the default theme's {expected}: {drawn:?}"
            );
        }
    }
}
