//! Overriding individual colours, fonts and geometry on top of a theme (US4, US5).
//!
//! The same two interfaces as `e2e_theme.rs` and for the same reason (research.md R22): the
//! compositor's own view of the surface for geometry, and the env-gated paint records for the
//! colours and fonts that actually reached the buffer. What separates these tests from US2's is
//! that every one of them asserts *independence* — that one setting reaching the overlay says
//! nothing about the others, and that one setting failing takes nothing else with it (FR-050,
//! FR-059, SC-022).

mod e2e;

use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, Setup};
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};
use e2e::overlay::{GRID, measure, pinned_panel, stage_scenario};
use e2e::style::{
    GRID_ELEMENTS, LIST_ELEMENTS, assert_drawn_over, assert_every_paint_over, colour_of, painted,
};

use hypr_swap::config::{self, Presentation};
use hypr_swap::ordering;
use hypr_swap::state::World;
use hypr_swap::theme::{self, Geometry};
use hypr_swap::ui::layout;

/// An accent no built-in theme uses, so seeing it can only mean the override applied.
const ACCENT: &str = "#ff00ff";

/// The same value as the renderer records it — every colour on the tape carries its alpha.
fn opaque(hex: &str) -> String {
    format!("{hex}ff")
}

/// US4-AS1, FR-050: a theme plus one colour override — that element is the override, and every
/// other element is still the named theme's.
///
/// Asserted against both themes: nothing of `dark`'s appears, which is US2's claim that the theme
/// applied, and nothing of `light`'s highlight appears either, which is the new claim that the
/// override beat the theme it sits on rather than being ignored.
#[test]
fn e2e_colour_override_wins_over_theme() {
    let overrides = [("highlight", opaque(ACCENT))];
    let overrides: Vec<(&str, &str)> = overrides
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();

    for (presentation, elements) in [("", LIST_ELEMENTS), (GRID, GRID_ELEMENTS)] {
        let painted = painted(Some(&format!(
            "theme = \"light\"\n{presentation}\n[style]\nhighlight = \"{ACCENT}\"\n"
        )));
        assert_every_paint_over(&painted, &theme::LIGHT, elements, &theme::DARK, &overrides);
        assert_every_paint_over(&painted, &theme::LIGHT, elements, &theme::LIGHT, &overrides);
        assert!(
            painted.running,
            "the daemon stopped after an override:\n{}",
            painted.stderr
        );
    }
}

/// US4-AS2, FR-050: overrides with no theme name at all apply on top of the default theme, rather
/// than being ignored for want of a theme to sit on.
#[test]
fn e2e_overrides_without_theme() {
    let text = "#ff0000";
    let mark = "#00ff00";
    let painted = painted(Some(&format!(
        "[style]\ntext = \"{text}\"\nactive_mark = \"{mark}\"\n"
    )));

    let overrides = [("text", opaque(text)), ("active_mark", opaque(mark))];
    let overrides: Vec<(&str, &str)> = overrides
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    // `other` is the default theme itself: the two overridden keys must not appear in its own
    // colours, and the other nine must.
    assert_every_paint_over(
        &painted,
        &theme::DARK,
        LIST_ELEMENTS,
        &theme::DARK,
        &overrides,
    );
    assert!(
        !painted.stderr.contains("config.style."),
        "a valid override was reported:\n{}",
        painted.stderr
    );
}

/// US4-AS3, FR-046: a `font_family` override reaches every piece of text the overlay draws, in
/// both presentations — one requested family per paint, and it is the configured one.
#[test]
fn e2e_font_override_applies() {
    // A generic family, so the assertion is about the override arriving rather than about which
    // fonts happen to be installed on the machine running the suite.
    let family = "Monospace";

    let default = painted(None);
    assert_eq!(
        requested(&default.fonts),
        vec![theme::DEFAULT_FONT_FAMILY.to_owned()],
        "the unconfigured overlay did not lay its text out in the default family:\n{}",
        default.stderr
    );

    for presentation in ["", GRID] {
        let painted = painted(Some(&format!(
            "{presentation}\n[style]\nfont_family = \"{family}\"\n"
        )));
        assert!(
            !painted.fonts.is_empty(),
            "the gate produced no font records:\n{}",
            painted.stderr
        );
        for (asked, loaded) in &painted.fonts {
            assert_eq!(
                asked,
                &vec![family.to_owned()],
                "some text was laid out in another family:\n{}",
                painted.stderr
            );
            assert!(
                loaded.iter().all(|family| !family.is_empty()),
                "a family was asked for and nothing was loaded for it: {loaded:?}"
            );
        }
    }
}

