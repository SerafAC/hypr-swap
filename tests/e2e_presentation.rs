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
use e2e::harness::{Nested, OVERLAY_LEVEL, OverlaySurface, Setup};
use e2e::keyboard::{KEY_LEFTALT, KEY_TAB, Keyboard};

use hypr_swap::config::Order;
use hypr_swap::ordering;
use hypr_swap::state::World;
use hypr_swap::ui::layout;

const SETTLE: Duration = Duration::from_millis(200);

/// The application configuration that selects the grid presentation (FR-016).
const GRID: &str = "presentation = \"grid\"\n";

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
        expected.surface_size(),
        "the mapped surface is the size ui::layout asked for"
    );

    // The properties FR-019 requires, asserted against the real surface rather than against the
    // computation that produced it. `hyprctl layers` reports logical pixels, and this monitor is
    // at scale 1, so its size is directly comparable; `e2e_overlay_scales_with_the_monitor`
    // covers the scaled case.
    assert!(
        surface.size.0 <= monitor.size.0 * 4 / 5 && surface.size.1 <= monitor.size.1 * 4 / 5,
        "the overlay stays inside 80 % of {:?}, got {:?}",
        monitor.size,
        surface.size
    );
    let rows = (expected.height - expected.padding * 2) / expected.row_height;
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

#[test]
fn e2e_overlay_scales_with_the_monitor() {
    // FR-019: on a monitor with a scale factor — a 4K panel on a 14" laptop, say — the overlay
    // has to sit in the compositor's layout exactly as every other window on that monitor does,
    // rather than `scale` times larger than all of them.
    //
    // This is the regression the two-unit-system split in `ui::layout` exists for. The buffer is
    // painted in device pixels, but `set_size` and `hyprctl layers` both speak logical ones, so
    // sizing the surface from the buffer made the overlay come out `scale` times too big.
    let nested = Nested::start();
    let _one = clients::spawn_on(&nested, 1, "alpha-window");
    let _two = clients::spawn_on(&nested, 2, "beta-window");

    // The same panel twice: once presented at scale 1, once at scale 2. The logical desktop is
    // 1920×1080 in the second case, so the overlay must be sized for that, not for 3840×2160.
    let panel = nested.add_headless_output();
    nested.hyprctl(&[
        "keyword",
        "monitor",
        &format!("{panel},3840x2160@60,auto,1"),
    ]);
    nested.dispatch(&format!("focusmonitor {panel}"));
    nested.wait_until("the panel is focused unscaled", || {
        nested.monitors().iter().any(|monitor| {
            monitor.name == panel && monitor.focused && (monitor.scale - 1.0).abs() < 0.01
        })
    });

    let (unscaled_monitor, unscaled_expected, unscaled) = measure_overlay(&nested);
    assert_eq!(
        unscaled.size,
        unscaled_expected.surface_size(),
        "the unscaled reference"
    );

    nested.hyprctl(&[
        "keyword",
        "monitor",
        &format!("{panel},3840x2160@60,auto,2"),
    ]);
    nested.wait_until("the same panel is now at scale 2", || {
        nested.monitors().iter().any(|monitor| {
            monitor.name == panel && monitor.focused && (monitor.scale - 2.0).abs() < 0.01
        })
    });

    let (scaled_monitor, scaled_expected, scaled) = measure_overlay(&nested);
    assert_eq!(
        scaled_monitor.size, unscaled_monitor.size,
        "the panel's own resolution did not change; only how it is presented did"
    );

    assert_eq!(
        scaled.size,
        scaled_expected.surface_size(),
        "the mapped surface is the logical size ui::layout asked for"
    );
    // The bug in one assertion. Scaling a 3840×2160 panel by 2 makes the compositor lay out a
    // 1920×1080 desktop on it, and the overlay has to be indistinguishable from one opened on a
    // real 1920×1080 monitor — which is where the pre-fix build put a 3072-wide surface.
    let logical_monitor = (scaled_monitor.size.0 / 2, scaled_monitor.size.1 / 2);
    assert_eq!(
        scaled.size,
        layout::list_metrics(logical_monitor, 1.0, entries(&nested, Order::Mru).len())
            .surface_size(),
        "a 4K panel at scale 2 must present the overlay exactly as a {logical_monitor:?} monitor does"
    );
    assert!(
        scaled.size.0 <= logical_monitor.0 * 4 / 5 && scaled.size.1 <= logical_monitor.1 * 4 / 5,
        "the overlay stays inside FR-019's cap on the {logical_monitor:?} logical desktop, got {:?}",
        scaled.size
    );

    // The two halves of that, spelled out. The width tracks the desktop, so halving the logical
    // desktop halves it...
    assert_eq!(
        scaled.size.0,
        unscaled.size.0 / 2,
        "the width is a fraction of the logical desktop, which scale 2 halved"
    );
    // ...while the height is built from a fixed logical row size, so it does not move. That is
    // the point of scaling: a row stays 36 logical pixels and therefore doubles in device pixels,
    // ending up the same physical size as the scaled-up text in every other window.
    assert_eq!(
        scaled.size.1, unscaled.size.1,
        "rows keep their logical height, so the entries match the rest of the desktop"
    );
    assert_eq!(
        scaled_expected.height,
        unscaled_expected.height * 2,
        "and are therefore painted at twice the device resolution"
    );
    // The buffer is still the panel's real resolution, so nothing is upscaled into blur.
    assert_eq!(
        scaled_expected.buffer_size().0,
        unscaled_expected.width,
        "the same device pixels across the panel either way"
    );
}

