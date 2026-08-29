//! Staging one overlay under a given configuration and asserting on what it was drawn with.
//!
//! The appearance stories share one gesture — write a configuration, open the overlay once, and
//! read the two interfaces research.md R22 allows — so it is written here once rather than in
//! each of `e2e_theme.rs` (US2) and `e2e_style.rs` (US4/US5). Nothing here compares pixels.

use hypr_swap::diag::PAINT_RECORDS_VAR;
use hypr_swap::theme::{COLOURS, Theme};

use super::harness::{Nested, Setup};
use super::keyboard::Keyboard;
use super::overlay::{measure, paint_colours, paint_fonts, pinned_panel, stage_scenario};

/// The elements the flat list draws, by the colour setting each one uses. Every workspace has a
/// name, one of them is highlighted, one is active, and at least one unhighlighted workspace holds
/// windows — so the scenario reaches all six whichever entry the highlight lands on.
pub const LIST_ELEMENTS: &[&str] = &[
    "backdrop",
    "highlight",
    "active_mark",
    "text",
    "text_highlighted",
    "text_dim",
];

/// The same for the grid, which adds the miniature and the window rectangles inside it. Only
/// `window_floating` is absent: the baseline scenario has no floating window, and inventing one
/// here would change the scenario every other comparison is made against.
pub const GRID_ELEMENTS: &[&str] = &[
    "backdrop",
    "highlight",
    "active_mark",
    "miniature",
    "window",
    "window_edge",
    "text",
    "text_highlighted",
    "text_dim",
];

/// One paint's worth of evidence: the surface the compositor reported, and what the daemon says
/// it drew with.
pub struct Painted {
    pub surface: (i32, i32, u32, u32),
    /// One entry per paint pass — the overlay repaints several times over one opening.
    pub colours: Vec<Vec<String>>,
    /// The `(requested, resolved)` families of each paint pass, in the same order (FR-046).
    pub fonts: Vec<(Vec<String>, Vec<String>)>,
    /// Whether the daemon was still running once the overlay had been opened and closed — the
    /// half of "every other setting still applies" that is about the process rather than the
    /// pixels (FR-058, FR-059).
    pub running: bool,
    pub stderr: String,
}

/// Stage the baseline scenario under `app_config`, open the overlay once, and collect what was
/// painted.
///
/// # Panics
/// If the overlay never maps, never unmaps, or is not reported by `hyprctl layers`.
#[must_use]
pub fn painted(app_config: Option<&str>) -> Painted {
    let setup = match app_config {
        Some(toml) => Setup::documented().with_app_config(toml),
        None => Setup::documented(),
    };
    let nested = Nested::start_with(&setup);
    let panel = pinned_panel(&nested);
    let _windows = stage_scenario(&nested, &panel);

    let mut daemon = nested.start_daemon_with_env(&[], &[(PAINT_RECORDS_VAR, "1")]);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let surface = measure(&nested, &mut keyboard);
    let running = daemon.is_running();
    let stderr = daemon.stderr();
    Painted {
        surface,
        colours: paint_colours(&stderr),
        fonts: paint_fonts(&stderr),
        running,
        stderr,
    }
}

/// The `#rrggbbaa` value a theme paints one element in.
///
/// # Panics
/// If `key` is not one of the eleven colour settings.
#[must_use]
pub fn colour_of(theme: &Theme, key: &str) -> String {
    COLOURS
        .iter()
        .find(|setting| setting.key == key)
        .unwrap_or_else(|| panic!("{key} is not a colour setting"))
        .read(theme)
        .hex_rgba()
}

/// Assert that one paint drew in `theme` and in nothing else (FR-045, FR-048).
///
/// # Panics
/// If any of the three claims below fails.
pub fn assert_drawn_in(drawn: &[String], theme: &Theme, elements: &[&str], other: &Theme) {
    assert_drawn_over(drawn, theme, elements, other, &[]);
}

/// The same, with `overrides` — `(key, "#rrggbbaa")` — replacing the theme's own value for those
/// keys, which is what a `[style]` colour override is (FR-050).
///
/// Three claims, which together are US2-AS1's "every themed element uses that theme's values and
/// none is left with the default theme's appearance", and US4-AS1's "that one element uses the
/// override and every other element uses the named theme's value": every colour that reached
/// cairo is one the resolved appearance calls for, every element the presentation draws is among
/// them, and no value that distinguishes `other` from that appearance appears at all — which is
/// also what catches an override that never applied, when `other` is the theme it sits on.
///
/// # Panics
/// If any of those three claims fails.
pub fn assert_drawn_over(
    drawn: &[String],
    theme: &Theme,
    elements: &[&str],
    other: &Theme,
    overrides: &[(&str, &str)],
) {
    let expected = |key: &str| {
        overrides
            .iter()
            .find(|(overridden, _)| *overridden == key)
            .map_or_else(|| colour_of(theme, key), |(_, value)| (*value).to_owned())
    };

    let palette: Vec<String> = COLOURS
        .iter()
        .map(|setting| expected(setting.key))
        .collect();
    for colour in drawn {
        assert!(
            palette.contains(colour),
            "{colour} is not a colour of the {:?} theme as overridden; the paint drew {drawn:?}",
            theme.name
        );
    }
    for element in elements {
        let wanted = expected(element);
        assert!(
            drawn.contains(&wanted),
            "the {element} was not drawn in {wanted}; the paint drew {drawn:?}"
        );
    }
    for setting in COLOURS {
        let foreign = colour_of(other, setting.key);
        if foreign == expected(setting.key) {
            // The two agree here, so seeing this value proves nothing either way.
            continue;
        }
        assert!(
            !drawn.contains(&foreign),
            "the {} was left in {:?}'s {foreign}; the paint drew {drawn:?}",
            setting.key,
            other.name
        );
    }
}

/// Every paint of one opening, checked the same way — an appearance that reached only the first
/// pass would be one that flickers.
///
/// # Panics
/// If nothing was painted, or if any pass fails [`assert_drawn_in`].
pub fn assert_every_paint(painted: &Painted, theme: &Theme, elements: &[&str], other: &Theme) {
    assert_every_paint_over(painted, theme, elements, other, &[]);
}

/// The same with overrides applied on top of the theme.
///
/// # Panics
/// If nothing was painted, or if any pass fails [`assert_drawn_over`].
pub fn assert_every_paint_over(
    painted: &Painted,
    theme: &Theme,
    elements: &[&str],
    other: &Theme,
    overrides: &[(&str, &str)],
) {
    assert!(
        !painted.colours.is_empty(),
        "the gate produced no colour records at all; stderr was:\n{}",
        painted.stderr
    );
    for drawn in &painted.colours {
        assert_drawn_over(drawn, theme, elements, other, overrides);
    }
}