/// US4-AS5: a family that is not installed is substituted by the platform, the text is still
/// drawn, and nothing is reported about it.
#[test]
fn e2e_missing_font_substitutes() {
    let absent = "No Such Family 84e1c0";
    let painted = painted(Some(&format!("[style]\nfont_family = \"{absent}\"\n")));

    assert!(
        !painted.fonts.is_empty(),
        "the gate produced no font records:\n{}",
        painted.stderr
    );
    for (asked, loaded) in &painted.fonts {
        assert_eq!(asked, &vec![absent.to_owned()], "{}", painted.stderr);
        assert!(
            loaded.iter().all(|family| !family.is_empty()) && !loaded.is_empty(),
            "nothing was loaded for the absent family, so nothing was drawn: {loaded:?}"
        );
        assert!(
            !loaded.contains(&absent.to_owned()),
            "the absent family was reported as loaded, so the substitution was not observed: \
             {loaded:?}"
        );
    }

    // Readable: the entries were painted with their names, exactly as with any other family.
    let records = e2e::overlay::paint_records(&painted.stderr);
    assert!(
        !records.is_empty() && records.iter().any(|record| record.contains("label=")),
        "no entry was drawn at all: {records:?}"
    );

    // And silent: an absent family is the platform's business, not a configuration error.
    let reported: Vec<&str> = painted
        .stderr
        .lines()
        .filter(|line| line.contains("font") || line.contains("config.style"))
        .filter(|line| line.starts_with("WARN") || line.starts_with("ERROR"))
        .collect();
    assert!(
        reported.is_empty(),
        "the substitution was reported: {reported:?}"
    );
    assert!(painted.running, "the daemon stopped over an absent family");
}