/// Open the overlay against a freshly started daemon and report the focused monitor, the metrics
/// `ui::layout` derives for it, and the surface the compositor actually mapped.
///
/// The daemon is started — and stopped — inside this function on purpose. It caches the
/// compositor's monitors and refreshes them from the event socket, and Hyprland emits no event
/// for a monitor's scale changing under `hyprctl keyword`, so a daemon that outlived the change
/// would size the overlay from the scale it saw at start-up. Restarting is how the scenario
/// measures the two configurations rather than one configuration twice.
fn measure_overlay(
    nested: &Nested,
) -> (hypr_swap::model::Monitor, layout::Metrics, OverlaySurface) {
    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let monitor = nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .expect("a focused monitor");
    let expected = layout::list_metrics(
        monitor.size,
        monitor.scale,
        entries(nested, Order::Mru).len(),
    );
    let surface = open_overlay(nested, &mut keyboard);

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the overlay unmaps", || {
        nested.overlay_surfaces().is_empty()
    });

    (monitor, expected, surface)
}

// --- User story 3: the grid presentation ------------------------------------

/// The focused monitor, as the compositor reports it.
fn focused_monitor(nested: &Nested) -> hypr_swap::model::Monitor {
    nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .expect("a focused monitor")
}

/// The entry for one workspace, as the overlay builds it.
fn entry_for(nested: &Nested, workspace: i32) -> ordering::Entry {
    entries(nested, Order::Mru)
        .into_iter()
        .find(|entry| entry.workspace_id == workspace)
        .unwrap_or_else(|| panic!("workspace {workspace} is listed"))
}

/// One window's layout rectangle as the compositor reports it: `(position, size)`.
type WindowGeometry = ((i32, i32), (u32, u32));
/// One miniature rectangle: `(x, y, width, height)`, device pixels.
type Rect = (f64, f64, f64, f64);

/// The rectangles the grid draws for an entry's windows, in the order it draws them — tiled
/// first, floating on top (FR-015a).
///
/// Every miniature is mapped into the *same* cell, slot 0, so two entries can be compared for
/// their arrangement rather than for where they happen to sit in the overlay.
fn miniatures(metrics: &layout::Metrics, entry: &ordering::Entry) -> Vec<Rect> {
    let area = metrics.miniature_box(0, entry.monitor_size);
    let tiled = entry.windows.iter().filter(|window| !window.floating);
    let floating = entry.windows.iter().filter(|window| window.floating);
    tiled
        .chain(floating)
        .filter_map(|window| {
            layout::miniature_rect(
                window.at,
                window.size,
                entry.monitor_position,
                entry.monitor_size,
                area,
            )
        })
        .collect()
}

