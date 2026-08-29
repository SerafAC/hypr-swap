# Implementation Plan: Overlay Visuals

**Branch**: `002-overlay-visuals` | **Date**: 2026-08-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-overlay-visuals/spec.md`

## Summary

Two changes to what the overlay draws, delivered as one feature because they share the renderer,
the configuration file, and the entry row itself.

**Program icons** (FR-035–FR-044): every window listed in either presentation is drawn with the icon
of the program that owns it. Icons are resolved ahead of time — when a window first appears and at
start-up — into a per-program in-memory cache, so opening the overlay only blits already-decoded
surfaces and never touches the filesystem (FR-043). Resolution is the freedesktop path: window class
→ desktop entry → icon name → icon set lookup → decode. Raster icons decode through cairo's own PNG
loader; vector icons through `resvg`, the one new dependency this feature adds
([research.md](./research.md) R18).

**Theming** (FR-045–FR-061): the eleven hard-coded colours in `ui/render.rs` and the ten geometry
constants in `ui/layout.rs` become resolved values rather than `const`s. A built-in theme is a
palette and nothing more (FR-049); fonts and geometry have one shared default each, reachable only
through per-key overrides. Resolution is one documented chain — override, then theme, then default —
expressed once in a new pure `theme.rs`.

The seam the project already follows holds: every decision rule (colour parsing, range clamping,
override precedence, class-to-entry matching, icon-set directory choice, which content a small
rectangle sheds) lands in pure unit-tested functions, and `ui/render.rs` stays a painter that is
told what to draw.

## Technical Context

**Language/Version**: Rust 1.96 (edition 2024) — unchanged.

**Primary Dependencies**: everything from feature 001, plus:

- `resvg` (with `usvg`, `tiny-skia`) — vector icon rasterisation. Default features only: no text
  shaping, no `svgz`. The single new direct dependency; justified in Complexity Tracking.
- `cairo-rs` gains its existing `png` feature — **[verified]** present in cairo-rs 0.22.0
  (`png = ["cairo-sys-rs/png"]`). Raster decoding therefore adds no crate at all.
- No new crate for desktop-entry parsing or icon-set lookup — both are implemented directly, as
  the Hyprland IPC was in 001 ([research.md](./research.md) R19, R20).

**Storage**: N/A, and explicitly so. FR-043b forbids an on-disk icon cache; resolved icons live in
process memory and die with it.

**Testing**: `cargo test` as before. New unit tests are in-module. New E2E tests drive the nested
Hyprland from `tests/e2e/harness.rs` and assert through two real external interfaces: the
compositor's own `hyprctl layers` geometry (**[verified]** — it reports `xywh` per layer surface)
and the daemon's stderr diagnostics (FR-029). A fixture icon set and fixture desktop entries are
staged into a temporary `XDG_DATA_HOME` so that no test depends on what the developer has installed
([research.md](./research.md) R22).

**Target Platform**: unchanged — Linux / Wayland / Hyprland ≥ 0.55.

**Performance Goals**: unchanged budgets, now with icons: overlay on screen ≤150 ms (SC-001,
SC-011). Measured on the development machine: 143 desktop entries totalling 408 KB to index once at
start-up, and a median 2.0 KB / p90 7.7 KB SVG to rasterise once per distinct program. Steady-state
cost of a window opening for an already-seen program is zero.

**Constraints**: No on-disk cache (FR-043b). No repaint of an open overlay to swap in a late icon
(FR-043a). No recolouring of program artwork (FR-051). Default configuration must reproduce the
pre-feature appearance exactly (FR-049a, SC-018, SC-019). Icons must not change row height or entry
count (FR-036), and no valid configuration may break the FR-019 / FR-015a layout guarantees
(FR-053).

**Scale/Scope**: 10–30 distinct programs per session; 16–190 KB of cached icon surfaces. Roughly
900–1100 new lines across the modules below, plus a mechanical refactor of `ui/layout.rs` and
`ui/render.rs` from constants to resolved values.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Gates derived from `.specify/memory/constitution.md` (v1.0.0).

**Initial evaluation (pre-Phase 0)**: PASS with three items flagged for research — the vector
rasteriser (risk of a heavy or ill-fitting dependency), the icon-set lookup (risk of hand-rolling a
large spec, or of adopting a crate on a release treadmill), and how a visual feature can be
E2E-covered at all given 001's R14 rejected screenshot comparison. All three are resolved in
Phase 0; two leave entries in Complexity Tracking.

**Post-design re-evaluation (post-Phase 1)**:

- [x] **I. KISS**: PASS with one justified dependency. The feature adds no framework, no image
      abstraction layer, and no plugin surface. Raster decoding reuses a cairo feature already
      compiled in; desktop entries are four keys out of an INI file; the icon-set lookup is the
      freedesktop directory rule written out once. The one genuinely new capability — turning an
      SVG into pixels — cannot be hand-rolled, so `resvg` enters with a Complexity Tracking entry.
- [x] **II. YAGNI**: PASS. Every module below traces to a requirement; the trace is in
      [contracts/README.md](./contracts/README.md). Three capabilities were deliberately *not*
      built because no requirement asks for them: an on-disk icon cache (FR-043b forbids it), a
      configurable resolution strategy (rejected during clarification and recorded in the spec's
      Assumptions), and `svgz` support (exactly one such file exists across every icon set installed
      on the development machine; FR-040a already defines the fallback).
- [x] **III. DRY**: PASS, and the feature removes an existing duplication. Today the overlay's
      geometry is `const`s in `ui/layout.rs` while its colours are `const`s in `ui/render.rs`; after
      this change both are style values with exactly one definition, in `theme.rs`. The precedence
      chain of FR-050 is expressed once, in `theme.rs::resolve`. The class-to-entry matching rule
      lives only in `icons/entries.rs`; the icon-set directory rule only in `icons/iconset.rs`.
- [x] **IV. Unit tests**: PASS, with the same documented shell exemption 001 carries. Newly added
      pure logic is unit-tested in-module: all of `theme.rs` (colour parsing, range clamping,
      precedence), `icons/entries.rs` (the matching rule), `icons/iconset.rs` (directory scoring and
      inheritance), `icons/mod.rs` (cache identity and single-resolution guarantee, against a
      fixture root), and the extended `ui/layout.rs`. `icons/decode.rs` is unit-tested against
      fixture PNG and SVG files. `ui/render.rs` remains in the shell exemption.
- [x] **V. E2E coverage**: PASS. Every major requirement maps to at least one E2E test driving the
      real interface. Visual-only properties are asserted through the daemon's own stderr
      diagnostics rather than by screenshot, which 001's R14 rejected; this is an extension of the
      env-gated hook precedent already in `hypr/ipc.rs`, and is recorded in Complexity Tracking.
      The mapping is the table below.

### E2E coverage mapping

| E2E test | Drives | Covers |
|---|---|---|
| `e2e_icons_in_flat_list` | fixture icon set, 2 programs, default presentation | FR-035, FR-036, FR-040, US1-AS1/3 |
| `e2e_icon_placeholder_for_unknown_program` | window whose class matches no entry | FR-041, US1-AS4 |
| `e2e_icons_keep_row_height_and_count` | same workspaces with icons on and off | FR-036, SC-015, US1-AS5 |
| `e2e_icons_truncate_names_sooner` | workspace with many windows | FR-036a, US1-AS2 |
| `e2e_icons_in_grid_miniatures` | `presentation = "grid"`, fixture icons | FR-037, US3-AS1/2 |
| `e2e_miniature_drops_title_then_icon` | grid, one workspace of many small windows | FR-038, US3-AS3 |
| `e2e_vector_icon_renders` | fixture set supplying only an SVG | FR-040a, SC-012 |
| `e2e_raster_icon_renders` | fixture set supplying only a PNG | FR-040a |
| `e2e_malformed_icon_reported_once` | fixture set with a truncated PNG | FR-044 |
| `e2e_no_icon_set_installed` | `XDG_DATA_*` pointing at an empty root | FR-041, SC-016 |
| `e2e_icons_resolved_before_overlay_opens` | open overlay immediately after a window appears | FR-043, FR-043a |
| `e2e_icon_resolved_once_per_program` | 3 windows of one program, overlay opened twice | FR-042, SC-017 |
| `e2e_no_icon_cache_on_disk` | XDG cache dirs watched across a session | FR-043b |
| `e2e_icons_disabled_matches_pre_feature` | `icons = false` | FR-056, SC-019, US6-AS1 |
| `e2e_icon_set_selected` | `icon_set` naming a second fixture set | FR-057, US6-AS3 |
| `e2e_unknown_icon_set_falls_back` | `icon_set` naming an absent set | FR-057, US6-AS4 |
| `e2e_refactor_is_pixel_neutral` | no config, before/after the Phase 2 refactor | FR-049a, SC-018 |
| `e2e_builtin_theme_applies` | `theme = "light"` | FR-045, FR-048, FR-053, US2-AS1/2 |
| `e2e_theme_on_all_monitors` | `placement = "all"`, two outputs | FR-048, US2-AS3 |
| `e2e_default_appearance_unchanged` | no configuration file | FR-049a, SC-018, US2-AS4 |
| `e2e_theme_switch_does_not_move_layout` | dark then light, `hyprctl layers` xywh compared | FR-049, SC-023, US2-AS5 |
| `e2e_unknown_theme_falls_back` | `theme = "nope"` | FR-058, US2-AS6 |
| `e2e_colour_override_wins_over_theme` | theme plus one colour override | FR-050, US4-AS1 |
| `e2e_overrides_without_theme` | overrides, no theme name | FR-050, US4-AS2 |
| `e2e_font_override_applies` | `font_family` override | FR-046, US4-AS3 |
| `e2e_missing_font_substitutes` | override naming an absent family | US4-AS5 |
| `e2e_invalid_value_falls_back_alone` | one bad colour among several good settings | FR-059, SC-022, US4-AS4 |
| `e2e_geometry_override_resizes` | raised text height and size cap, `hyprctl layers` xywh | FR-047, FR-055, US5-AS1 |
| `e2e_geometry_override_still_caps_and_scrolls` | geometry overrides, 20 workspaces | FR-053, SC-023, US5-AS2 |
| `e2e_grid_geometry_override` | cell size and gap overrides | FR-047, US5-AS3 |
| `e2e_out_of_range_geometry_clamped` | cell size of 0, cap of 5.0 | FR-054, US5-AS4 |
| `e2e_geometry_scales_with_monitor` | overrides on a scale-2 output | FR-055, US5-AS5 |
| `e2e_visual_settings_need_restart` | edit config while running | FR-060 |


Requirements deliberately not E2E-covered: **FR-039** (aspect ratio and device-resolution drawing)
and **FR-051**/**FR-052** (icon artwork not recoloured; icon slot follows text height) are pixel
properties that 001's R14 rules out asserting by screenshot; they are unit-tested in
`icons/decode.rs` and `ui/layout.rs` against computed rectangles, and listed for manual confirmation
in [quickstart.md](./quickstart.md). **SC-012**'s 90 % figure is a property of the user's installed
icon set, not of this code, and is measured by the quickstart survey rather than asserted in CI.
**SC-014** (users find workspaces faster with icons) is a usability criterion measured the same way
001 measures SC-004. **SC-024** and **SC-025** (documentation completeness, no inert setting) are
checked by a unit test that walks the style-value catalogue in
[contracts/style-values.md](./contracts/style-values.md) against the settings `theme.rs` actually
resolves, so the two cannot drift.

## Project Structure

### Documentation (this feature)

```text
specs/002-overlay-visuals/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── README.md        # Contract index + requirement trace
│   ├── config.md        # The visual settings added to the configuration file
│   ├── style-values.md  # The FR-061 catalogue: every value, form, range, default
│   └── icon-lookup.md   # Class → entry → icon name → file, and the test fixture format
├── checklists/
│   └── requirements.md  # Pre-existing
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

New and changed files only; everything else in the 001 tree is untouched.

```text
Cargo.toml                   # + resvg; cairo-rs gains its "png" feature

src/
├── theme.rs                 # NEW pure: palette, style values, colour parsing, ranges,
│                            #   FR-050 precedence. The single home of every default.
├── icons/
│   ├── mod.rs               # NEW: IconStore — per-program cache, ensure()/get(), FR-042/043
│   ├── entries.rs           # NEW: desktop-entry index; the class → icon-name rule
│   ├── iconset.rs           # NEW: freedesktop icon-set lookup (search path, index.theme,
│   │                        #   Inherits, directory size scoring)
│   └── decode.rs            # NEW: PNG via cairo, SVG via resvg → cairo ImageSurface
├── config.rs                # CHANGED: parses the new visual settings, delegates validation
│                            #   to theme.rs; defaults still live in one place
├── model.rs                 # UNCHANGED — Window::class is already deserialised
├── main.rs                  # CHANGED: builds the IconStore, refreshes it on world rebuild
└── ui/
    ├── mod.rs               # CHANGED: carries the resolved Style and the IconStore
    ├── layout.rs            # CHANGED: constants become fields on a Geometry taken from theme.rs;
    │                        #   adds the icon slot and the miniature content-shedding thresholds
    └── render.rs            # CHANGED: colours and fonts read from Style; paints icons inline in
                             #   the list row and inside miniature rectangles

tests/
└── e2e/
    ├── fixtures.rs          # NEW: stages a fixture icon set + desktop entries into a temp
    │                        #   XDG_DATA_HOME so tests never read the developer's own theme
    └── (existing harness.rs, keyboard.rs, clients.rs, notify.rs unchanged)
```

