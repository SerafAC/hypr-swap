//! Records the pre-feature visual baseline that feature 002's "unchanged from before" assertions
//! compare against (FR-049a, SC-018, SC-019).
//!
//! **This recorder is a one-shot.** Feature 002's foundational phase replaces every colour in
//! `ui::render` and every geometry constant in `ui::layout` with resolved style values; once that
//! lands, the renderer this file measures no longer exists and the numbers below can never be
//! reproduced. They are therefore captured *before* the refactor and committed as fixtures under
//! `tests/fixtures/baseline/`, which is what later tests read.
//!
//! Two things are recorded, in the two forms the research decisions allow:
//!
//! 1. **Geometry**, from the compositor's own `hyprctl layers` view of the overlay surface, plus
//!    the `ui::layout` metrics the pre-feature build computed to ask for it (research.md R22 —
//!    screenshot comparison stays rejected, per feature 001's R14).
//! 2. **Style**, the eleven colours and the drawing constants the pre-feature renderer used. Those
//!    are private `const`s in `ui::render`, so they are transcribed rather than read; the
//!    transcription is checked against the source by `style_baseline_matches_the_source` below,
//!    which fails if anyone edits a constant without updating the fixture.
//!
//! The recorder is `#[ignore]`d: it writes fixtures rather than asserting anything, and normal
//! runs must not overwrite a committed baseline. Run it deliberately:
//!
//! ```text
//! cargo test --test e2e_baseline -- --ignored record_pre_feature_baseline
//! ```

mod e2e;

use std::path::{Path, PathBuf};
use std::time::Duration;

use e2e::clients;
use e2e::harness::{Nested, OverlaySurface, Setup};
use e2e::keyboard::{KEY_ESC, KEY_LEFTALT, KEY_TAB, Keyboard};

use hypr_swap::config::Order;
use hypr_swap::model::Monitor;
use hypr_swap::ordering;
use hypr_swap::state::World;
use hypr_swap::theme::Geometry;
use hypr_swap::ui::layout;

/// The scenario is pinned to a fixed panel so the baseline is a property of the code rather than
/// of the developer's screen: the recorded numbers must be reproducible on any machine.
const PANEL_MODE: &str = "1920x1080@60,auto,1";

/// The application configuration that selects the grid presentation (FR-016).
const GRID: &str = "presentation = \"grid\"\n";

const SETTLE: Duration = Duration::from_millis(200);

/// Where the committed fixtures live.
fn baseline_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("baseline")
}

// --- The style baseline -----------------------------------------------------

/// The eleven palette colours the pre-feature renderer drew with, as `(name, r, g, b, a)`.
///
/// These are `ui::render`'s private constants. Keeping the list here rather than making the
/// constants public avoids widening the renderer's surface for a test, and
/// [`style_baseline_matches_the_source`] guarantees the copy cannot drift from the original while
/// the original still exists.
const PALETTE: &[(&str, f64, f64, f64, f64)] = &[
    ("backdrop", 0.09, 0.09, 0.11, 0.93),
    ("highlight", 0.20, 0.42, 0.72, 1.0),
    ("active_mark", 0.42, 0.72, 0.45, 1.0),
    ("text", 0.92, 0.92, 0.94, 1.0),
    ("text_highlighted", 1.0, 1.0, 1.0, 1.0),
    ("text_dim", 0.66, 0.66, 0.70, 1.0),
    ("text_dim_highlighted", 0.86, 0.90, 0.96, 1.0),
    ("miniature", 0.16, 0.16, 0.19, 1.0),
    ("window", 0.30, 0.32, 0.38, 1.0),
    ("window_edge", 0.52, 0.55, 0.62, 1.0),
    ("window_floating", 0.38, 0.40, 0.48, 1.0),
];

/// The renderer's non-colour drawing constants, as `(name, value)`.
const RENDER_SCALARS: &[(&str, f64)] = &[
    ("corner_radius", 0.28),
    ("mark_width", 0.12),
    ("text_size", 0.78),
    ("miniature_font_fraction", 0.42),
    ("miniature_min_text_height", 9.0),
    ("miniature_edge", 0.008),
];

/// The font family the pre-feature renderer asked pango for.
const FONT_FAMILY: &str = "Sans";

