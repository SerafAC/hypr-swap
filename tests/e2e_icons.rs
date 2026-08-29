//! Program icons, end to end (FR-035–FR-044, US1).
//!
//! Every window in the overlay is drawn with the icon of the program that owns it, resolved ahead
//! of time and cached, with a placeholder whenever resolution fails. None of that is asserted by
//! comparing pixels — feature 001's R14 rejected screenshot comparison and research.md R22 replaces
//! it with two real interfaces: `hyprctl layers` for the surface geometry, and the env-gated paint
//! records for what the renderer actually resolved and drew.
//!
//! Every icon these tests resolve comes from the synthetic root in `e2e::fixtures`, so no
//! assertion depends on what the developer happens to have installed.

mod e2e;

use e2e::clients;
use e2e::fixtures::{self, Fixtures};
use e2e::harness::{Daemon, Nested, Setup};
use e2e::keyboard::Keyboard;
use e2e::overlay::{
    GRID, baseline, field, icons_of, measure, paint_records, pinned_panel, rects_of,
    stage_classified,
};

use hypr_swap::diag::PAINT_RECORDS_VAR;
use hypr_swap::icons::PLACEHOLDER_SOURCE;

/// The application configuration every test here starts from: the fixture icon set, so the
/// staged artwork is what gets resolved rather than whatever the machine has installed (FR-057).
fn config(extra: &str) -> String {
    format!("icon_set = \"{}\"\n{extra}", fixtures::SET)
}

/// One window to stage: the class it reports, the title it carries, and the workspace it opens on.
type Staged<'a> = (&'a str, &'a str, i32);

/// Everything one scenario produced.
struct Run {
    /// The daemon's complete diagnostic record (`contracts/diagnostics.md`).
    stderr: String,
    /// The overlay's `xywh` as `hyprctl layers` reported it while it was up.
    geometry: (i32, i32, u32, u32),
    /// Whether the daemon was still alive after the last opening — sampled before its stderr is
    /// read, because reading stops it (FR-024's "and keeps running").
    running: bool,
}

impl Run {
    /// The paint records, in the order the daemon emitted them.
    fn records(&self) -> Vec<String> {
        paint_records(&self.stderr)
    }

    /// The records of the *last* paint pass covering `entries` entries.
    ///
    /// An overlay repaints several times over one opening — a `configure` and its commits — so
    /// the records arrive as whole passes. The last one is the settled picture, which is what the
    /// visual requirements are about.
    fn last_pass(&self, entries: usize) -> Vec<String> {
        let records = self.records();
        assert!(
            records.len() >= entries,
            "fewer records than entries on screen: {records:?}\nstderr:\n{}",
            self.stderr
        );
        records[records.len() - entries..].to_vec()
    }

    /// Every diagnostic the daemon reported about an icon (FR-044).
    fn icon_diagnostics(&self) -> Vec<&str> {
        self.stderr
            .lines()
            .filter(|line| line.contains(" icon."))
            .collect()
    }
}