/// Geometry is fractional; comparing it exactly would assert the rounding, not the mapping.
fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 0.01
}

/// Assert that the three-window arrangement US3-AS2 describes survives the mapping into a
/// miniature (FR-015a, SC-008).
///
/// Which window landed where is the compositor's decision, not the order they were spawned in, so
/// the three roles are identified from the geometry it reported: the window spanning the full
/// height, and the pair stacked beside it. Asserting that arrangement first means a layout change
/// in Hyprland fails with a clear message rather than somewhere further down.
fn assert_arrangement_preserved(real: &[WindowGeometry], drawn: &[Rect]) {
    let mut mapped: Vec<(WindowGeometry, Rect)> =
        real.iter().copied().zip(drawn.iter().copied()).collect();
    mapped.sort_by_key(|((_, size), _)| std::cmp::Reverse(size.1));
    let (full_height, full_height_rect) = mapped[0];
    let mut stacked = [mapped[1], mapped[2]];
    stacked.sort_by_key(|((at, _), _)| at.1);
    let (upper, upper_rect) = stacked[0];
    let (lower, lower_rect) = stacked[1];

    assert!(
        full_height.1.1 > upper.1.1 && full_height.1.1 > lower.1.1,
        "one window spans the height of the two beside it: {real:?}"
    );
    assert_eq!(
        upper.0.0, lower.0.0,
        "the other two share a column: {upper:?} against {lower:?}"
    );
    assert_ne!(
        full_height.0.0, upper.0.0,
        "which is not the first window's column: {real:?}"
    );
    assert!(
        lower.0.1 > upper.0.1,
        "and one of the pair sits below the other: {upper:?} against {lower:?}"
    );

    // The same three relationships, now in the miniature.
    assert_eq!(
        full_height.0.0 < upper.0.0,
        full_height_rect.0 < upper_rect.0,
        "the miniature keeps the two columns in the same left-to-right order: \
         {full_height_rect:?} against {upper_rect:?}"
    );
    assert!(
        full_height_rect.0 + full_height_rect.2 <= upper_rect.0 + 0.01
            || upper_rect.0 + upper_rect.2 <= full_height_rect.0 + 0.01,
        "and side by side rather than overlapping: {full_height_rect:?} against {upper_rect:?}"
    );
    assert!(
        close(upper_rect.0, lower_rect.0) && close(upper_rect.2, lower_rect.2),
        "the stacked pair share a column: {upper_rect:?} against {lower_rect:?}"
    );
    assert!(
        lower_rect.1 > upper_rect.1,
        "and the third stays below the second: {upper_rect:?} against {lower_rect:?}"
    );
}

