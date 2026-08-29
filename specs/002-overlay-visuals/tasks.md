# Tasks: Overlay Visuals

**Input**: Design documents from `/specs/002-overlay-visuals/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Test tasks are REQUIRED (Constitution IV & V). Unit tests live in-module under
`#[cfg(test)]` per Rust idiom, so a unit-test task names the same `src/*.rs` file as the code it
covers. E2E tests are integration tests under `tests/` driving a nested Hyprland instance
(001 research.md R14), asserting through `hyprctl layers` geometry and the daemon's stderr
(research.md R22). Tests MAY be written after the implementation they cover — test-first ordering is
not required — but a story is not complete until its tests exist and pass.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US6)
- Include exact file paths in descriptions

## Path Conventions

Single Rust binary crate at the repository root: `src/`, `tests/` (plan.md → Project Structure).
Unit tests are in-module; E2E tests are in `tests/e2e_*.rs` with shared harness modules in
`tests/e2e/`.

## A note on the shape of this feature

Phase 2 is unusually large, and deliberately so. Both halves of this feature converge on the same
two files: every colour in `ui/render.rs` and every geometry constant in `ui/layout.rs` becomes a
resolved value. Spreading that refactor across the stories would mean six phases each editing the
same two files. It is real shared infrastructure, so it sits in Foundational, and its checkpoint is
a strong one: **the overlay must be pixel-identical to today while being driven entirely by resolved
values.** Once that holds, each story is a genuinely independent increment.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies and module skeleton

- [X] T001 Record the pre-feature visual baseline in tests/fixtures/baseline/ — the overlay's `xywh` from `hyprctl layers` and a reference capture of both presentations, taken on the current build before any refactor lands. **This must happen first: once Phase 2 begins the pre-feature renderer no longer exists and the baseline can never be reproduced.** Every later "unchanged from before this feature" assertion (FR-049a, SC-018, SC-019) compares against these files
- [X] T002 Add `resvg` to `[dependencies]` in Cargo.toml with `default-features = false` (no `text`, no `system-fonts`, no `svgz` — research.md R18), and add the `png` feature to the existing `cairo-rs` entry (research.md R19)
- [X] T003 [P] Create the theming module skeleton — empty src/theme.rs, declared as `mod theme;` in src/main.rs and src/lib.rs (plan.md → Project Structure)
- [X] T004 [P] Create the icons module skeleton — src/icons/mod.rs declaring `mod entries; mod iconset; mod decode;` with empty src/icons/entries.rs, src/icons/iconset.rs, src/icons/decode.rs; declare `mod icons;` in src/main.rs and src/lib.rs (plan.md → Project Structure)
- [X] T005 [P] Add the placeholder icon asset at assets/placeholder.svg and embed it with `include_bytes!` from src/icons/mod.rs, so a placeholder is always available with no icon set installed (spec Assumptions, SC-016)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Turn every hard-coded colour and geometry constant into a resolved style value, without
changing a single pixel. Also lands the config surface and the test hooks every story needs.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### The style model

- [X] T006 Implement `Colour` in src/theme.rs — RGBA with channels in `0.0..=1.0`, and a total parser accepting exactly `#rgb`, `#rrggbb`, `#rrggbbaa` with alpha defaulting to opaque (research.md R25, contracts/style-values.md)
- [X] T007 [P] Unit tests in src/theme.rs for the colour parser — all three forms, alpha default, case insensitivity, and rejection of every other input including empty string, missing `#`, bad length and non-hex digits (FR-045, research.md R25)
- [X] T008 Implement `Geometry` in src/theme.rs — the ten fields of FR-047 with the defaults and `[min, max]` ranges from contracts/style-values.md, held as `const` range data rather than scattered conditionals (research.md R26)
- [X] T009 Implement clamping in src/theme.rs — an out-of-range geometry value is brought to the nearer bound and the adjustment reported through `diag.rs` naming the setting and the value used (FR-054, FR-059)
- [X] T010 [P] Unit tests in src/theme.rs for clamping — each of the ten values below-min, above-max and in-range, asserting the reported message names the setting and the value actually used (FR-054, FR-059)
- [X] T011 Implement `Theme` (the eleven-colour palette) and `Style` (palette + font family + text size + geometry) in src/theme.rs, with the `dark` built-in whose values are byte-for-byte today's constants in src/ui/render.rs (FR-045, FR-049a, data-model.md)
- [X] T012 Implement `resolve(config) -> Style` in src/theme.rs expressing the FR-050 precedence chain exactly once: explicit override → named theme (colours only) → default (research.md R24)
- [X] T013 [P] Unit tests in src/theme.rs for the precedence chain — override beats theme, theme beats default, overrides without a theme apply over the default theme, and an invalid override falls back alone leaving every other value intact (FR-050, FR-059)