/// Stage a scenario, run the overlay `openings` times, and collect everything the daemon said.
///
/// `fixtures` is the synthetic `XDG_DATA_HOME` the daemon is pointed at; the caller owns it so a
/// test can stage an empty one (SC-016) or name a file inside it in an assertion.
fn run_with(
    fixtures: &Fixtures,
    app_config: &str,
    windows: &[Staged<'_>],
    openings: usize,
    extra_env: &[(&str, &str)],
) -> Run {
    run_shaped(
        fixtures,
        app_config,
        windows,
        openings,
        extra_env,
        |_, _| {},
    )
}

/// The same, with a chance to reshape the windows the compositor placed before the daemon starts.
///
/// The grid's shedding tests are about what happens at particular rectangle *sizes*, and a tiling
/// layout's sizes are the compositor's business rather than the test's — so those tests take the
/// windows the harness spawned and size them by hand here.
fn run_shaped(
    fixtures: &Fixtures,
    app_config: &str,
    windows: &[Staged<'_>],
    openings: usize,
    extra_env: &[(&str, &str)],
    shape: impl FnOnce(&Nested, &[clients::Client]),
) -> Run {
    let nested = Nested::start_with(&Setup::documented().with_app_config(app_config));
    let panel = pinned_panel(&nested);

    let mut staged = Vec::new();
    for (class, title, workspace) in windows {
        nested.dispatch(&format!("moveworkspacetomonitor {workspace} {panel}"));
        staged.push(clients::spawn_as_on(
            &nested,
            Some(class),
            *workspace,
            title,
        ));
    }
    nested.dispatch("workspace 1");
    nested.wait_until("the scenario starts on workspace 1", || {
        nested.active_workspace() == 1
    });
    shape(&nested, &staged);

    let daemon = start(&nested, fixtures, extra_env);
    let mut keyboard = Keyboard::attach(&nested.wayland_display);

    let mut geometry = (0, 0, 0, 0);
    for _ in 0..openings.max(1) {
        geometry = measure(&nested, &mut keyboard);
    }

    let mut daemon = daemon;
    let running = daemon.is_running();
    Run {
        stderr: daemon.stderr(),
        geometry,
        running,
    }
}

/// The daemon, started against the staged root with the paint-record gate open.
fn start(nested: &Nested, fixtures: &Fixtures, extra: &[(&str, &str)]) -> Daemon {
    let staged = fixtures.env();
    let mut environment: Vec<(&str, &str)> = staged
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    environment.push((PAINT_RECORDS_VAR, "1"));
    environment.extend_from_slice(extra);
    nested.start_daemon_with_env(&[], &environment)
}

/// The common case: the full fixture root, one opening, no extra environment.
fn run(app_config: &str, windows: &[Staged<'_>]) -> (Fixtures, Run) {
    let fixtures = Fixtures::stage();
    let outcome = run_with(&fixtures, app_config, windows, 1, &[]);
    (fixtures, outcome)
}

/// The scenario most tests here use: one workspace per fixture program, in a known order, so
/// entry *n* of the overlay is program *n*.
const ONE_EACH: &[Staged<'static>] = &[
    (fixtures::ALPHA_CLASS, "alpha-window", 1),
    (fixtures::BETA_CLASS, "beta-window", 2),
];

// --- T044: icons in the flat list (FR-035, FR-036, FR-040, SC-013) ----------

/// Each window's name is preceded by that program's own icon, and no window is drawn iconless
/// (US1-AS1, US1-AS3).
#[test]
fn e2e_icons_in_flat_list() {
    let (fixtures, run) = run(&config(""), ONE_EACH);
    let pass = run.last_pass(2);

    let alpha = fixtures
        .icon(fixtures::SET, "scalable/apps", "fixture-alpha.svg")
        .display()
        .to_string();
    let beta = fixtures
        .icon(fixtures::SET, "48x48/apps", "fixture-beta.png")
        .display()
        .to_string();

    assert_eq!(
        icons_of(&pass[0]),
        vec![alpha.as_str()],
        "workspace 1 holds one window of the vector-icon program: {}",
        pass[0]
    );
    assert_eq!(
        icons_of(&pass[1]),
        vec![beta.as_str()],
        "workspace 2 holds one window of the raster-icon program: {}",
        pass[1]
    );

    for record in &pass {
        let windows: usize = field(record, "windows")
            .and_then(|value| value.parse().ok())
            .expect("every record names its window count");
        assert_eq!(
            icons_of(record).len(),
            windows,
            "exactly one icon per window, none left iconless: {record}"
        );
        assert_eq!(
            field(record, "shed"),
            Some("0"),
            "nothing was shed: {record}"
        );
    }
}

// --- T066/T067: icons in the grid miniatures (FR-037, FR-038, US3) ----------

/// Every window rectangle in a miniature carries its program's icon alongside its title, and a
/// title too long for its rectangle is truncated visibly rather than dropped (US3-AS1/AS2).
#[test]
fn e2e_icons_in_grid_miniatures() {
    // Long enough that no rectangle in a 240-pixel cell could ever hold it, so the truncation
    // half of US3-AS2 is exercised rather than assumed.
    const LONG: &str = "an alpha window whose title runs on far past anything a miniature \
                        rectangle could hold, and then further still";

    let (fixtures, run) = run(
        &config(GRID),
        &[
            (fixtures::ALPHA_CLASS, LONG, 1),
            (fixtures::BETA_CLASS, "beta-window", 2),
        ],
    );
    let pass = run.last_pass(2);

    let alpha = fixtures
        .icon(fixtures::SET, "scalable/apps", "fixture-alpha.svg")
        .display()
        .to_string();
    let beta = fixtures
        .icon(fixtures::SET, "48x48/apps", "fixture-beta.png")
        .display()
        .to_string();

    assert!(
        pass.iter().all(|record| record.contains(" grid:")),
        "these are the grid's records, not the list's: {pass:?}"
    );
    assert_eq!(
        icons_of(&pass[0]),
        vec![alpha.as_str()],
        "the rectangle in workspace 1's miniature draws its own program's icon: {}",
        pass[0]
    );
    assert_eq!(
        icons_of(&pass[1]),
        vec![beta.as_str()],
        "and so does the other program's, in the other miniature: {}",
        pass[1]
    );

    for record in &pass {
        let windows: usize = field(record, "windows")
            .and_then(|value| value.parse().ok())
            .expect("every record names its window count");
        assert_eq!(
            rects_of(record),
            vec!["icon+title"; windows],
            "every rectangle holds both its icon and its title at this size (FR-037): {record}"
        );
        assert_eq!(
            icons_of(record).len(),
            windows,
            "one icon per window, none left iconless: {record}"
        );
        assert_eq!(
            field(record, "shed"),
            Some("0"),
            "nothing was shed from a rectangle this size: {record}"
        );
    }

    assert_eq!(
        field(&pass[0], "ellipsized"),
        Some("true"),
        "FR-015b: the overlong title is truncated with a visible indication rather than \
         overflowing its rectangle or being dropped for the icon: {}",
        pass[0]
    );
    assert_eq!(
        field(&pass[1], "ellipsized"),
        Some("false"),
        "and a title that fits is not truncated: {}",
        pass[1]
    );
}

/// A workspace whose windows are of very different sizes produces rectangles in all three of
/// FR-038's states — and never the fourth, which would be the shedding order backwards (US3-AS3).
#[test]
fn e2e_miniature_drops_title_then_icon() {
    // The three sizes, in the monitor's own pixels. A 1920×1080 workspace maps into a 218×123
    // miniature, so these become roughly 114×68, 46×46 and 11×7 — one rectangle comfortably
    // holding both, one with room for the icon alone, and one too small for either.
    const SIZES: [(u32, u32); 3] = [(1000, 600), (400, 400), (96, 64)];

    let fixtures = Fixtures::stage();
    let run = run_shaped(
        &fixtures,
        &config(GRID),
        &[
            (fixtures::ALPHA_CLASS, "roomy-window", 1),
            (fixtures::ALPHA_CLASS, "cramped-window", 1),
            (fixtures::ALPHA_CLASS, "shrunken-window", 1),
        ],
        1,
        &[],
        |nested, staged| {
            // Floated and sized by hand: under a tiling layout the sizes would be Hyprland's
            // decision, and this test is about the renderer's behaviour at three known sizes.
            for (client, (width, height)) in staged.iter().zip(SIZES) {
                nested.dispatch(&format!("setfloating address:{}", client.address));
                nested.dispatch(&format!(
                    "resizewindowpixel exact {width} {height},address:{}",
                    client.address
                ));
            }
            nested.wait_until("the windows are the sizes the scenario asked for", || {
                let reported = nested.clients();
                staged.iter().zip(SIZES).all(|(client, size)| {
                    reported
                        .iter()
                        .find(|window| window.address == client.address)
                        .is_some_and(|window| window.size == size)
                })
            });
        },
    );

    let pass = run.last_pass(1);
    let record = &pass[0];
    let states = rects_of(record);

    assert_eq!(
        states.len(),
        3,
        "FR-038: every window still gets a rectangle, whatever it had room for: {record}"
    );
    for state in ["icon+title", "icon", "none"] {
        assert!(
            states.contains(&state),
            "no rectangle ended up in the {state:?} state, so the shedding order is untested: \
             {record}"
        );
    }
    assert!(
        !states.contains(&"title"),
        "a rectangle kept its title and shed its icon, which is FR-038's order backwards: {record}"
    );

    // The rest of the record agrees with those states: an icon was drawn for each rectangle that
    // kept one, and each rectangle that did not is counted as having shed it.
    assert_eq!(
        icons_of(record).len(),
        states
            .iter()
            .filter(|state| state.starts_with("icon"))
            .count(),
        "the icons drawn and the rectangles that kept one disagree: {record}"
    );
    assert_eq!(
        field(record, "shed").and_then(|value| value.parse::<usize>().ok()),
        Some(states.iter().filter(|state| **state == "none").count()),
        "the shed count and the rectangles that dropped their icon disagree: {record}"
    );
}

// --- T045: the placeholder (FR-041, US1-AS4) --------------------------------

/// A window whose class matches no desktop entry shows the placeholder — in the same slot, so the
/// names beside it stay aligned — and nothing is reported about it.
#[test]
fn e2e_icon_placeholder_for_unknown_program() {
    let (_fixtures, run) = run(
        &config(""),
        &[
            (fixtures::UNKNOWN_CLASS, "nobody-window", 1),
            (fixtures::ALPHA_CLASS, "alpha-window", 2),
        ],
    );
    let pass = run.last_pass(2);

    assert_eq!(
        icons_of(&pass[0]),
        vec![PLACEHOLDER_SOURCE],
        "an unclaimed class draws the placeholder: {}",
        pass[0]
    );
    assert_eq!(
        icons_of(&pass[1]).len(),
        1,
        "and the program beside it still gets its own icon: {}",
        pass[1]
    );
    assert_ne!(icons_of(&pass[1]), vec![PLACEHOLDER_SOURCE]);
    assert!(
        run.icon_diagnostics().is_empty(),
        "FR-041: an unresolvable icon is a normal outcome, not a reported failure: {:?}",
        run.icon_diagnostics()
    );
}

// --- T046: icons change no geometry (FR-036, SC-013, SC-015, US1-AS5) -------

/// The overlay is exactly the size it was before this feature: icons change neither the row
/// height nor how many entries are on screen.
#[test]
fn e2e_icons_keep_row_height_and_count() {
    let recorded = baseline("list.json");
    let fixtures = Fixtures::stage();
    // The baseline's own scenario, every window given a resolvable program identity so the rows
    // really are carrying icons while being measured.
    let staged: Vec<Staged<'_>> = vec![
        (fixtures::ALPHA_CLASS, "alpha-window", 1),
        (fixtures::BETA_CLASS, "beta-window", 2),
        (fixtures::ALPHA_CLASS, "gamma-window", 2),
        (fixtures::BETA_CLASS, "crowded-window-one", 4),
        (fixtures::ALPHA_CLASS, "crowded-window-two", 4),
        (fixtures::BETA_CLASS, "crowded-window-three", 4),
        (fixtures::ALPHA_CLASS, "crowded-window-four", 4),
        (fixtures::BETA_CLASS, "crowded-window-five", 4),
    ];
    let run = run_with(&fixtures, &config(""), &staged, 1, &[]);

    let (_, _, width, height) = run.geometry;
    let expected = &recorded["surface"];
    assert_eq!(
        (i64::from(width), i64::from(height)),
        (
            expected["w"].as_i64().expect("w"),
            expected["h"].as_i64().expect("h"),
        ),
        "icons changed the overlay's size; the pre-feature baseline is {expected}"
    );

    let visible = usize::try_from(
        recorded["metrics"]["visible_entries"]
            .as_u64()
            .expect("visible_entries"),
    )
    .expect("a plausible count");
    let entries = recorded["scenario"]["entries"]
        .as_array()
        .expect("entries")
        .len();
    let on_screen = visible.min(entries);
    let pass = run.last_pass(on_screen);
    assert_eq!(
        pass.len(),
        on_screen,
        "icons changed how many entries are visible without scrolling"
    );
    // And every window on those rows really did draw an icon, so the size above was measured
    // with icons present rather than with the feature quietly off.
    assert!(
        pass.iter().any(|record| !icons_of(record).is_empty()),
        "no icons were drawn at all, so this proves nothing: {pass:?}"
    );
}

// --- T047: names truncate sooner (FR-036a, US1-AS2) -------------------------

/// A workspace of many windows still renders one visibly truncated line, and the icons have taken
/// real width from the names — which is what "truncates sooner than the same row without icons"
/// means for a single-line row.
#[test]
fn e2e_icons_truncate_names_sooner() {
    let crowded: Vec<Staged<'_>> = (1..=8)
        .map(|n| {
            (
                fixtures::ALPHA_CLASS,
                match n {
                    1 => "crowded-window-one-with-a-very-long-title",
                    2 => "crowded-window-two-with-a-very-long-title",
                    3 => "crowded-window-three-with-a-very-long-title",
                    4 => "crowded-window-four-with-a-very-long-title",
                    5 => "crowded-window-five-with-a-very-long-title",
                    6 => "crowded-window-six-with-a-very-long-title",
                    7 => "crowded-window-seven-with-a-very-long-title",
                    _ => "crowded-window-eight-with-a-very-long-title",
                },
                1,
            )
        })
        .collect();
    let (_fixtures, run) = run(&config(""), &crowded);
    let pass = run.last_pass(1);
    let record = &pass[0];

    assert_eq!(
        field(record, "windows"),
        Some("8"),
        "the crowded workspace is the one being measured: {record}"
    );
    assert_eq!(
        field(record, "ellipsized"),
        Some("true"),
        "the row must truncate visibly rather than wrap or overflow (FR-036a): {record}"
    );
    let taken: u32 = field(record, "icon_width")
        .and_then(|value| value.parse().ok())
        .expect("the record names the width the icons took");
    assert!(
        taken > 0,
        "the icons took no width from the names, so nothing truncates sooner: {record}"
    );
    assert!(
        !icons_of(record).is_empty(),
        "icons were reserved but none drawn: {record}"
    );
}

// --- T048/T049: both formats resolve (FR-040a, SC-012) ----------------------

/// A program whose icon set supplies only a vector file still shows its own icon (SC-012).
#[test]
fn e2e_vector_icon_renders() {
    let (fixtures, run) = run(&config(""), &[(fixtures::ALPHA_CLASS, "alpha-window", 1)]);
    let pass = run.last_pass(1);
    let expected = fixtures
        .icon(fixtures::SET, "scalable/apps", "fixture-alpha.svg")
        .display()
        .to_string();

    assert_eq!(
        icons_of(&pass[0]),
        vec![expected.as_str()],
        "the vector icon did not resolve: {}",
        pass[0]
    );
}

/// And a program whose set supplies only a raster file, likewise.
#[test]
fn e2e_raster_icon_renders() {
    let (fixtures, run) = run(&config(""), &[(fixtures::BETA_CLASS, "beta-window", 1)]);
    let pass = run.last_pass(1);
    let expected = fixtures
        .icon(fixtures::SET, "48x48/apps", "fixture-beta.png")
        .display()
        .to_string();

    assert_eq!(
        icons_of(&pass[0]),
        vec![expected.as_str()],
        "the raster icon did not resolve: {}",
        pass[0]
    );
}

// --- T050: a broken file is reported once (FR-044) --------------------------

/// A truncated icon file is reported exactly once however many times the overlay is opened, and
/// shows the placeholder from then on.
#[test]
fn e2e_malformed_icon_reported_once() {
    let fixtures = Fixtures::stage();
    let run = run_with(
        &fixtures,
        &config(""),
        &[(fixtures::BROKEN_CLASS, "broken-window", 1)],
        3,
        &[],
    );

    let reported = run.icon_diagnostics();
    assert_eq!(
        reported.len(),
        1,
        "FR-044: reported once across three openings, got {reported:?}"
    );
    assert!(
        reported[0].contains("fixture-broken.png") && reported[0].starts_with("WARN"),
        "the record names the file and does not claim to be fatal: {}",
        reported[0]
    );

    let pass = run.last_pass(1);
    assert_eq!(
        icons_of(&pass[0]),
        vec![PLACEHOLDER_SOURCE],
        "a file that cannot be decoded shows the placeholder: {}",
        pass[0]
    );
}

// --- T051: nothing installed at all (FR-041, SC-016) ------------------------

/// An empty `XDG_DATA_*` root still opens the overlay, with every name readable and no error
/// raised: the placeholder ships with the application.
#[test]
fn e2e_no_icon_set_installed() {
    let fixtures = Fixtures::empty();
    let run = run_with(
        &fixtures,
        &config(""),
        &[
            (fixtures::ALPHA_CLASS, "alpha-window", 1),
            (fixtures::BETA_CLASS, "beta-window", 2),
        ],
        1,
        &[],
    );

    let pass = run.last_pass(2);
    for record in &pass {
        assert_eq!(
            icons_of(record),
            vec![PLACEHOLDER_SOURCE],
            "with nothing installed every window falls back to the embedded placeholder: {record}"
        );
    }
    assert!(
        pass[0].contains("label=\"1\"") && pass[1].contains("label=\"2\""),
        "the workspace names are still drawn: {pass:?}"
    );
    assert!(
        run.icon_diagnostics().is_empty(),
        "SC-016: no error is raised, got {:?}",
        run.icon_diagnostics()
    );
    assert!(
        run.geometry.2 > 0 && run.geometry.3 > 0,
        "the overlay still mapped with a real size"
    );
}

// --- T052: resolution happens before the overlay opens (FR-043, FR-043a) ----

/// Opening the overlay performs no resolution: every icon diagnostic the run produced was emitted
/// before the first entry was ever painted, and the overlay was neither delayed nor repainted to
/// swap an icon in.
#[test]
fn e2e_icons_resolved_before_overlay_opens() {
    let fixtures = Fixtures::stage();
    let run = run_with(
        &fixtures,
        &config(""),
        &[
            (fixtures::BROKEN_CLASS, "broken-window", 1),
            (fixtures::ALPHA_CLASS, "alpha-window", 2),
        ],
        2,
        &[],
    );

    // The broken fixture is the observable one: resolving it emits a diagnostic, so where that
    // line sits relative to the first paint record says when resolution ran (FR-043).
    let lines: Vec<&str> = run.stderr.lines().collect();
    let resolved_at = lines
        .iter()
        .position(|line| line.contains(" icon."))
        .expect("resolving the broken fixture reports once");
    let first_paint = lines
        .iter()
        .position(|line| line.contains("paint: "))
        .expect("the overlay painted");
    assert!(
        resolved_at < first_paint,
        "resolution happened during a paint rather than ahead of it:\n{}",
        run.stderr
    );

    // No entry was ever repainted to swap a placeholder for a real icon (FR-043a). Keyed by the
    // workspace label rather than by position in the pass: MRU reorders the entries between
    // openings, so entry 0 of one paint is not entry 0 of the next, and comparing positions would
    // be comparing two different workspaces.
    let records = run.records();
    let mut drawn: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for record in &records {
        let label = field(record, "label").expect("every record names its entry");
        let icons = icons_of(record);
        match drawn.get(label) {
            Some(first) => assert_eq!(
                &icons, first,
                "workspace {label:?} changed its icons between paints, so an entry was repainted \
                 to swap one in:\n{}",
                run.stderr
            ),
            None => {
                drawn.insert(label, icons);
            }
        }
    }
    assert!(
        drawn.values().any(|icons| !icons.is_empty()),
        "no icons were drawn at all, so this proves nothing:\n{}",
        run.stderr
    );
}

// --- T053: one resolution per program (FR-042, SC-017) ----------------------

/// Three windows of one program across two openings resolve exactly once.
#[test]
fn e2e_icon_resolved_once_per_program() {
    let fixtures = Fixtures::stage();
    // The broken fixture again, for the same reason: a resolution that happens is a line on
    // stderr, and a resolution that is served from the cache is silence.
    let run = run_with(
        &fixtures,
        &config(""),
        &[
            (fixtures::BROKEN_CLASS, "broken-one", 1),
            (fixtures::BROKEN_CLASS, "broken-two", 1),
            (fixtures::BROKEN_CLASS, "broken-three", 1),
        ],
        2,
        &[],
    );

    assert_eq!(
        run.icon_diagnostics().len(),
        1,
        "SC-017: three windows over two openings resolved more than once: {:?}",
        run.icon_diagnostics()
    );

    let pass = run.last_pass(1);
    assert_eq!(
        icons_of(&pass[0]).len(),
        3,
        "all three windows were still drawn with an icon each: {}",
        pass[0]
    );
}

// --- T054: no cache on disk, ever (FR-043b) ---------------------------------

/// Nothing is written under any XDG cache location across a session.
#[test]
fn e2e_no_icon_cache_on_disk() {
    let cache = std::env::temp_dir().join(format!("hypr-swap-cache-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::create_dir_all(&cache).expect("a cache probe directory");

    let fixtures = Fixtures::stage();
    let before = tree(fixtures.data_home());
    let run = run_with(
        &fixtures,
        &config(""),
        &[
            (fixtures::ALPHA_CLASS, "alpha-window", 1),
            (fixtures::BETA_CLASS, "beta-window", 2),
            (fixtures::BROKEN_CLASS, "broken-window", 2),
        ],
        2,
        &[("XDG_CACHE_HOME", &cache.display().to_string())],
    );

    assert!(
        !run.records().is_empty(),
        "the session did nothing, so it cannot have proved anything:\n{}",
        run.stderr
    );
    // Not "the directory is empty": pango's font stack writes a fontconfig cache there the first
    // time it measures text, and it did so before this feature existed. What FR-043b forbids is an
    // *icon* cache, so the assertion is that everything under the cache root belongs to
    // fontconfig — nothing this application put there.
    let written: Vec<String> = tree(&cache)
        .into_iter()
        .filter(|path| !path.starts_with("fontconfig"))
        .collect();
    assert_eq!(
        written,
        Vec::<String>::new(),
        "FR-043b: the application wrote an icon cache into the XDG cache directory"
    );
    assert_eq!(
        tree(fixtures.data_home()),
        before,
        "FR-043b: the application wrote into the icon set it read from"
    );
    let _ = std::fs::remove_dir_all(&cache);
}

// --- T087: icons off is the pre-feature overlay (FR-056, SC-019, US6-AS1) ---

/// US6-AS1, SC-019: with `icons = false` and the default theme, the overlay is the one that
/// existed before this feature — in both presentations.
///
/// "Pixel-identical" is asserted through the two interfaces research.md R22 allows, since R14
/// rejected screenshot comparison: the surface geometry `hyprctl layers` reports, compared against
/// the baseline recorded from the pre-feature build in T001, and the paint records, which must
/// name no icon and reserve no room for one. Every window is spawned under a class the fixtures
/// *can* resolve, so the absence of icons is the setting doing its work rather than a lookup
/// quietly failing.
///
/// Staged with the baseline's own helper rather than through [`run_with`], because the baseline
/// scenario includes an empty workspace and only that helper creates one.
///
/// Together with `e2e_default_appearance_unchanged`, which pins the colours, this is what closes
/// SC-018's claim that icons are the only difference this feature makes.
#[test]
fn e2e_icons_disabled_matches_pre_feature() {
    for presentation in ["list", "grid"] {
        let recorded = baseline(&format!("{presentation}.json"));
        let fixtures = Fixtures::stage();
        let app_config = format!(
            "icons = false\n{}",
            if presentation == "grid" { GRID } else { "" }
        );

        let nested = Nested::start_with(&Setup::documented().with_app_config(&app_config));
        let panel = pinned_panel(&nested);
        let _windows = stage_classified(&nested, &panel, Some(fixtures::ALPHA_CLASS));

        let daemon = start(&nested, &fixtures, &[]);
        let mut keyboard = Keyboard::attach(&nested.wayland_display);
        let geometry = measure(&nested, &mut keyboard);
        let stderr = daemon.stderr();

        let expected = &recorded["surface"];
        assert_eq!(
            (
                i64::from(geometry.0),
                i64::from(geometry.1),
                i64::from(geometry.2),
                i64::from(geometry.3),
            ),
            (
                expected["x_on_monitor"].as_i64().expect("x_on_monitor"),
                expected["y_on_monitor"].as_i64().expect("y_on_monitor"),
                expected["w"].as_i64().expect("w"),
                expected["h"].as_i64().expect("h"),
            ),
            "with icons off the {presentation} overlay is not where the pre-feature one was"
        );

        let entries = recorded["scenario"]["entries"]
            .as_array()
            .expect("the baseline records the entries");
        let visible = usize::try_from(
            recorded["metrics"]["visible_entries"]
                .as_u64()
                .expect("the baseline records the visible entry count"),
        )
        .expect("a plausible entry count");
        let on_screen = visible.min(entries.len());

        let records = paint_records(&stderr);
        assert!(
            records.len() >= on_screen,
            "fewer records than entries on screen:\n{stderr}"
        );
        let pass = &records[records.len() - on_screen..];

        for (index, record) in pass.iter().enumerate() {
            let entry = &entries[index];
            let label = entry["label"].as_str().expect("a label");
            let windows = entry["windows"].as_array().expect("windows").len();
            assert!(
                record.starts_with(&format!("entry {index} {presentation}:")),
                "record {index} names the wrong entry or presentation: {record}"
            );
            assert!(
                record.contains(&format!("label={label:?}")),
                "record {index} drew {record}, expected the baseline's {label:?}"
            );
            assert!(
                record.contains(&format!("windows={windows}")),
                "record {index} drew {record}, expected {windows} windows"
            );
            assert!(
                icons_of(record).is_empty(),
                "an icon was drawn with icons off: {record}"
            );
            assert_eq!(
                field(record, "icon_width"),
                Some("0"),
                "room was reserved for an icon that is never drawn: {record}"
            );
            assert_eq!(
                field(record, "shed"),
                Some("0"),
                "with icons off there is nothing to shed: {record}"
            );
            assert!(
                !rects_of(record).iter().any(|rect| rect.contains("icon")),
                "a miniature rectangle reserved room for an icon: {record}"
            );
        }

        // And nothing was looked up either: FR-056 suppresses the resolution as well as the paint.
        assert!(
            !stderr.contains(" icon."),
            "the icon set was searched with icons off:\n{stderr}"
        );
    }
}

// --- T088: the icon set is selectable (FR-057, US6-AS3) ---------------------

/// US6-AS3: `icon_set` naming the second installed set draws that set's artwork.
///
/// Both fixture sets carry the same icon names, so the only thing that can distinguish them is the
/// file each icon actually came from — which is exactly what the paint record names.
#[test]
fn e2e_icon_set_selected() {
    let fixtures = Fixtures::stage();
    let app_config = format!("icon_set = \"{}\"\n", fixtures::SECOND_SET);
    let run = run_with(&fixtures, &app_config, ONE_EACH, 1, &[]);
    let pass = run.last_pass(2);

    for (index, (set, directory, file)) in [
        (fixtures::SECOND_SET, "scalable/apps", "fixture-alpha.svg"),
        (fixtures::SECOND_SET, "48x48/apps", "fixture-beta.png"),
    ]
    .into_iter()
    .enumerate()
    {
        let expected = fixtures.icon(set, directory, file).display().to_string();
        assert_eq!(
            icons_of(&pass[index]),
            vec![expected.as_str()],
            "entry {index} did not draw the named set's icon: {}",
            pass[index]
        );
    }

    // The set the user did *not* name contributed nothing, so this is a real selection rather than
    // both sets happening to answer.
    let other = format!("/{}/", fixtures::SET);
    for record in &pass {
        assert!(
            !record.contains(&other),
            "an icon came from the set that was not selected: {record}"
        );
    }
    assert!(run.running, "the daemon stopped");
}

// --- T089: an unknown set is reported and falls back (FR-057, US6-AS4) ------

/// US6-AS4: a set that is not installed is reported, the default is used instead, every other
/// setting still applies, and the daemon keeps running (FR-024).
#[test]
fn e2e_unknown_icon_set_falls_back() {
    let fixtures = Fixtures::stage();
    // The grid alongside it, so "every other setting still applies" is asserted rather than
    // assumed: a fallback that discarded the rest of the configuration would draw the list.
    let run = run_with(
        &fixtures,
        &format!("icon_set = \"NoSuchSet\"\n{GRID}"),
        ONE_EACH,
        1,
        &[],
    );

    assert!(
        run.stderr.contains("WARN  config.icon_set:"),
        "the unknown set was not reported:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("NoSuchSet") && run.stderr.contains("hicolor"),
        "the report does not say what was wrong and what was used instead:\n{}",
        run.stderr
    );

    let pass = run.last_pass(2);
    assert!(
        pass.iter().all(|record| record.contains(" grid:")),
        "the presentation setting was dropped along with the icon set: {pass:?}"
    );
    // Nothing in the staged root is installed under `hicolor`, so the fallback set resolves
    // nothing and every window is the placeholder — which is FR-041's normal outcome, not a
    // failure, and leaves every name readable.
    for record in &pass {
        for icon in icons_of(record) {
            assert_eq!(
                icon, PLACEHOLDER_SOURCE,
                "an icon resolved from a set that is not installed: {record}"
            );
        }
    }
    assert!(
        run.running,
        "the daemon stopped over a configuration value it was meant to fall back from"
    );
}

/// Every path under `root`, relative and sorted — enough to notice a file appearing.
fn tree(root: &std::path::Path) -> Vec<String> {
    fn walk(directory: &std::path::Path, base: &std::path::Path, found: &mut Vec<String>) {
        let Ok(reader) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in reader.flatten() {
            let path = entry.path();
            if let Ok(relative) = path.strip_prefix(base) {
                found.push(relative.display().to_string());
            }
            if path.is_dir() {
                walk(&path, base, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}