#[test]
fn e2e_grid_miniature_layout() {
    // FR-015, FR-015a, SC-008, US3-AS1/AS2: two windows side by side and a third below the second
    // become three rectangles in those same relative positions and proportions, each labelled,
    // with the workspace name underneath.
    //
    // The overlay's pixels are not asserted on (research.md R14). What is asserted is the surface
    // the compositor mapped — which is the grid's shape, not the list's — and the rectangles
    // `ui::layout` derives from the compositor's own live geometry, which is exactly what the
    // renderer paints.
    // On a landscape monitor Hyprland's default layout splits vertically first and then
    // horizontally, which is the arrangement US3-AS2 describes. The nested instance's own output
    // is a window on the developer's session and is taller than it is wide, so the scenario runs
    // on a headless output of a known, ordinary size instead.
    let nested = Nested::start_with(&Setup::documented().with_app_config(GRID));
    let wide = nested.add_headless_output();
    nested.hyprctl(&["keyword", "monitor", &format!("{wide},1920x1080@60,auto,1")]);
    nested.dispatch(&format!("focusmonitor {wide}"));
    nested.wait_until("the wide output is focused at 1920x1080", || {
        nested
            .monitors()
            .iter()
            .any(|monitor| monitor.name == wide && monitor.focused && monitor.size == (1920, 1080))
    });

    let workspace = nested.active_workspace();
    let _one = clients::spawn(&nested, "grid-window-1");
    let _two = clients::spawn(&nested, "grid-window-2");
    let _three = clients::spawn(&nested, "grid-window-3");
    nested.wait_until(
        "the three windows share the wide output's workspace",
        || clients::titles_on(&nested, workspace).len() == 3,
    );

    let monitor = focused_monitor(&nested);
    let listed = entries(&nested, Order::Mru);
    let expected = layout::grid_metrics(monitor.size, monitor.scale, listed.len());

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = open_overlay(&nested, &mut keyboard);

    assert_eq!(surface.level, OVERLAY_LEVEL);
    assert_eq!(
        surface.size,
        expected.surface_size(),
        "the mapped surface is the grid ui::layout asked for"
    );
    assert_ne!(
        surface.size,
        layout::list_metrics(monitor.size, monitor.scale, listed.len()).surface_size(),
        "and is distinguishable from the list, so the setting demonstrably took effect"
    );

    let entry = entry_for(&nested, workspace);
    let titles: Vec<&str> = entry
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect();
    assert_eq!(
        titles,
        vec!["grid-window-1", "grid-window-2", "grid-window-3"],
        "every window on the workspace is labelled in the miniature"
    );

    // SC-008: one rectangle per window, and `miniatures` yields them in the order the renderer
    // paints them, so each rectangle sits beside the window it was mapped from.
    let real: Vec<WindowGeometry> = entry
        .windows
        .iter()
        .map(|window| (window.at, window.size))
        .collect();
    let drawn = miniatures(&expected, &entry);
    assert_eq!(drawn.len(), 3, "one rectangle per window: {drawn:?}");
    assert_arrangement_preserved(&real, &drawn);

    // Proportion, not just position (FR-015a): each rectangle covers the same fraction of the
    // miniature that its window covers of the monitor.
    let area = expected.miniature_box(0, entry.monitor_size);
    for (rect, (at, size)) in drawn.iter().zip(&real) {
        let expected_width = f64::from(size.0) / f64::from(entry.monitor_size.0) * area.2;
        let expected_height = f64::from(size.1) / f64::from(entry.monitor_size.1) * area.3;
        assert!(
            close(rect.2, expected_width) && close(rect.3, expected_height),
            "{at:?}/{size:?} on {:?} became {rect:?}, expected {expected_width}×{expected_height}",
            entry.monitor_size
        );
    }

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
}

#[test]
fn e2e_grid_offscreen_workspace() {
    // FR-015a, US3-AS3, compositor-ipc assumption 1: a workspace that is not being displayed on
    // any monitor produces exactly the miniature the same layout produces on a visible one.
    //
    // Two workspaces are given the identical two-window arrangement; one is left on screen and
    // the other is switched away from. Their miniatures must be indistinguishable — which is the
    // whole reason FR-015a forbids screen capture, since a workspace that was never composited
    // has no pixels to copy (research.md R7).
    let nested = Nested::start_with(&Setup::documented().with_app_config(GRID));
    let _visible_one = clients::spawn_on(&nested, 1, "visible-first");
    let _visible_two = clients::spawn(&nested, "visible-second");
    let _hidden_one = clients::spawn_on(&nested, 2, "hidden-first");
    let _hidden_two = clients::spawn(&nested, "hidden-second");
    nested.dispatch("workspace 1");
    nested.wait_until("workspace 2 is no longer displayed anywhere", || {
        nested.active_workspace() == 1
            && nested
                .monitors()
                .iter()
                .all(|monitor| monitor.active_workspace != 2)
    });

    let monitor = focused_monitor(&nested);
    let metrics = layout::grid_metrics(
        monitor.size,
        monitor.scale,
        entries(&nested, Order::Mru).len(),
    );

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let _surface = open_overlay(&nested, &mut keyboard);

    let visible = entry_for(&nested, 1);
    let hidden = entry_for(&nested, 2);
    assert_eq!(
        hidden.windows.len(),
        2,
        "the compositor reports the off-screen workspace's windows at all"
    );
    for window in &hidden.windows {
        assert!(
            window.size.0 > 0 && window.size.1 > 0,
            "with real geometry, not a placeholder: {window:?}"
        );
    }

    let on_screen = miniatures(&metrics, &visible);
    let off_screen = miniatures(&metrics, &hidden);
    assert_eq!(
        off_screen.len(),
        on_screen.len(),
        "the same number of rectangles"
    );
    for (hidden_rect, visible_rect) in off_screen.iter().zip(&on_screen) {
        assert!(
            close(hidden_rect.0, visible_rect.0)
                && close(hidden_rect.1, visible_rect.1)
                && close(hidden_rect.2, visible_rect.2)
                && close(hidden_rect.3, visible_rect.3),
            "an off-screen workspace maps differently: {hidden_rect:?} against {visible_rect:?}"
        );
    }

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
}