/// One colour as the `#rrggbbaa` form feature 002's configuration accepts (research.md R25).
///
/// Rounding is half-away-from-zero, matching `f64::round`. Two channels land exactly on `.5`
/// (`0.30` → 76.5 and `0.70` → 178.5), so a half-to-even convention would render them one step
/// lower. This is why the `dark` theme must be built from the float constants rather than by
/// re-parsing the hex below: the floats have no tie to break (FR-049a).
fn hex(r: f64, g: f64, b: f64, a: f64) -> String {
    // Clamped to `0.0..=1.0` and then rounded, so the value is an integer in `0..=255` and the
    // cast is exact — the same allowance `ui::render` carries where it hands cairo device pixels.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(r),
        channel(g),
        channel(b),
        channel(a)
    )
}

fn style_baseline() -> serde_json::Value {
    let palette: serde_json::Map<String, serde_json::Value> = PALETTE
        .iter()
        .map(|&(name, r, g, b, a)| {
            (
                name.to_owned(),
                serde_json::json!({ "rgba": [r, g, b, a], "hex": hex(r, g, b, a) }),
            )
        })
        .collect();
    let scalars: serde_json::Map<String, serde_json::Value> = RENDER_SCALARS
        .iter()
        .map(|&(name, value)| (name.to_owned(), serde_json::json!(value)))
        .collect();

    serde_json::json!({
        "note": "The values ui/render.rs and ui/layout.rs used before feature 002 turned them \
                 into resolved style values. The `dark` theme and the geometry defaults must \
                 reproduce these exactly (FR-049a, SC-018). `rgba` is authoritative; `hex` is the \
                 8-bit round-trip, rounded half away from zero.",
        "font_family": FONT_FAMILY,
        "palette": palette,
        "render_scalars": scalars,
        // Transcribed, like the palette above: these were `pub const`s in `ui::layout` and are
        // now the fields of `theme::Geometry`. Reading them from `theme::Geometry::DEFAULT` would
        // make the recorder describe whatever the defaults *are* rather than what they *were*,
        // which is the one thing a baseline must not do.
        "geometry": {
            "text_line_height": 20,
            "row_padding": 8,
            "overlay_padding": 12,
            "width_fraction": 0.8,
            "height_fraction": 0.8,
            "grid_cell_width": 240,
            "grid_cell_height": 135,
            "grid_gap": 12,
            "corner_radius": 0.28,
            "mark_width": 0.12,
        },
        "derived": {
            "note": "Not settings — derived from the values above (plan.md Complexity Tracking).",
            "grid_label_height": 28,
            "scroll_margin": layout::SCROLL_MARGIN,
        },
    })
}

// Where the guard on this transcription used to be.
//
// While the pre-feature renderer existed, `style_baseline_matches_the_source` re-read its private
// constants out of `src/ui/render.rs` and failed if an edit had made the fixture stale. Feature
// 002's foundational phase replaced those constants with resolved style values, so there is no
// longer a source to check against and the test was deleted along with them — as its own comment
// said it would be.
//
// The guard has a successor, in the place that now owns the numbers: `theme.rs`'s
// `the_dark_theme_is_byte_for_byte_the_pre_feature_palette` and
// `the_default_geometry_is_byte_for_byte_the_pre_feature_geometry` read
// `tests/fixtures/baseline/style.json` and fail if a default drifts from it (FR-049a, SC-018).
// Those run under `cargo test --lib`, with no compositor needed.

// --- The geometry baseline --------------------------------------------------

/// The entries the overlay is built from, derived exactly as the application derives them.
fn entries(nested: &Nested) -> Vec<ordering::Entry> {
    let mut world = World::default();
    world.rebuild(nested.monitors(), nested.workspaces(), nested.clients());
    ordering::entries(&world, Order::Mru).0
}

fn focused_monitor(nested: &Nested) -> Monitor {
    nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .expect("a focused monitor")
}

/// Pin the focused output to a fixed mode and scale, so the recorded numbers do not depend on the
/// machine that recorded them.
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

/// The scenario both presentations are recorded against: four workspaces, one of them crowded
/// enough that its row has to ellipsise. The crowded row is what feature 002's FR-036a assertion
/// ("names truncate sooner once icons take slot space") compares against.
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

/// Open the overlay and leave the modifier held, so the surface can be measured.
fn open_overlay(nested: &Nested, keyboard: &mut Keyboard) -> OverlaySurface {
    keyboard.hold(KEY_LEFTALT);
    keyboard.tap_while_held(KEY_TAB);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    nested.wait_until("the overlay maps", || !nested.overlay_surfaces().is_empty());
    nested
        .overlay_surfaces()
        .into_iter()
        .next()
        .expect("the overlay surface")
}

fn close_overlay(nested: &Nested, keyboard: &mut Keyboard) {
    keyboard.tap_while_held(KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay unmaps", || {
        nested.overlay_surfaces().is_empty()
    });
}

fn metrics_json(metrics: &layout::Metrics) -> serde_json::Value {
    serde_json::json!({
        "width": metrics.width,
        "height": metrics.height,
        "logical_width": metrics.logical_width,
        "logical_height": metrics.logical_height,
        "scale": metrics.scale,
        "row_height": metrics.row_height,
        "text_height": metrics.text_height,
        "padding": metrics.padding,
        "visible_rows": metrics.visible_rows,
        "columns": metrics.columns,
        "cell_width": metrics.cell_width,
        "miniature_height": metrics.miniature_height,
        "gap": metrics.gap,
        "visible_entries": metrics.visible_entries(),
    })
}

/// Record one presentation: stage the scenario, open the overlay, and write what the compositor
/// and `ui::layout` both say about it.
fn record(presentation: &str, app_config: Option<&str>) -> serde_json::Value {
    let setup = match app_config {
        Some(toml) => Setup::documented().with_app_config(toml),
        None => Setup::documented(),
    };
    let nested = Nested::start_with(&setup);
    let panel = pinned_panel(&nested);
    let _windows = stage_scenario(&nested, &panel);

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let monitor = focused_monitor(&nested);
    let listed = entries(&nested);
    let metrics = if presentation == "grid" {
        layout::grid_metrics(
            &Geometry::DEFAULT,
            monitor.size,
            monitor.scale,
            listed.len(),
        )
    } else {
        layout::list_metrics(
            &Geometry::DEFAULT,
            monitor.size,
            monitor.scale,
            listed.len(),
        )
    };

    let surface = open_overlay(&nested, &mut keyboard);
    close_overlay(&nested, &mut keyboard);

    let (cell_x, cell_y, cell_w, cell_h) = metrics.cell_rect(0);

    let mut record = serde_json::json!({
        "presentation": presentation,
        "scenario": {
            "monitor": {
                "size": [monitor.size.0, monitor.size.1],
                "scale": monitor.scale,
                "position": [monitor.position.0, monitor.position.1],
            },
            "entry_count": listed.len(),
            "entries": listed
                .iter()
                .map(|entry| serde_json::json!({
                    "label": entry.label,
                    "windows": entry
                        .windows
                        .iter()
                        .map(|window| window.label.clone())
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        },
        // `hyprctl layers` reports the surface in global layout coordinates, so where the
        // headless panel happened to be placed leaks into `x`/`y`. The comparable value is the
        // offset *within* its own monitor, which is what a later run reproduces.
        "surface": {
            "x": surface.position.0,
            "y": surface.position.1,
            "w": surface.size.0,
            "h": surface.size.1,
            "level": surface.level,
            "x_on_monitor": surface.position.0 - monitor.position.0,
            "y_on_monitor": surface.position.1 - monitor.position.1,
        },
        "metrics": metrics_json(&metrics),
        "first_cell": [cell_x, cell_y, cell_w, cell_h],
    });

    if presentation == "grid" {
        let entry = listed.first().expect("at least one entry");
        let (x, y, w, h) = metrics.label_rect(0);
        let (mx, my, mw, mh) = metrics.miniature_box(0, entry.monitor_size);
        record["first_label"] = serde_json::json!([x, y, w, h]);
        record["first_miniature"] = serde_json::json!([mx, my, mw, mh]);
    }

    record
}

fn write(name: &str, value: &serde_json::Value) {
    let path = baseline_dir().join(name);
    std::fs::create_dir_all(baseline_dir()).expect("create tests/fixtures/baseline");
    let mut text = serde_json::to_string_pretty(value).expect("serialise the baseline");
    text.push('\n');
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    eprintln!("recorded {}", path.display());
}

#[test]
#[ignore = "recorder: overwrites the committed baseline; run once, before feature 002's refactor"]
fn record_pre_feature_baseline() {
    write("style.json", &style_baseline());
    write("list.json", &record("list", None));
    write("grid.json", &record("grid", Some(GRID)));
}