/// US4-AS4, FR-059, SC-022: one unreadable value among several good ones is reported once, falls
/// back on its own, and leaves every other setting — of every kind — applied.
#[test]
fn e2e_invalid_value_falls_back_alone() {
    let painted = painted(Some(&format!(
        "theme = \"light\"\n{GRID}\n[style]\n\
         highlight = \"octarine\"\n\
         active_mark = \"{ACCENT}\"\n\
         font_family = \"Monospace\"\n\
         row_padding = 10\n"
    )));

    // Reported once, naming the setting, what was wrong, and the value used instead.
    let reported: Vec<&str> = painted
        .stderr
        .lines()
        .filter(|line| line.contains("config.style.highlight"))
        .collect();
    assert_eq!(reported.len(), 1, "{:?}", painted.stderr);
    let report = reported[0];
    assert!(
        report.starts_with("WARN  config.style.highlight:"),
        "{report}"
    );
    assert!(
        report.contains("expected #rgb, #rrggbb or #rrggbbaa")
            && report.contains(r#"got "octarine""#)
            // The message spells a colour as `#rrggbb`; the paint tape spells it `#rrggbbaa`.
            && report.contains(&format!("using {}", theme::LIGHT.highlight.hex())),
        "the report does not say what was wrong and what was used: {report}"
    );
    assert!(
        !painted.stderr.contains("config.style.active_mark")
            && !painted.stderr.contains("config.style.font_family")
            && !painted.stderr.contains("config.style.row_padding"),
        "a good setting was reported alongside the bad one:\n{}",
        painted.stderr
    );

    // The bad colour alone fell back to the theme's own value; the good one still applied.
    let overrides = [("active_mark", opaque(ACCENT))];
    let overrides: Vec<(&str, &str)> = overrides
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    assert_every_paint_over(
        &painted,
        &theme::LIGHT,
        GRID_ELEMENTS,
        &theme::DARK,
        &overrides,
    );
    for drawn in &painted.colours {
        assert!(
            drawn.contains(&colour_of(&theme::LIGHT, "highlight")),
            "the highlight did not fall back to the theme's own value: {drawn:?}"
        );
    }

    // And every setting of every other kind still applied: the presentation, the font, and the
    // geometry override beside them.
    let records = e2e::overlay::paint_records(&painted.stderr);
    assert!(
        !records.is_empty() && records.iter().all(|record| record.contains(" grid:")),
        "the presentation was dropped along with the bad colour: {records:?}"
    );
    assert_eq!(
        requested(&painted.fonts),
        vec!["Monospace".to_owned()],
        "the font override was dropped along with the bad colour:\n{}",
        painted.stderr
    );
    assert!(
        painted.running,
        "the daemon stopped over one unreadable value:\n{}",
        painted.stderr
    );

    // A last check that the comparison above is not vacuous: `dark`'s highlight, which the
    // fallback would have used had the theme been dropped too, is nowhere on the tape.
    for drawn in &painted.colours {
        assert_drawn_over(
            drawn,
            &theme::LIGHT,
            GRID_ELEMENTS,
            &theme::DARK,
            &overrides,
        );
    }
}

/// The distinct families every paint of one opening asked for, which must agree pass to pass.
///
/// # Panics
/// If the paints disagree, or if nothing was painted at all.
fn requested(fonts: &[(Vec<String>, Vec<String>)]) -> Vec<String> {
    assert!(!fonts.is_empty(), "no paint recorded a font");
    let first = fonts[0].0.clone();
    for (asked, _) in fonts {
        assert_eq!(asked, &first, "two paints asked for different families");
    }
    first
}

// --- US5: geometry overrides (T079–T083) ------------------------------------
//
// Geometry is the one part of the appearance the compositor can see for itself, so these tests
// need none of the paint tape above: `hyprctl layers` reports the surface, and the claim is that
// it is the surface `ui::layout` asks for under the resolved geometry — the same arithmetic the
// daemon used, driven from the same configuration text (research.md R22).

/// A raised text height, a raised row padding and a raised cap: the three settings US5-AS1 names,
/// each well clear of its default so the resulting overlay cannot be mistaken for the default one.
const BIGGER: &str = "[style]\n\
                      text_line_height = 40\n\
                      row_padding = 16\n\
                      width_fraction = 0.95\n\
                      height_fraction = 0.95\n";

/// What one opening under one configuration produced: the surface the compositor reported, beside
/// the metrics `ui::layout` asks for on the monitor it was shown on.
struct Measured {
    monitor: (u32, u32),
    scale: f32,
    entries: usize,
    expected: layout::Metrics,
    /// `(x, y, width, height)`, logical pixels, as `hyprctl layers` reports them.
    surface: (i32, i32, u32, u32),
    stderr: String,
    running: bool,
}

impl Measured {
    /// The surface's size alone, which is what every claim below is about.
    fn size(&self) -> (u32, u32) {
        (self.surface.2, self.surface.3)
    }

    /// The monitor as the compositor lays out on it — the units the cap is a fraction of.
    fn logical_monitor(&self) -> (u32, u32) {
        let scale = f64::from(self.scale);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (
            (f64::from(self.monitor.0) / scale).round() as u32,
            (f64::from(self.monitor.1) / scale).round() as u32,
        )
    }
}

/// Stage the documented scenario under `app_config`, open the overlay once, and measure it.
///
/// `mode` pins the panel to something other than the documented 1920×1080 at scale 1, which is how
/// the scaled case below gets a scaled monitor to run on.
///
/// The expectation is computed from the configuration text through the *application's own*
/// `config::parse`, not from numbers copied into the test — so a key that never reaches the
/// geometry produces a mismatch here rather than an expectation quietly rewritten to match.
///
/// # Panics
/// If the panel never reports the pinned mode, or the overlay never maps.
fn measured_with(app_config: &str, mode: Option<&str>) -> Measured {
    let nested = Nested::start_with(&Setup::documented().with_app_config(app_config));
    let panel = match mode {
        None => pinned_panel(&nested),
        Some(mode) => {
            let panel = nested.add_headless_output();
            nested.hyprctl(&["keyword", "monitor", &format!("{panel},{mode}")]);
            nested.dispatch(&format!("focusmonitor {panel}"));
            nested.wait_until("the panel is focused at the pinned mode", || {
                nested
                    .monitors()
                    .iter()
                    .any(|monitor| monitor.name == panel && monitor.focused)
            });
            panel
        }
    };
    let _windows = stage_scenario(&nested, &panel);

    let monitor = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.name == panel)
        .expect("the pinned panel");

    let (configuration, _) = config::parse(app_config);
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    let entries = ordering::entries(&world, configuration.order).0.len();
    let metrics = match configuration.presentation {
        Presentation::List => layout::list_metrics,
        Presentation::Grid => layout::grid_metrics,
    };
    let expected = metrics(
        &configuration.style.geometry,
        monitor.size,
        monitor.scale,
        entries,
    );

    let mut daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = measure(&nested, &mut keyboard);
    let running = daemon.is_running();

    Measured {
        monitor: monitor.size,
        scale: monitor.scale,
        entries,
        expected,
        surface,
        stderr: daemon.stderr(),
        running,
    }
}