#[test]
fn e2e_grid_empty_workspace() {
    // FR-007, US3-AS5: a workspace with no windows is listed as a marked empty miniature rather
    // than omitted. The compositor keeps workspace 5 alive with a persistent workspace rule,
    // since Hyprland destroys an ordinary workspace the moment its last window closes.
    let nested = Nested::start_with(
        &Setup::documented()
            .with_app_config(GRID)
            .with_compositor_config("workspace = 5, persistent:true\n"),
    );
    let _one = clients::spawn_on(&nested, 1, "alpha-window");
    let _two = clients::spawn_on(&nested, 2, "beta-window");
    nested.dispatch("workspace 1");
    nested.wait_until("the empty workspace 5 exists alongside the others", || {
        nested.active_workspace() == 1
            && nested
                .workspaces()
                .iter()
                .any(|workspace| workspace.id == 5 && workspace.window_count == 0)
    });

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let _surface = open_overlay(&nested, &mut keyboard);

    let empty = entry_for(&nested, 5);
    assert!(
        empty.windows.is_empty(),
        "the empty workspace carries no windows: {empty:?}"
    );
    assert_eq!(empty.label, "5", "and is still labelled with its name");

    // Listed *and* usable: an entry the user cannot reach would be omitted in all but name.
    let listed = entries(&nested, Order::Mru);
    let position = listed
        .iter()
        .position(|entry| entry.workspace_id == 5)
        .expect("the empty workspace has a place in the order");
    // The highlight opens on index 1 with an empty history, so this many taps land on it.
    let taps = (position + listed.len() - 1) % listed.len();
    for _ in 0..taps {
        keyboard.tap_while_held(KEY_TAB);
        std::thread::sleep(Duration::from_millis(30));
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the empty workspace can be selected from the grid", || {
        nested.active_workspace() == 5
    });
}

#[test]
fn e2e_title_truncation() {
    // FR-015b: a title too long for its rectangle is truncated with a visible indication rather
    // than overflowing or being omitted.
    //
    // Pango does the truncating, and its pixels are out of reach of a test (research.md R14).
    // What is observable is the two halves of "rather than": the title is *not* omitted — the
    // daemon receives it whole and the window still gets its rectangle — and it does *not*
    // overflow, because neither the overlay nor the rectangle inside it grows by a pixel to
    // accommodate it.
    const LONG: &str = "a window title long enough that no miniature could ever hold it in full, \
                        going on well past the point of any reasonable label and then further \
                        still for good measure";

    let nested = Nested::start_with(&Setup::documented().with_app_config(GRID));
    let _long = clients::spawn_on(&nested, 1, LONG);
    let _short = clients::spawn_on(&nested, 2, "b");
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });

    let monitor = focused_monitor(&nested);
    let listed = entries(&nested, Order::Mru);
    let expected = layout::grid_metrics(monitor.size, monitor.scale, listed.len());

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = open_overlay(&nested, &mut keyboard);

    let overlong = entry_for(&nested, 1);
    assert_eq!(
        overlong.windows.len(),
        1,
        "the window is present rather than dropped for being awkward"
    );
    assert_eq!(
        overlong.windows[0].label, LONG,
        "and reaches the renderer whole, which is what pango ellipsises"
    );

    assert_eq!(
        surface.size,
        expected.surface_size(),
        "the overlay is the documented size regardless of the title in it"
    );
    let short = entry_for(&nested, 2);
    let long_rect = miniatures(&expected, &overlong)[0];
    let short_rect = miniatures(&expected, &short)[0];
    assert!(
        close(long_rect.2, short_rect.2) && close(long_rect.3, short_rect.3),
        "and the rectangle holding it is the size its window earns, not the size its title wants: \
         {long_rect:?} against {short_rect:?}"
    );

    keyboard.tap_while_held(e2e::keyboard::KEY_ESC);
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    std::thread::sleep(SETTLE);
    assert_eq!(
        nested.active_workspace(),
        1,
        "and the session behaves normally throughout"
    );
}

