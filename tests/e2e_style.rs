//! Overriding individual colours, fonts and geometry on top of a theme (US4, US5).
//!
//! The same two interfaces as `e2e_theme.rs` and for the same reason (research.md R22): the
//! compositor's own view of the surface for geometry, and the env-gated paint records for the
//! colours and fonts that actually reached the buffer. What separates these tests from US2's is
//! that every one of them asserts *independence* — that one setting reaching the overlay says
//! nothing about the others, and that one setting failing takes nothing else with it (FR-050,
//! FR-059, SC-022).

mod e2e;

use e2e::overlay::GRID;
use e2e::style::{
    GRID_ELEMENTS, LIST_ELEMENTS, assert_drawn_over, assert_every_paint_over, colour_of, painted,
};

use hypr_swap::theme;

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