/// The documented panel, which is where every case but the scaled one runs.
fn measured(app_config: &str) -> Measured {
    measured_with(app_config, None)
}

/// US5-AS1, FR-047, FR-055: raising the text height, the row padding and the size cap makes the
/// overlay larger, and larger by exactly the amount the settings ask for.
///
/// Asserted in both directions — bigger than the default overlay, and equal to what `ui::layout`
/// computes from the same file — because either alone is weak: "bigger" would pass on an overlay
/// that grew by the wrong amount, and "equal" would pass on a setting that changed nothing if the
/// expectation were computed from a geometry that had not been overridden either.
#[test]
fn e2e_geometry_override_resizes() {
    let default = measured("");
    let bigger = measured(BIGGER);

    assert_eq!(
        default.size(),
        default.expected.surface_size(),
        "the unconfigured overlay is not the one ui::layout asks for"
    );
    assert_eq!(
        bigger.size(),
        bigger.expected.surface_size(),
        "the overridden overlay is not the one ui::layout asks for; stderr was:\n{}",
        bigger.stderr
    );

    assert!(
        bigger.size().0 > default.size().0 && bigger.size().1 > default.size().1,
        "the overrides did not enlarge the overlay: {:?} against {:?}",
        bigger.size(),
        default.size()
    );
    // The rows themselves grew, not merely the box around them (FR-047, FR-052).
    assert!(
        bigger.expected.row_height > default.expected.row_height
            && bigger.expected.text_height > default.expected.text_height
            && bigger.expected.icon_slot() > default.expected.icon_slot(),
        "the entries did not grow with the text height"
    );
    assert!(
        bigger.running,
        "the daemon stopped under a geometry override:\n{}",
        bigger.stderr
    );
}

/// US5-AS2, FR-053, SC-023: with twenty workspaces and geometry raised well past what fits, the
/// three layout guarantees still hold — the overlay stays inside its cap, it scrolls rather than
/// shrinking, and the entries are the size one entry would be.
#[test]
fn e2e_geometry_override_still_caps_and_scrolls() {
    // A small panel and a large entry, so the cap is reached with certainty rather than by
    // hoping twenty rows overflow a full-size monitor.
    let config = "[style]\ntext_line_height = 48\nrow_padding = 20\n";
    let nested = Nested::start_with(&Setup::documented().with_app_config(config));
    let small = nested.add_headless_output();
    nested.hyprctl(&["keyword", "monitor", &format!("{small},800x600@60,auto,1")]);
    nested.dispatch(&format!("focusmonitor {small}"));
    nested.wait_until("the small output is focused at 800x600", || {
        nested
            .monitors()
            .iter()
            .any(|monitor| monitor.name == small && monitor.focused && monitor.size == (800, 600))
    });

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

    let monitor = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.name == small)
        .expect("the small panel");
    let (configuration, diagnostics) = config::parse(config);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let geometry = &configuration.style.geometry;

    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    let listed = ordering::entries(&world, configuration.order).0.len();
    let expected = layout::list_metrics(geometry, monitor.size, monitor.scale, listed);
    let one = layout::list_metrics(geometry, monitor.size, monitor.scale, 1);
    assert!(
        expected.scrolls(listed),
        "{listed} entries at this geometry must exceed the cap on {:?}",
        monitor.size
    );

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    // Opened by hand rather than through `measure`, which closes the overlay again: the scrolling
    // half of this test needs it still up and the modifier still held.
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());
    let surface = nested.overlay_xywh().expect("the overlay surface");
    let size = (surface.2, surface.3);

    assert_eq!(
        size,
        expected.surface_size(),
        "the mapped surface is the one ui::layout asked for"
    );
    // Still capped (FR-053), asserted against the compositor's own report of the monitor rather
    // than against the computation that produced the surface.
    // Both fractions are inside `0.1..=1.0` and the monitor is 800x600, so the products are far
    // inside `u32` and the casts are exact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cap = (
        (f64::from(monitor.size.0) * geometry.width_fraction).round() as u32,
        (f64::from(monitor.size.1) * geometry.height_fraction).round() as u32,
    );
    assert!(
        size.0 <= cap.0 && size.1 <= cap.1,
        "the overlay is {size:?} against a cap of {cap:?} on {:?}",
        monitor.size
    );
    // Still full size (FR-019): twenty entries are drawn at the height one entry would be, and
    // the surface holds a whole number of them.
    assert_eq!(
        expected.row_height, one.row_height,
        "twenty entries are not the height of one"
    );
    assert!(
        expected.row_height
            > layout::list_metrics(&Geometry::DEFAULT, monitor.size, monitor.scale, listed)
                .row_height,
        "the override did not actually raise the row, so the claim is vacuous"
    );
    assert_eq!(
        u32::try_from(expected.visible_rows).unwrap(),
        (expected.height - expected.padding * 2) / expected.row_height,
        "the surface holds whole rows of the fixed entry height, not shrunken ones"
    );

    // And still scrolls to keep the highlight in view: an entry past the bottom of the viewport
    // is still reachable and still commits (FR-053).
    let entries = ordering::entries(&world, configuration.order).0;
    let taps = expected.visible_rows + 2;
    let target = entries[1 + taps].workspace_id;
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