#[test]
fn e2e_grid_commit_matches_list() {
    // FR-016, US3-AS4: the presentation changes what the overlay looks like and nothing else.
    // The identical gesture under each setting must reach the identical workspace, because the
    // session, the navigation and the commit path are one implementation shared by both.
    //
    // The two runs are sequential: the harness allows only one nested compositor at a time.
    let list = presentation_outcome(None);
    let grid = presentation_outcome(Some(GRID));

    assert_eq!(
        list.order, grid.order,
        "both presentations list the same entries in the same order"
    );
    assert_eq!(
        grid.activated, list.activated,
        "and the same gesture selects the same workspace in each"
    );
    assert_eq!(
        grid.activated, grid.expected,
        "which is the entry two taps past the opening highlight"
    );
    assert_ne!(
        grid.surface, list.surface,
        "while the overlay itself is visibly a different shape"
    );
}

/// What one run of the shared gesture produced.
struct Outcome {
    order: Vec<i32>,
    expected: i32,
    activated: i32,
    surface: (u32, u32),
}

/// Set up four workspaces, hold the switcher, tap twice, release — under whichever presentation
/// `app_config` selects.
fn presentation_outcome(app_config: Option<&str>) -> Outcome {
    const TAPS: usize = 2;

    let mut setup = Setup::documented();
    if let Some(toml) = app_config {
        setup = setup.with_app_config(toml);
    }
    let nested = Nested::start_with(&setup);
    let mut windows = Vec::new();
    for id in 1..=4 {
        windows.push(clients::spawn_on(&nested, id, &format!("window-{id}")));
    }
    nested.dispatch("workspace 1");
    nested.wait_until("four workspaces exist and the first is active", || {
        nested.active_workspace() == 1
            && nested
                .workspaces()
                .iter()
                .filter(|workspace| !workspace.is_special())
                .count()
                == 4
    });

    let listed = entries(&nested, Order::Mru);
    // The daemon starts with an empty history, so MRU is the compositor's order and the highlight
    // opens on index 1 — which makes the target of two taps known exactly (FR-008b).
    let expected = listed[1 + TAPS].workspace_id;

    let _daemon = nested.start_daemon();
    let mut keyboard = Keyboard::attach(&nested.wayland_display);
    let surface = open_overlay(&nested, &mut keyboard);

    for _ in 0..TAPS {
        keyboard.tap_while_held(KEY_TAB);
        std::thread::sleep(Duration::from_millis(30));
    }
    keyboard.release(KEY_LEFTALT);
    keyboard.settle();
    nested.wait_until("the selection is activated", || {
        nested.active_workspace() == expected
    });

    Outcome {
        order: listed.iter().map(|entry| entry.workspace_id).collect(),
        expected,
        activated: nested.active_workspace(),
        surface: surface.size,
    }
}

/// The focused monitor's pixel size — what a fullscreen window fills.
fn nested_size(nested: &Nested) -> (u32, u32) {
    nested
        .monitors()
        .into_iter()
        .find(|monitor| monitor.focused)
        .map_or((0, 0), |monitor| monitor.size)
}