**Structure Decision**: The existing single-binary layout is kept. Icons get a submodule directory
for the same reason `hypr/` has one — there are three genuinely separate concerns (which entry owns
this class, which file is that icon name, how do those bytes become a surface) and collapsing them
into one file would mix three sets of rules. Theming gets a single flat `theme.rs` because it is one
concern with one entry point, and splitting palette from geometry would put the FR-050 precedence
chain in two places, violating Principle III.

Note the naming: `theme.rs` is the *overlay* theme, `icons/iconset.rs` is the *icon set*. FR-057
makes those two independent settings, and the module names keep the spec's vocabulary so the
distinction survives into the code.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| New dependency `resvg` (with `usvg`, `tiny-skia`) | FR-040a requires decoding scalable icons, and SC-012's 90 % target is unreachable without it. Measured: the development machine's configured set (Papirus-Dark) contains 3970 SVG files and **zero** PNGs, so a raster-only build shows the placeholder for essentially every window. Turning SVG into pixels is not something that can be hand-rolled at any sane cost. | **Raster-only** was offered during clarification and rejected by the user; the measurement above shows why. **`librsvg`** is installed system-wide (2.62.3) and renders straight into a `cairo::Context`, which looks like a better fit — but its Rust bindings pin their own `glib`/`cairo` generations, and a mismatch with our `glib` 0.22 would put two incompatible `cairo::Context` types in one tree, destroying the only advantage it had ([research.md](./research.md) R18). **Shelling out to a rasteriser** is a process spawn per icon and a parsing surface, for no gain. |
| Env-gated paint diagnostics used by E2E assertions | Principle V requires E2E coverage of major requirements, but this feature's requirements are almost entirely visual, and 001's R14 already rejected screenshot comparison as brittle across fonts and scaling. Without an observable signal, FR-035/037/038/041/043a would have no E2E at all. The daemon emitting what it resolved and painted makes them assertable through stderr, which FR-029 already defines as a real external interface. | **Screenshot comparison** — rejected in 001 R14 and no less brittle here. **Asserting nothing and relying on unit tests** — fails Principle V for the feature's headline requirements. **A permanent query interface** (a socket or CLI to dump overlay state) — a new external surface no requirement asks for, and Principle II forbids it. The env gate keeps it inert in normal operation, matching the fault-injection hook `hypr/ipc.rs` already carries for the 001 rollback tests. **Extended in US2** with a per-paint colour record, tapped inside `ui/render.rs::set_colour` — the single point every themed colour passes through on its way to cairo — and held on a thread-local armed from the same gate. Taken there rather than beside each drawing call because a push paired with a `set_colour` can disagree with the call it sits next to, and a tap inside it cannot; and threaded state would mean a `&mut` parameter on every drawing helper for evidence that is strictly weaker. **Extended again in US4** with a per-paint font record — the families asked for and the families pango loaded — tapped in `ui/render.rs::line`, the single point every piece of overlay text is given a font. Both halves are kept because FR-046 and US4-AS5 are different claims: that the override reached every layout, and that an absent family is substituted rather than refused. |
| Shell modules unit-test-exempt (Principle IV) | `ui/render.rs`, `ui/mod.rs` and `main.rs` are the deliberately logic-free shell described in `CLAUDE.md`; their behaviour is Wayland and cairo side effects, which a unit test can only assert by re-implementing the compositor. Every decision rule this feature adds lives in `theme.rs`, `icons/*` and `ui/layout.rs`, all of which **are** unit-tested. This is the same deviation feature 001 recorded, restated here because the constitution requires it in *this* feature's table. | **Unit-testing the shell** would mean mocking a compositor and a rendering backend — a test harness larger than the code under test, asserting against the mock rather than reality. **Dropping the E2E suite instead** would leave the feature's headline requirements with no evidence at all, failing Principle V. The split (pure logic unit-tested, shell E2E-covered) is what 001 established and what the E2E mapping above delivers. |
| `ui/layout.rs` public constants become fields | FR-047 makes eight of them user-settable (the other two, corner radius and mark width, live in `ui/render.rs`), so they cannot remain `const`. Every call site and every existing `ui/layout.rs` unit test changes shape. | Keeping the constants and layering overrides on top would leave two sources for one number — exactly the duplication Principle III exists to prevent, and it would make "which value is really in effect" unanswerable from one place. |