/// US5-AS3, FR-047, FR-015a: overriding the cell size and the gap resizes the grid's cells, and
/// each window rectangle inside them still keeps the relative position and proportion of the real
/// layout it was mapped from.
#[test]
fn e2e_grid_geometry_override() {
    let config =
        format!("{GRID}[style]\ngrid_cell_width = 320\ngrid_cell_height = 180\ngrid_gap = 24\n");
    let nested = Nested::start_with(&Setup::documented().with_app_config(&config));
    let panel = pinned_panel(&nested);
    let workspace = nested.active_workspace();
    let _one = clients::spawn(&nested, "grid-window-1");
    let _two = clients::spawn(&nested, "grid-window-2");
    let _three = clients::spawn(&nested, "grid-window-3");
    nested.wait_until("the three windows share the panel's workspace", || {
        clients::titles_on(&nested, workspace).len() == 3
    });

    let monitor = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.name == panel)
        .expect("the pinned panel");
    let (configuration, diagnostics) = config::parse(&config);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let geometry = &configuration.style.geometry;

    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    let entries = ordering::entries(&world, configuration.order).0;
    let expected = layout::grid_metrics(geometry, monitor.size, monitor.scale, entries.len());
    let defaults = layout::grid_metrics(
        &Geometry::DEFAULT,
        monitor.size,
        monitor.scale,
        entries.len(),
    );

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = measure(&nested, &mut keyboard);

    assert_eq!(
        (surface.2, surface.3),
        expected.surface_size(),
        "the mapped surface is the grid ui::layout asked for"
    );
    assert_ne!(
        (surface.2, surface.3),
        defaults.surface_size(),
        "the cell overrides changed nothing, so nothing below is being tested"
    );
    assert!(
        expected.cell_width > defaults.cell_width
            && expected.miniature_height > defaults.miniature_height
            && expected.gap > defaults.gap,
        "the cells and the gap did not grow"
    );

    // FR-015a under the override: every window keeps the fraction of the miniature its window
    // covers of the monitor. The rectangles are the ones `ui::layout` maps, which is exactly what
    // the renderer paints (`e2e_grid_miniature_layout` makes the same claim at the defaults).
    let entry = entries
        .iter()
        .find(|entry| entry.workspace_id == workspace)
        .expect("the workspace is listed");
    let area = expected.miniature_box(0, entry.monitor_size);
    let mut drawn = 0;
    for window in &entry.windows {
        let Some(rect) = layout::miniature_rect(
            window.at,
            window.size,
            entry.monitor_position,
            entry.monitor_size,
            area,
        ) else {
            continue;
        };
        drawn += 1;
        let width = f64::from(window.size.0) / f64::from(entry.monitor_size.0) * area.2;
        let height = f64::from(window.size.1) / f64::from(entry.monitor_size.1) * area.3;
        assert!(
            (rect.2 - width).abs() < 0.01 && (rect.3 - height).abs() < 0.01,
            "{:?}/{:?} became {rect:?}, expected {width}×{height}",
            window.at,
            window.size
        );
        // Position, not only proportion: the rectangle sits inside the miniature it belongs to.
        assert!(
            rect.0 >= area.0 - 0.01
                && rect.1 >= area.1 - 0.01
                && rect.0 + rect.2 <= area.0 + area.2 + 0.01
                && rect.1 + rect.3 <= area.1 + area.3 + 0.01,
            "{rect:?} escapes the miniature {area:?}"
        );
    }
    assert_eq!(drawn, 3, "one rectangle per window");
}

/// US5-AS4, FR-054, FR-059: a cell width of 0 and a cap of 5.0 are each brought within range
/// rather than rejected, each reported once by name, and the overlay is usable — with every other
/// setting in the same file still applied.
#[test]
fn e2e_out_of_range_geometry_clamped() {
    let config = format!(
        "{GRID}[style]\ngrid_cell_width = 0\nheight_fraction = 5.0\ngrid_cell_height = 180\n"
    );
    let measured = measured(&config);

    // Clamped to the documented bounds, not to the defaults — a value below the minimum means
    // "as small as allowed", which is a different answer from "never mind" (research.md R26).
    let bounds = |key: &str| {
        theme::GEOMETRY
            .iter()
            .find(|setting| setting.key == key)
            .unwrap_or_else(|| panic!("{key} is a geometry setting"))
    };
    let (configuration, diagnostics) = config::parse(&config);
    let geometry = configuration.style.geometry;
    assert!(
        (f64::from(geometry.grid_cell_width) - bounds("grid_cell_width").min).abs() < f64::EPSILON,
        "FR-054: below the minimum is raised to it, got {}",
        geometry.grid_cell_width
    );
    assert!(
        (geometry.height_fraction - bounds("height_fraction").max).abs() < f64::EPSILON,
        "FR-054: above the maximum is lowered to it"
    );
    assert_eq!(
        geometry.grid_cell_height, 180,
        "the in-range setting beside them still applied"
    );

    // Reported once each, by name, saying what was wrong and what was used (FR-059).
    let reported: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.subject.as_str())
        .collect();
    assert_eq!(
        reported,
        vec![
            "config.style.grid_cell_width",
            "config.style.height_fraction"
        ],
        "{diagnostics:?}"
    );
    for key in ["grid_cell_width", "height_fraction"] {
        let lines: Vec<&str> = measured
            .stderr
            .lines()
            .filter(|line| line.contains(&format!("config.style.{key}")))
            .collect();
        assert_eq!(lines.len(), 1, "{key}: {:?}", measured.stderr);
        assert!(
            lines[0].starts_with(&format!("WARN  config.style.{key}:"))
                && lines[0].contains("using"),
            "the report does not say what was used: {}",
            lines[0]
        );
    }

    // And the overlay is usable: it mapped, it is the size the clamped geometry asks for, it fits
    // the monitor, and the daemon is still running (SC-022, SC-023).
    assert_eq!(
        measured.size(),
        measured.expected.surface_size(),
        "the clamped geometry did not produce the overlay ui::layout asks for"
    );
    let logical = measured.logical_monitor();
    assert!(
        measured.size().0 <= logical.0 && measured.size().1 <= logical.1,
        "a cap of 5.0 escaped the monitor: {:?} on {logical:?}",
        measured.size()
    );
    assert!(
        measured.running,
        "the daemon stopped over an out-of-range value:\n{}",
        measured.stderr
    );
}

/// US5-AS5, FR-055: the same overrides on a scale-2 output scale exactly as the defaults do — the
/// logical desktop is what the overlay is sized against, so the same file gives the same surface.
#[test]
fn e2e_geometry_scales_with_monitor() {
    // The same logical desktop twice: 1920×1080 at scale 1, and a 3840×2160 panel at scale 2,
    // which the compositor also lays 1920×1080 out on.
    let unscaled = measured(BIGGER);
    let scaled = measured_with(BIGGER, Some("3840x2160@60,auto,2"));

    assert!(
        (scaled.scale - 2.0).abs() < 0.01 && scaled.monitor == (3840, 2160),
        "the scaled panel is {:?} at {}",
        scaled.monitor,
        scaled.scale
    );
    assert_eq!(
        scaled.logical_monitor(),
        unscaled.logical_monitor(),
        "the two panels must lay out the same logical desktop"
    );
    assert_eq!(
        scaled.entries, unscaled.entries,
        "the two runs must show the same number of entries"
    );

    assert_eq!(
        scaled.size(),
        unscaled.size(),
        "the same overrides gave a different surface at scale 2; stderr was:\n{}",
        scaled.stderr
    );
    assert_eq!(
        scaled.size(),
        scaled.expected.surface_size(),
        "the scaled overlay is not the one ui::layout asks for"
    );
    // The buffer behind it doubled even though the surface did not — which is what "scaled per
    // monitor the same way the defaults are" means in the two unit systems (FR-055).
    assert_eq!(
        scaled.expected.buffer_size(),
        (unscaled.expected.width * 2, unscaled.expected.height * 2),
        "the overridden geometry was not scaled into device pixels"
    );
    assert_eq!(
        scaled.expected.row_height,
        unscaled.expected.row_height * 2,
        "the overridden row height was not scaled"
    );
}