### Configuration surface

- [X] T014 Extend src/config.rs with the four visual settings — `icons` (bool, default true), `icon_set` (string, default: the desktop's configured set), `theme` (string, default `"dark"`), and the `[style]` override table — delegating all value validation to src/theme.rs so defaults keep a single home (contracts/config.md, FR-056, FR-057, FR-049, FR-050)
- [X] T015 [P] Unit tests in src/config.rs for the new settings — defaults with no file, each setting parsed, one invalid setting falling back alone while every other setting still applies, and an unknown `theme` or `icon_set` name reported and falling back (FR-058, FR-059, SC-022)

### The pixel-neutral refactor

- [X] T016 Refactor src/ui/layout.rs — replace the `pub const` geometry with fields taken from `theme::Geometry`, threading it through `list_metrics`, `grid_metrics`, `Metrics` and every helper; `GRID_LABEL_HEIGHT` stays derived rather than becoming a setting (FR-047, plan.md Complexity Tracking)
- [X] T017 Update the existing src/ui/layout.rs unit tests to construct a default `Geometry` instead of reading constants, asserting the same numbers as before — this is the proof the refactor changed no arithmetic (FR-047, FR-049a)
- [X] T018 Refactor src/ui/render.rs — replace the eleven colour constants, the font family and `FONT_FRACTION`, `CORNER` and `MARK_WIDTH` with lookups on the resolved `Style` passed in from src/ui/mod.rs (FR-045, FR-046, FR-047)
- [X] T019 Thread the resolved `Style` from start-up through src/main.rs and src/ui/mod.rs to the paint path, resolving it once at start-up and never re-reading it (FR-060)
- [X] T020 [P] Unit test in src/theme.rs asserting the `dark` theme's eleven colours and the default geometry equal the values the pre-feature renderer used, so a future edit cannot silently change the default appearance (FR-049a, SC-018)

### Test infrastructure

- [X] T021 Add the env-gated paint diagnostics to src/diag.rs — under an environment gate, one record per painted entry naming what was resolved and drawn (icon file chosen, placeholder used, or content shed from a miniature rectangle), following the fault-injection precedent in src/hypr/ipc.rs and inert when unset (research.md R22)
- [X] T022 [P] Unit tests in src/diag.rs for the paint records — correct content, and complete silence when the gate is unset (research.md R22)
- [X] T023 [P] Create tests/e2e/fixtures.rs staging a synthetic `XDG_DATA_HOME` — the desktop entries, both fixture icon sets, and the valid SVG, valid PNG and truncated PNG described in contracts/icon-lookup.md; declare it in tests/e2e/mod.rs (research.md R22)
- [X] T024 [P] Add a `hyprctl layers` geometry helper to tests/e2e/harness.rs returning the overlay surface's `xywh` from the nested instance, for the layout assertions in US2 and US5 (research.md R22)
- [X] T025 E2E test `e2e_refactor_is_pixel_neutral` in tests/e2e_theme.rs — with no configuration file, the overlay's `xywh` and paint records are identical to those recorded before Phase 2, proving the refactor changed nothing (FR-049a, SC-018)

**Checkpoint**: The overlay looks exactly as it did, but every colour and dimension now comes from a
resolved `Style`. User story work can begin.

---

## Phase 3: User Story 1 - Program icons in the flat list (Priority: P1) 🎯 MVP

**Goal**: Every window in the flat list is drawn with the icon of the program that owns it, resolved
ahead of time and cached, with a placeholder whenever resolution fails.

**Independent Test**: With three workspaces holding windows of visibly different programs, open the
overlay in the default flat list presentation and confirm each window name is preceded by that
program's icon, and that entries are the same height and count as before.

### Implementation for User Story 1

- [X] T026 [P] [US1] Implement the desktop-entry index in src/icons/entries.rs — scan `$XDG_DATA_HOME/applications` and each `$XDG_DATA_DIRS/applications`, reading only `Icon`, `StartupWMClass`, `Name` and `NoDisplay` with a minimal INI reader and no new dependency (research.md R21, contracts/icon-lookup.md)
- [X] T027 [US1] Implement the class-to-entry matching ladder in src/icons/entries.rs as a pure function from `(class, index)` to an optional icon name — the five ordered steps of contracts/icon-lookup.md, with `NoDisplay=true` entries ranked last (FR-040)
- [X] T028 [P] [US1] Unit tests in src/icons/entries.rs for the matching ladder — one test per step, first-hit-wins ordering, `NoDisplay` ranking, reverse-DNS ids such as `org.gnome.Nautilus` against class `nautilus`, and no-match yielding `None` (FR-040, FR-041)
- [X] T029 [P] [US1] Implement icon-set lookup in src/icons/iconset.rs — the search path, `index.theme` parsing for `Size`/`Scale`/`Type`/`MinSize`/`MaxSize`/`Threshold`, `Inherits` followed in order, terminating at the standard default set (research.md R20, FR-040)
- [X] T030 [US1] Implement directory choice for a requested size in src/icons/iconset.rs as a pure function over parsed directory metadata, so it is testable without a filesystem (research.md R20)
- [X] T031 [P] [US1] Unit tests in src/icons/iconset.rs — directory scoring for exact, larger and smaller sizes; `Threshold` and `MinSize`/`MaxSize` handling; inheritance chains; and a set with a malformed `index.theme` degrading rather than panicking (FR-040, research.md R20 `[assumed]`)
- [X] T032 [P] [US1] Implement decoding in src/icons/decode.rs — PNG through `cairo::ImageSurface::create_from_png`, SVG through `resvg` into a `tiny_skia::Pixmap` converted to a cairo `ImageSurface`; confirm and document the channel order (research.md R18 `[assumed]`, R19)
- [X] T033 [US1] Implement scaling in src/icons/decode.rs — the icon is fitted to its slot without aspect distortion and rasterised at the monitor's device resolution rather than upscaled (FR-039)
- [X] T034 [P] [US1] Unit tests in src/icons/decode.rs against fixture files — a valid PNG, a valid SVG, a non-square icon keeping its aspect ratio, an unsupported extension, and a truncated file (FR-039, FR-040a, FR-044)
- [X] T035 [US1] Implement `IconStore` in src/icons/mod.rs — `ensure(classes)` resolving only classes absent from the cache and `get(class)` as a pure lookup that never touches the filesystem, caching failures as `Placeholder` so a malformed file is reported once and never retried (FR-041, FR-042, FR-043, FR-044)
- [X] T036 [US1] Hold the store in memory only, dropped on exit and on connection loss alongside the rest of the derived state, writing nothing to disk (FR-043b, research.md R28)
- [X] T037 [P] [US1] Unit tests in src/icons/mod.rs against a fixture root — one resolution per class per run including across repeated openings, failures cached, `get` never resolving, and the store empty when icons are disabled (FR-042, FR-043, SC-017)
- [X] T038 [US1] Wire resolution in src/main.rs — call `ensure` once at start-up and on the existing world-rebuild path that `state.rs` already returns `Applied::ByRebuilding` for, so icons are always cached before any overlay can open (FR-043, research.md R27)
- [X] T039 [US1] Add the icon slot to src/ui/layout.rs — the slot follows the resolved text height so raising the font size raises the icons with it, without changing row height, entry count or the number of entries visible (FR-036, FR-052)
- [X] T040 [P] [US1] Unit tests in src/ui/layout.rs for the icon slot — the slot tracks text height, row height is unchanged from the icon-less case, and visible-entry count is identical with and without icons (FR-036, FR-052, SC-015)
- [X] T041 [US1] Paint icons inline in the flat-list row in src/ui/render.rs — build the row text with `U+FFFC` per icon, reserve each slot with `pango::AttrShape` merged into the list from `pango::parse_markup`, switch from `set_markup` to `set_text` + `set_attributes`, and draw the icons at the positions `Layout::index_to_pos` reports (research.md R23, FR-035, FR-036)
- [X] T042 [US1] Skip any icon whose reserved slot falls past the line's ellipsis in src/ui/render.rs, so the row stays one visibly-truncated line and names truncate sooner rather than the row wrapping or overflowing (FR-036a)
- [X] T043 [US1] Draw the placeholder in the same slot at the same size when a program is unresolved, tinted to the theme's primary text colour, while program artwork is never recoloured (FR-041, FR-051)

### Tests for User Story 1 (REQUIRED)

- [X] T044 [P] [US1] E2E test `e2e_icons_in_flat_list` in tests/e2e_icons.rs — two fixture programs, default presentation, each window name preceded by exactly one icon of its own, and no window drawn iconless (FR-035, FR-036, FR-040, SC-013, US1-AS1/AS3)
- [X] T045 [P] [US1] E2E test `e2e_icon_placeholder_for_unknown_program` in tests/e2e_icons.rs — a window whose class matches no entry shows the placeholder with its name still aligned (FR-041, US1-AS4)
- [X] T046 [P] [US1] E2E test `e2e_icons_keep_row_height_and_count` in tests/e2e_icons.rs — row height and visible-entry count match the pre-feature baseline recorded in T001, so US1 needs no icons-off mode to prove it (FR-036, SC-013, SC-015, US1-AS5)
- [X] T047 [P] [US1] E2E test `e2e_icons_truncate_names_sooner` in tests/e2e_icons.rs — a workspace of many windows still renders one visibly truncated line, with names ellipsised earlier than in the pre-feature baseline recorded in T001 (FR-036a, US1-AS2)
- [X] T048 [P] [US1] E2E test `e2e_vector_icon_renders` in tests/e2e_icons.rs — a fixture set supplying only an SVG resolves to that program's own icon, not the placeholder (FR-040a, SC-012)
- [X] T049 [P] [US1] E2E test `e2e_raster_icon_renders` in tests/e2e_icons.rs — a fixture set supplying only a PNG resolves likewise (FR-040a)
- [X] T050 [P] [US1] E2E test `e2e_malformed_icon_reported_once` in tests/e2e_icons.rs — a truncated PNG is reported exactly once across several overlay openings and shows the placeholder thereafter (FR-044)
- [X] T051 [P] [US1] E2E test `e2e_no_icon_set_installed` in tests/e2e_icons.rs — an empty `XDG_DATA_*` root still opens the overlay with every name readable and no error raised (FR-041, SC-016)
- [X] T052 [P] [US1] E2E test `e2e_icons_resolved_before_overlay_opens` in tests/e2e_icons.rs — opening the overlay performs no resolution, and a window that appeared moments earlier shows the placeholder for that opening without the overlay being delayed or repainted (FR-043, FR-043a)
- [X] T053 [P] [US1] E2E test `e2e_icon_resolved_once_per_program` in tests/e2e_icons.rs — three windows of one program across two openings resolve exactly once (FR-042, SC-017)
- [X] T054 [P] [US1] E2E test `e2e_no_icon_cache_on_disk` in tests/e2e_icons.rs — no file is created under any XDG cache location across a session (FR-043b)

**Checkpoint**: Icons work end to end in the default presentation. This is the MVP.

---

## Phase 4: User Story 2 - Match the overlay with one setting (Priority: P1)

**Goal**: A user names a built-in theme and the whole overlay is recoloured coherently, without the
layout moving.

**Independent Test**: With a configuration naming a non-default built-in theme, open the overlay in
both presentations and confirm every drawn element uses that theme's values and none is left with
the default theme's appearance.

### Implementation for User Story 2

- [X] T055 [US2] Add the `light` built-in theme to src/theme.rs — the eleven colours of FR-045 chosen to read on a light desktop, defined as a palette only so it cannot carry font or geometry values (FR-045, FR-049)
- [X] T056 [US2] Apply the selected theme name in src/theme.rs `resolve`, falling back to the default and reporting when the name is unknown, with every other setting still applied (FR-049, FR-058)
- [X] T057 [P] [US2] Unit tests in src/theme.rs — every built-in theme defines all eleven colours and no font or geometry value, so selecting a theme provably cannot move the layout (FR-049, SC-023)

### Tests for User Story 2 (REQUIRED)

- [X] T058 [P] [US2] E2E test `e2e_builtin_theme_applies` in tests/e2e_theme.rs — `theme = "light"` recolours every themed element in both presentations, with no element left in another theme's colours, and the switch is a single line of configuration (FR-045, FR-048, FR-053, SC-020, SC-021, US2-AS1/AS2)
- [X] T059 [P] [US2] E2E test `e2e_theme_on_all_monitors` in tests/e2e_theme.rs — `placement = "all"` with two headless outputs, every copy using the same theme (FR-048, US2-AS3)
- [X] T060 [P] [US2] E2E test `e2e_default_appearance_unchanged` in tests/e2e_theme.rs — no configuration file, colours and geometry matching the pre-feature baseline from T001. Asserts colours and geometry only, so it passes whether or not US1 has landed and US2 stays independent of it; SC-018's "icons are the only difference" is closed by T087 once both stories exist (FR-049a, SC-018, US2-AS4)
- [X] T061 [P] [US2] E2E test `e2e_theme_switch_does_not_move_layout` in tests/e2e_theme.rs — the overlay's `xywh` from `hyprctl layers` is identical under `dark` and `light` (FR-049, SC-023, US2-AS5)
- [X] T062 [P] [US2] E2E test `e2e_unknown_theme_falls_back` in tests/e2e_theme.rs — an unknown name is reported, the default theme applies, every other setting still applies, and the daemon keeps running (FR-058, US2-AS6)

**Checkpoint**: Both P1 stories are complete and independently demonstrable.

---

## Phase 5: User Story 3 - Program icons in the grid miniatures (Priority: P2)

**Goal**: Each window rectangle in a miniature carries its program's icon alongside its title,
shedding content in a fixed order as rectangles get smaller.

**Independent Test**: Set the presentation to grid, open the overlay, and confirm each window
rectangle shows its program's icon in addition to its title, with the title still legible and still
truncated rather than overflowing.

### Implementation for User Story 3

- [ ] T063 [US3] Add the miniature icon rectangle and the content-shedding thresholds to src/ui/layout.rs — title dropped first, then the icon, so a rectangle shows icon and title, or icon alone, or neither, and is always drawn (FR-038)
- [ ] T064 [P] [US3] Unit tests in src/ui/layout.rs for the shedding order — the two thresholds, all three resulting states, and the rectangle still being produced in every case, including a single fullscreen window whose icon is not scaled up to fill it (FR-038, spec Edge Cases)
- [ ] T065 [US3] Paint the icon inside each window rectangle in src/ui/render.rs, leaving every rectangle's position, size and proportion untouched (FR-037, FR-015a)

### Tests for User Story 3 (REQUIRED)

- [ ] T066 [P] [US3] E2E test `e2e_icons_in_grid_miniatures` in tests/e2e_icons.rs — grid presentation, each rectangle showing its program's icon with its title still truncated visibly (FR-037, US3-AS1/AS2)
- [ ] T067 [P] [US3] E2E test `e2e_miniature_drops_title_then_icon` in tests/e2e_icons.rs — a workspace of many small windows produces rectangles in all three states, in the documented order (FR-038, US3-AS3)

**Checkpoint**: Icons are complete in both presentations.

---

## Phase 6: User Story 4 - Override individual colours and fonts (Priority: P2)

**Goal**: A user overrides individual colours and the font on top of a named theme, and only the
overridden values change.

**Independent Test**: With a theme name plus an override for the highlight colour and the font
family, confirm the highlight and font are the overridden values while every other element still
matches the named theme.

### Implementation for User Story 4

- [ ] T068 [US4] Wire the eleven colour override keys from `[style]` through src/config.rs into `theme::resolve`, each independent so an invalid one affects only itself (FR-045, FR-050, FR-059)
- [ ] T069 [US4] Wire `font_family` and `text_size` from `[style]` into the resolved `Style` and apply them to all overlay text in both presentations in src/ui/render.rs, letting the platform substitute an absent family without raising an error (FR-046)
- [ ] T070 [P] [US4] Unit tests in src/theme.rs for override independence — one invalid colour among several valid settings leaves every other value applied, and the reported message names the setting, what was wrong and the value used (FR-059, SC-022)

### Tests for User Story 4 (REQUIRED)

- [ ] T071 [P] [US4] E2E test `e2e_colour_override_wins_over_theme` in tests/e2e_style.rs — a theme plus one colour override, that element overridden and every other one still from the theme (FR-050, US4-AS1)
- [ ] T072 [P] [US4] E2E test `e2e_overrides_without_theme` in tests/e2e_style.rs — overrides with no theme name apply on top of the default theme (FR-050, US4-AS2)
- [ ] T073 [P] [US4] E2E test `e2e_font_override_applies` in tests/e2e_style.rs — a `font_family` override applied to all text in both presentations (FR-046, US4-AS3)
- [ ] T074 [P] [US4] E2E test `e2e_missing_font_substitutes` in tests/e2e_style.rs — an absent family is substituted, text stays readable, and nothing is reported (US4-AS5)
- [ ] T075 [P] [US4] E2E test `e2e_invalid_value_falls_back_alone` in tests/e2e_style.rs — one bad colour among several good settings, reported once, every other setting still applied (FR-059, SC-022, US4-AS4)

**Checkpoint**: Both P2 stories are complete.

---

## Phase 7: User Story 5 - Resize the overlay for readability (Priority: P3)

**Goal**: A user raises text size, entry height and the size cap, and the overlay grows while
keeping every layout guarantee.

**Independent Test**: With geometry overrides raising text size, entry height and size cap, open the
overlay with more workspaces than fit and confirm entries are drawn larger, the overlay does not
exceed the configured cap, and the highlighted entry is scrolled into view.

### Implementation for User Story 5

- [ ] T076 [US5] Wire the ten geometry override keys from `[style]` through src/config.rs into `theme::Geometry`, applying the clamping and reporting already implemented in Phase 2 (FR-047, FR-054)
- [ ] T077 [P] [US5] Unit tests in src/ui/layout.rs proving the FR-053 invariants hold across the whole valid range of every geometry value — entries stay fixed-size regardless of workspace count, the overlay never exceeds its cap, the highlighted entry is always in view, and entries are never scaled down to fit (FR-053, SC-023)
- [ ] T078 [P] [US5] Unit tests in src/ui/layout.rs for the scale round-trip with non-default geometry, confirming overridden values are logical units scaled per monitor exactly as the defaults are (FR-055)

### Tests for User Story 5 (REQUIRED)

- [ ] T079 [P] [US5] E2E test `e2e_geometry_override_resizes` in tests/e2e_style.rs — raised text height and size cap produce a larger overlay, confirmed against `hyprctl layers` (FR-047, FR-055, US5-AS1)
- [ ] T080 [P] [US5] E2E test `e2e_geometry_override_still_caps_and_scrolls` in tests/e2e_style.rs — geometry overrides with 20 workspaces still cap, still scroll, and still keep entries full size (FR-053, SC-023, US5-AS2)
- [ ] T081 [P] [US5] E2E test `e2e_grid_geometry_override` in tests/e2e_style.rs — cell size and gap overrides applied, each window rectangle keeping its relative position and proportion (FR-047, US5-AS3)
- [ ] T082 [P] [US5] E2E test `e2e_out_of_range_geometry_clamped` in tests/e2e_style.rs — a cell width of 0 and a cap of 5.0 are clamped and reported, and the overlay is usable (FR-054, US5-AS4)
- [ ] T083 [P] [US5] E2E test `e2e_geometry_scales_with_monitor` in tests/e2e_style.rs — the same overrides on a scale-2 output scale as the defaults do (FR-055, US5-AS5)

**Checkpoint**: Geometry is user-controllable without any layout guarantee weakening.

---

## Phase 8: User Story 6 - Turn icons off, or choose the icon set (Priority: P3)

**Goal**: A user disables icons and gets exactly the pre-feature overlay, or names the icon set they
want.

**Independent Test**: With icons disabled, confirm the overlay is identical to the pre-feature one;
then name a specific icon set and confirm the icons drawn come from it.

**Note**: T084 is a few lines and is the escape hatch that makes every icon story safe to ship. Its
P3 label reflects its value, not its cost. No earlier task depends on it — US1's tests compare
against the T001 baseline rather than a runtime icons-off mode — so pulling it forward into Phase 3
is a shipping judgement, not a prerequisite.

### Implementation for User Story 6

- [ ] T084 [US6] Honour `icons = false` in src/ui/render.rs and src/main.rs — no icons, no placeholders, no reserved space, and no desktop-entry scan or icon-set lookup performed at all (FR-056)
- [ ] T085 [US6] Apply `icon_set` in src/icons/iconset.rs — the configured set, else the desktop's configured set, else the standard default, with an unknown name reported and falling back while every other setting still applies (FR-057)
- [ ] T086 [P] [US6] Unit tests in src/icons/iconset.rs for set selection and its fallback chain, and in src/icons/mod.rs for the store staying empty when icons are disabled (FR-056, FR-057)

### Tests for User Story 6 (REQUIRED)

- [ ] T087 [P] [US6] E2E test `e2e_icons_disabled_matches_pre_feature` in tests/e2e_icons.rs — `icons = false` with the default theme reproduces the pre-feature overlay exactly in both presentations (FR-056, SC-019, US6-AS1)
- [ ] T088 [P] [US6] E2E test `e2e_icon_set_selected` in tests/e2e_icons.rs — `icon_set` naming the second fixture set draws that set's icons (FR-057, US6-AS3)
- [ ] T089 [P] [US6] E2E test `e2e_unknown_icon_set_falls_back` in tests/e2e_icons.rs — an absent set is reported, falls back to the default, and the daemon keeps running (FR-057, US6-AS4)

**Checkpoint**: All six stories complete.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [ ] T090 [P] E2E test `e2e_visual_settings_need_restart` in tests/e2e_config.rs — editing a visual setting while running changes nothing until restart (FR-060)
- [ ] T091 [P] Unit test in src/theme.rs walking the catalogue in specs/002-overlay-visuals/contracts/style-values.md against the settings `theme.rs` actually resolves, so no setting can be documented but inert nor implemented but undocumented (FR-061, SC-024, SC-025)
- [ ] T092 Resolve the two `[assumed]` items in research.md — confirm the `tiny_skia`-to-cairo channel order against a fixture of known colour, and run the icon-set parser against every set installed on the machine to confirm no `index.theme` is mishandled; record both outcomes in research.md
- [ ] T093 [P] Update CLAUDE.md — add src/theme.rs and src/icons/ to the module map, place them on the pure/shell seam, and note that `theme.rs` owns every default while `icons/iconset.rs` owns the icon set, which is a different setting from the overlay theme (plan.md → Project Structure)
- [ ] T094 [P] Update the user-facing documentation with the new configuration settings, linking the catalogue in contracts/style-values.md (FR-061)
- [ ] T095 Measure SC-011 with icons enabled — at least 20 workspaces, 60 windows and 10 distinct programs, under a built-in theme with overrides, confirming the overlay is still visible within 150 ms; record the figure alongside feature 001's budget table
- [ ] T096 Run the SC-012 coverage survey from quickstart.md against the machine's real icon set and record the percentage; if it falls below 90 %, record the failing classes and evaluate the `initialClass` avenue that research.md R21 documents as the next step
- [ ] T097 Run every quickstart.md scenario, including the by-eye confirmations for FR-039, FR-051 and FR-052 that no automated test asserts, and the SC-014 usability check comparing time-to-find with icons enabled and disabled (SC-014)
- [ ] T098 Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`, and confirm the full suite passes with no test left failing or skipped without justification (Constitution, Testing Standards)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies. **T001 must complete before any Phase 2 work begins** — it
  captures the pre-feature appearance, which ceases to exist the moment the refactor lands
- **Foundational (Phase 2)**: depends on Setup — **blocks every user story**
- **US1 (Phase 3)** and **US2 (Phase 4)**: both P1, both depend only on Foundational, and are
  genuinely independent of each other — icons and theming touch different code once the refactor has
  landed, and each story's tests compare against the T001 baseline rather than against the other
  story's output
- **US3 (Phase 5)**: depends on Foundational and on US1 (it reuses the `IconStore` and the decoder)
- **US4 (Phase 6)**: depends on Foundational; independent of every icon story
- **US5 (Phase 7)**: depends on Foundational; independent of every icon story
- **US6 (Phase 8)**: depends on Foundational and on US1 (it turns off and re-points what US1 built)
- **Polish (Phase 9)**: depends on all desired stories

### Within Phase 2

T006 → T007; T008 → T009 → T010; T011 → T012 → T013; T014 → T015.
T016 → T017 and T018 → T019 both need T011. T025 needs T021, T024, the refactor complete, and the
T001 baseline to compare against.

### Parallel Opportunities

- **Phase 1**: T001 first and alone (it must run on the untouched build); then T003, T004, T005 in
  parallel after T002
- **Phase 2**: the four tracks — style model (T006–T013), config (T014–T015), refactor
  (T016–T020), test infrastructure (T021–T024) — can be worked concurrently, converging on T025
- **Phase 3**: T026, T029, T032 start in parallel (three separate files); all eleven E2E tasks
  T044–T054 are parallel once the implementation lands
- **Phases 4, 6, 7**: entirely parallel with each other and with Phase 3 given staff, since they
  touch `theme.rs`/`config.rs` while the icon stories touch `icons/`
- Every task marked [P] within a phase touches a different file from its siblings

---

## Parallel Example: User Story 1

```bash
# The three resolution stages are separate files — start them together:
Task: "Implement the desktop-entry index in src/icons/entries.rs"
Task: "Implement icon-set lookup in src/icons/iconset.rs"
Task: "Implement decoding in src/icons/decode.rs"

# Once the pipeline lands, every E2E test is independent:
Task: "E2E test e2e_icons_in_flat_list in tests/e2e_icons.rs"
Task: "E2E test e2e_vector_icon_renders in tests/e2e_icons.rs"
Task: "E2E test e2e_icon_resolved_once_per_program in tests/e2e_icons.rs"
```

---

## Implementation Strategy

### MVP (User Story 1)

1. Phase 1 → Phase 2 → **stop at the Phase 2 checkpoint and confirm the overlay is unchanged**
2. Phase 3
3. Validate US1 independently, then demo

Phase 2's checkpoint is the one worth being strict about. If the overlay is not pixel-identical
after the refactor, something has changed that no requirement asked to change, and every later
comparison against "the pre-feature appearance" (SC-018, SC-019) is built on sand.

### Incremental Delivery

1. Setup + Foundational → the overlay is style-driven and unchanged
2. + US1 → icons in the flat list (**MVP**)
3. + US2 → theme selection; both P1 stories done, a coherent release
4. + US3, US4 → icons everywhere, overrides available
5. + US5, US6 → geometry control and the icon escape hatches
6. Polish

### Suggested deviation from strict priority order

T084 (`icons = false`) sits in P3 because turning icons off is not where the value is. But it is a
few lines, and it is what a user with no usable icon set falls back to. Landing it with Phase 3
costs almost nothing and makes the MVP safe to ship to someone whose desktop has no icon set.
Nothing forces the decision — no earlier task depends on it — so it is a judgement about who the
MVP is safe for, not about whether the tests can run.

---

## Notes

- [P] tasks touch different files and have no dependency on incomplete work
- Unit tests are in-module (`#[cfg(test)]`), so a unit-test task names the same `src/*.rs` file as
  the code it covers — this is why several unit-test tasks are not marked [P] against their
  implementation task
- Every E2E task names the requirement it covers and, where one exists, the acceptance scenario
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently
