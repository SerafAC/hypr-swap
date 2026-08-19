# Tasks: Workspace Swap Overlay

**Input**: Design documents from `/specs/001-workspace-swap-overlay/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Test tasks are REQUIRED (Constitution IV & V). Unit tests live in-module under
`#[cfg(test)]` per Rust idiom, so a unit-test task names the same `src/*.rs` file as the code it
covers. E2E tests are integration tests under `tests/` driving a nested Hyprland instance
(research.md R14). Tests MAY be written after the implementation they cover — test-first ordering
is not required — but a story is not complete until its tests exist and pass.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US5)
- Include exact file paths in descriptions

## Path Conventions

Single Rust binary crate at the repository root: `src/`, `tests/`, `protocols/`, `docs/`
(plan.md → Project Structure). Unit tests are in-module; E2E tests are in `tests/e2e_*.rs` with
shared harness modules in `tests/e2e/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and build plumbing

- [X] T001 Initialize the binary crate in Cargo.toml — name `hypr-swap`, edition 2024, Rust 1.96; dependencies `wayland-client`, `smithay-client-toolkit`, `wayland-protocols-wlr`, `cairo-rs`, `pango`, `pangocairo`, `serde` (derive), `serde_json`, `toml`, `calloop`; build-dependency `wayland-scanner` (plan.md → Primary Dependencies)
- [X] T002 [P] Vendor and pin the protocol XML at protocols/hyprland-global-shortcuts-v1.xml, recording the upstream revision in a header comment (research.md R3)
- [X] T003 Add build.rs at the repository root running `wayland-scanner` over protocols/hyprland-global-shortcuts-v1.xml to generate the client-side bindings (depends on T002)
- [X] T004 [P] Create the module skeleton — src/main.rs declaring `mod config; mod diag; mod model; mod state; mod ordering; mod actions; mod session; mod hypr; mod ui;` with empty src/config.rs, src/diag.rs, src/model.rs, src/state.rs, src/ordering.rs, src/actions.rs, src/session.rs, src/hypr/mod.rs, src/hypr/ipc.rs, src/hypr/events.rs, src/ui/mod.rs, src/ui/shortcuts.rs, src/ui/layout.rs, src/ui/render.rs
- [X] T005 [P] Add rustfmt.toml and a `[lints.clippy]` section in Cargo.toml enabling `pedantic` at warn level
- [X] T006 [P] Add .gitignore at the repository root ignoring `/target`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The daemon shell — it starts, connects to Hyprland, tracks compositor state, receives
its named shortcuts, reconnects, and reports diagnostics. No overlay and no workspace changes yet.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Spikes (gate the rest of the phase)

- [X] T007 Run the R4 spike in examples/spike_modifiers.rs — a throwaway client that maps an overlay layer surface on shortcut `pressed` with exclusive keyboard interactivity and logs every `wl_keyboard.modifiers` event with a timestamp; confirm (a) modifiers arrive on `enter` and on each change including release of a bind's own modifier, and (b) `pressed` → first frame stays inside 150 ms (SC-001). Record the outcome in research.md R4; if (a) fails, switch to the documented `keyboard-shortcuts-inhibit-unstable-v1` fallback
- [X] T008 Bootstrap the nested-compositor spike in tests/e2e/harness.rs — start a nested Hyprland with its own `HYPRLAND_INSTANCE_SIGNATURE` and config, confirm `hyprctl output create headless` works and that `virtual-keyboard-unstable-v1` input is accepted, per research.md R14 [spike]

### Core modules

- [X] T009 [P] Implement `Workspace`, `Monitor`, `Window` and their serde deserialisers for `j/monitors`, `j/workspaces`, `j/clients` in src/model.rs, including the `id < 0` special/scratchpad predicate (data-model.md)
- [X] T010 [P] Unit tests in src/model.rs for the deserialisers against captured JSON fixtures in tests/fixtures/{monitors,workspaces,clients}.json, covering the special-workspace predicate and the zero-size and unmapped window rules
- [X] T011 [P] Implement the `ERROR|WARN|INFO <subject>: <message>` stderr record and the detached `notify-send` spawn with per-process one-shot failure reporting in src/diag.rs (contracts/diagnostics.md, FR-029–FR-032)
- [X] T012 [P] Unit tests in src/diag.rs for record formatting and the notify-policy table (which conditions notify and which do not)
- [X] T013 [P] Implement configuration loading in src/config.rs — `$XDG_CONFIG_HOME/hypr-swap/config.toml` with `~/.config` fallback, the three keys `presentation`/`placement`/`order`, per-setting validation with fallback to that setting's default, unknown-key warning, whole-file parse-error path, and the documented defaults (contracts/config.md, FR-023, FR-024)
- [X] T014 [P] Unit tests in src/config.rs for missing file (silent defaults), each valid value, one invalid value leaving the others honoured, unknown key, and invalid TOML falling back to all defaults
- [X] T015 Implement socket1 request/response in src/hypr/ipc.rs — one connection per request against `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`, `j/*` queries, `/dispatch`, and `[[BATCH]]` encoding (contracts/compositor-ipc.md, research.md R2)
- [X] T016 Unit tests in src/hypr/ipc.rs for request encoding, `[[BATCH]]` composition, and `ok`/error response classification
- [X] T017 [P] Implement `EVENT>>DATA` line parsing for `workspace`, `workspacev2`, `focusedmon`, `createworkspace`, `destroyworkspace`, `moveworkspace`, `openwindow`, `closewindow`, `movewindow`, `windowtitle`, `monitoradded`, `monitorremoved`, ignoring unknown names, in src/hypr/events.rs — with unit tests for each line shape and for unknown-event tolerance
- [X] T018 Implement the persistent `.socket2.sock` connection with exponential backoff reconnect (100 ms doubling to a 5 s cap, indefinite, reset on success) in src/hypr/events.rs, with unit tests for the backoff sequence (FR-026a, FR-026d)
- [X] T019 Implement `World` (monitors, workspaces, windows) with full rebuild from `j/monitors` + `j/workspaces` + `j/clients`, incremental application of each compositor event, and `ActivationHistory` fed only from observed activations, in src/state.rs (data-model.md → World, FR-008c, FR-026)
- [X] T020 Unit tests in src/state.rs for each event→state transition in the data-model table, `push` moving an id to the front without duplicates, removal of destroyed ids, and history cleared on connection loss (FR-008c, FR-008d, FR-026c)
- [X] T021 Implement the Wayland client core in src/ui/mod.rs — connection, registry, binding `wl_compositor`, `wl_shm`, `wl_seat`, `wl_output`, `zwlr_layer_shell_v1`, and the shortcuts manager, with keyboard handling scaffolding; a missing required global is fatal exit 3 (contracts/compositor-ipc.md)
- [X] T022 Register `hypr-swap:switcher` and `hypr-swap:new-workspace` and dispatch their `pressed`/`released` events in src/ui/shortcuts.rs, reporting a registration failure on stderr and as a notification (contracts/shortcuts.md, FR-022, FR-030)
- [X] T023 Implement start-up in src/main.rs — `--config`/`--version`/`--help` parsing, required-environment checks, second-instance detection via `hyprctl globalshortcuts`, and exit codes 0/2/3 (contracts/cli.md, FR-025, FR-025a, FR-033, FR-034)
- [X] T024 Wire the calloop event loop in src/main.rs over the Wayland fd and the Hyprland event socket fd, plus `SIGTERM`/`SIGINT` handling that closes any overlay without committing and exits 0 (contracts/cli.md)
- [X] T025 Implement reconnection orchestration in src/main.rs — on connection loss close any open overlay uncommitted, retry with backoff, and on success rebuild the world, clear the activation history, and re-register both shortcuts, all reported at `INFO` with no notification (FR-026a–d, FR-031)

### E2E harness

- [X] T026 Complete the nested-Hyprland harness in tests/e2e/harness.rs — instance lifecycle, generated config carrying the documented bind lines, `hyprctl output create headless` for extra monitors, IPC state assertions, and teardown (research.md R14)
- [X] T027 [P] Implement `virtual-keyboard-unstable-v1` key injection (press, release, hold-with-taps) in tests/e2e/keyboard.rs
- [X] T028 [P] Implement `foot` toplevel spawning with known titles and geometry in tests/e2e/clients.rs

**Checkpoint**: The daemon starts, tracks state, survives a compositor restart, and receives its
shortcuts. User story implementation can now begin.

---

## Phase 3: User Story 1 - Switch to any workspace with a hold-and-release hotkey (Priority: P1) 🎯 MVP

**Goal**: Holding the switcher combination opens an overlay listing every ordinary workspace with
its windows; tapping moves the highlight; releasing the modifier activates the highlighted
workspace on the current monitor.

**Independent Test**: With three workspaces containing distinct windows, hold the switcher
combination, tap through to the third entry, release, and confirm that workspace is now active and
focused.

### Implementation for User Story 1

- [X] T029 [P] [US1] Implement `entries(world, order) -> (Vec<Entry>, usize)` in src/ordering.rs — MRU (history first, never-active in compositor order, highlight index 1), compositor order and grouped-by-monitor (highlight on the active workspace), with special/scratchpad workspaces filtered out and the single-workspace clamp (FR-007, FR-008a, FR-008b, FR-008d)
- [X] T030 [P] [US1] Unit tests in src/ordering.rs for all three orders, the initial highlight of each, never-active workspaces sorting last, scratchpad exclusion, and the one-workspace clamp
- [X] T031 [P] [US1] Implement the session state machine in src/session.rs — `Open`/`Committed`/`Cancelled`, `AwaitingFocus`/`Focused`/`NeverFocused`, wrapping navigation in both directions, `initial_mods` capture, and the vanished-target rule: a target workspace that no longer exists cancels, while a surviving workspace whose snapshot monitor has gone degrades to plain activation (data-model.md → Switcher Session, FR-003, FR-004, FR-027, FR-028)
- [X] T032 [P] [US1] Unit tests in src/session.rs for every transition in the data-model state diagram, including wrap-around both ways, cancel leaving history untouched, fast-tap commit from `AwaitingFocus`, and connection-loss cancellation
- [X] T033 [P] [US1] Implement list-entry metrics and the viewport/scroll arithmetic in src/ui/layout.rs — fixed row height, the 80 % × 80 % monitor cap, one-entry scroll margin, scale multiplication (FR-019, research.md R16, contracts/config.md constants table)
- [X] T034 [P] [US1] Unit tests in src/ui/layout.rs for viewport arithmetic at several monitor sizes and entry counts including 20 workspaces, proving entries never shrink and the highlight is always in view with its margin (SC-005)
- [X] T035 [US1] Implement `actions::plan` in src/actions.rs for the same-monitor case (`workspace <id>`) and the already-active no-op returning `None` (FR-009, FR-011, research.md R8)
- [X] T036 [US1] Unit tests in src/actions.rs for the same-monitor plan, the `None` no-op case, and the FR-027 degradation — a selected workspace whose snapshot monitor is absent from the current world resolves to same-monitor activation rather than cancelling
- [X] T037 [US1] Implement flat-list painting in src/ui/render.rs with cairo/pango — workspace name followed by its window titles, distinct highlight and active-workspace styling, pango `ellipsize` for overlong text (FR-008, FR-014, FR-015b)
- [X] T038 [US1] Map the overlay layer surface in src/ui/mod.rs — `zwlr_layer_shell_v1` overlay layer, `exclusive` keyboard interactivity, namespace `hypr-swap`, no exclusive zone, shm buffer allocation and damage/commit on navigation; report and abort the session if exclusive focus is refused (FR-002a, FR-018)
- [X] T039 [US1] Implement commit-on-release in src/ui/mod.rs — record `initial_mods` on `wl_keyboard.enter`, commit when a subsequent `modifiers` event shows any of them released, plus the fast-tap path that commits the initial highlight when `released` arrives before focus and never maps the overlay, plus sticky mode when `initial_mods` is empty (FR-002, FR-005, FR-022a, FR-022c, research.md R4, R15)
- [X] T040 [US1] Implement the fixed in-overlay key map in src/ui/mod.rs — `Tab`/`Right`/`Down` next, `Shift+Tab`/`Left`/`Up` previous, `Escape` cancel, `Enter` commit in sticky mode, all other keys ignored (FR-004, FR-004a, FR-006, contracts/shortcuts.md)
- [X] T041 [US1] Make a `switcher` `pressed` event advance the highlight when a session is already open, with no second overlay, in src/ui/shortcuts.rs and its handler in src/main.rs (FR-003, FR-028, research.md R5)
- [X] T042 [US1] Wire the commit path in src/main.rs — session outcome → `actions::plan` resolving the target's monitor from the current world → `hypr::ipc` dispatch; a target workspace that no longer exists is a cancellation reported at `INFO`, a vanished monitor degrades to activation on the focused monitor (FR-005, FR-027)

### Tests for User Story 1 (REQUIRED)

- [X] T043 [P] [US1] E2E `e2e_activate_same_monitor` — hold, tap, release; covers FR-001, FR-002, FR-005, FR-009, US1-AS4 — in tests/e2e_switcher.rs
- [X] T044 [P] [US1] E2E `e2e_mru_order_and_highlight` — overlay opens on MRU default with the current workspace first and the highlight on the second entry; covers FR-008a, FR-008b, FR-008d, US1-AS1/AS2 — in tests/e2e_switcher.rs
- [X] T045 [P] [US1] E2E `e2e_configured_order` — `order = "compositor"`; covers FR-008a, FR-008b, US1-AS3, US5-AS7 — in tests/e2e_switcher.rs
- [X] T046 [P] [US1] E2E `e2e_external_switch_tracked` — switch via a compositor keybind, then open the overlay; covers FR-008c, US1-AS9 — in tests/e2e_switcher.rs
- [X] T047 [P] [US1] E2E `e2e_cancel_leaves_state` — Escape while the modifier is held; covers FR-006, US1-AS5/AS6 — in tests/e2e_switcher.rs
- [X] T048 [P] [US1] E2E `e2e_navigation_wraps_and_reverses` — tap past the last entry, then Shift+Tab; covers FR-003, FR-004, FR-004a, US1-AS8 — in tests/e2e_switcher.rs
- [X] T049 [P] [US1] E2E `e2e_select_active_is_noop` — select the current workspace; covers FR-011, US1-AS7 — in tests/e2e_switcher.rs
- [X] T050 [P] [US1] E2E `e2e_repeat_trigger_advances` — fire the switcher shortcut while the overlay is open; covers FR-003, FR-028 — in tests/e2e_switcher.rs
- [X] T051 [P] [US1] E2E `e2e_fast_tap_commits` — press and release inside 20 ms; covers FR-005, SC-001 — in tests/e2e_switcher.rs
- [X] T052 [P] [US1] E2E `e2e_vanished_target_cancels` — destroy the target workspace while the overlay is open; covers FR-027 — in tests/e2e_switcher.rs
- [X] T053 [P] [US1] E2E `e2e_special_workspaces_excluded` — a scratchpad workspace present; covers FR-007 — in tests/e2e_switcher.rs
- [X] T054 [P] [US1] E2E `e2e_list_shows_window_names` — default presentation entries show workspace name then window titles; covers FR-014, US3-AS6 — in tests/e2e_presentation.rs
- [X] T055 [P] [US1] E2E `e2e_scrolls_many_workspaces` and `e2e_above_fullscreen` — 20 workspaces scroll at fixed entry size, and the overlay renders above a fullscreen client (asserted via `hyprctl layers`); cover FR-019, SC-005, FR-018 — in tests/e2e_presentation.rs
- [X] T097 [P] [US1] E2E `e2e_focus_returns_on_close` — a `foot` client holds focus, the overlay opens and closes, and keyboard focus returns to that client; covers FR-002a — in tests/e2e_switcher.rs
- [X] T098 [P] [US1] E2E `e2e_monitor_removed_degrades` — destroy a headless output holding the highlighted workspace while the overlay is open, then release; the selection resolves to plain activation on the focused monitor rather than cancelling; covers FR-027 and the monitor-disconnected edge case — in tests/e2e_switcher.rs

**Checkpoint**: The switcher is fully usable on a single monitor — this is the MVP.

---

## Phase 4: User Story 2 - Swap workspaces between monitors (Priority: P1)

**Goal**: Selecting a workspace bound to another monitor exchanges the two workspaces between
monitors, atomically, with rollback and reporting on failure.

**Independent Test**: With workspace A active on monitor 1 and B active on monitor 2, select B from
monitor 1 and confirm that after release B is on monitor 1 and active, and A is on monitor 2.

### Implementation for User Story 2

- [X] T056 [US2] Extend `actions::plan` in src/actions.rs with the two cross-monitor shapes — `swapactiveworkspaces <monA> <monB>` + `focusmonitor <monA>` when the target is its monitor's active workspace, and `moveworkspacetomonitor` × 2 + `focusworkspaceoncurrentmonitor` when it is not — each producing an `ExpectedState` and an inverse `RollbackPlan` computed from the pre-state (FR-010, FR-013a, research.md R8)
- [X] T057 [US2] Unit tests in src/actions.rs for both cross-monitor plan shapes, their `ExpectedState`, their generated rollback batches, and the single-monitor degradation to plain activation (FR-009, FR-010, US2-AS5)
- [X] T058 [US2] Unit test in src/actions.rs for the FR-013c double-failure path — an injected failure on both the plan and its rollback yields the "report the resulting state" outcome rather than a silent inconsistency
- [X] T059 [US2] Implement batched dispatch with post-dispatch read-back verification against `ExpectedState` and inverse-batch rollback on mismatch in src/hypr/ipc.rs, asserting the FR-013 post-condition that both affected monitors still show an active workspace (FR-013, FR-013a, SC-010)
- [X] T060 [US2] Add the E2E-only fault-injection hook in src/hypr/ipc.rs, enabled by an environment variable, that fails a nominated step of a batch — the one documented substitution for the rollback tests (research.md R14)
- [X] T061 [US2] Emit the swap diagnostics in src/main.rs via src/diag.rs — `ERROR swap:` with a notification for a rolled-back swap, and the distinct resulting-state message when the rollback itself fails (FR-013b, FR-013c, contracts/diagnostics.md)

### Tests for User Story 2 (REQUIRED)

- [X] T062 [P] [US2] E2E `e2e_swap_active_workspaces` — two headless outputs, target active on the other monitor, asserting no intermediate half-swapped state; covers FR-010, FR-012, FR-013, US2-AS1, and compositor-ipc assumption 4 — in tests/e2e_swap.rs
- [X] T063 [P] [US2] E2E `e2e_swap_inactive_target` — target bound to another monitor but not shown there; covers FR-010, US2-AS2, and confirms the research.md R8 [spike] behaviour of `moveworkspacetomonitor` and post-move focus — in tests/e2e_swap.rs
- [X] T064 [P] [US2] E2E `e2e_swap_single_monitor_degrades` — one output only, selection behaves as plain activation with no error; covers FR-009, US2-AS5 — in tests/e2e_swap.rs
- [X] T065 [P] [US2] E2E `e2e_swap_rollback_on_failure` — fault-injected second step; covers FR-013a, FR-013b, SC-010, US2-AS6 — in tests/e2e_swap.rs
- [X] T066 [P] [US2] E2E `soak` (`#[ignore]`) — 100 consecutive two-monitor swaps comparing the window inventory before and after; covers SC-003, US2-AS4 — in tests/e2e_swap.rs

**Checkpoint**: US1 and US2 both work independently; the product's namesake behaviour is complete.

---

## Phase 5: User Story 3 - Preview workspaces as a grid of miniatures (Priority: P2)

**Goal**: With `presentation = "grid"`, the overlay shows each workspace as a schematic miniature of
its window layout, labelled underneath, with identical navigation and selection behaviour.

**Independent Test**: Set the presentation to grid, open the overlay, and confirm labelled
miniatures appear and that selection and release behave exactly as in the flat list.

### Implementation for User Story 3

- [X] T067 [P] [US3] Implement grid cell metrics (240 × 135 logical px + label line) and the miniature rect mapping `(window.at - monitor.position) / monitor.size` normalised against the monitor the *workspace* is bound to, in src/ui/layout.rs (FR-015a, contracts/config.md constants table)
- [X] T068 [P] [US3] Unit tests in src/ui/layout.rs for miniature normalisation — relative positions and proportions preserved, identical results for a workspace bound to a monitor it is not currently displayed on, zero-size windows skipped (SC-008)
- [X] T069 [US3] Implement miniature painting in src/ui/render.rs — one labelled rectangle per mapped window, floating windows drawn on top in `clients` order, pango-ellipsised titles, workspace name beneath the cell, and a clearly-marked empty miniature for a workspace with no windows (FR-015, FR-015a, FR-015b, FR-007, US3-AS5)
- [X] T070 [US3] Select the presentation from `config.presentation` in src/ui/mod.rs so navigation, commit and cancel paths are shared between list and grid with no duplicated session logic (FR-016, US3-AS4)

### Tests for User Story 3 (REQUIRED)

- [X] T071 [P] [US3] E2E `e2e_grid_miniature_layout` — `presentation = "grid"` with two windows side by side and a third below the second; covers FR-015, FR-015a, SC-008, US3-AS1/AS2 — in tests/e2e_presentation.rs
- [X] T072 [P] [US3] E2E `e2e_grid_offscreen_workspace` — a workspace never displayed on any monitor renders as accurately as a visible one; covers FR-015a, US3-AS3, and compositor-ipc assumption 1 — in tests/e2e_presentation.rs
- [X] T073 [P] [US3] E2E `e2e_grid_empty_workspace` — a workspace with no windows appears as a marked empty miniature; covers FR-007, US3-AS5 — in tests/e2e_presentation.rs
- [X] T074 [P] [US3] E2E `e2e_title_truncation` — a `foot` client with a very long title; covers FR-015b — in tests/e2e_presentation.rs
- [X] T099 [P] [US3] E2E `e2e_grid_commit_matches_list` — the same navigation and release gesture under `presentation = "grid"` produces the identical activation and swap outcome as the list; covers FR-016, US3-AS4 — in tests/e2e_presentation.rs

**Checkpoint**: Both presentations work with identical interaction.

---

## Phase 6: User Story 4 - Create a new empty workspace on the current monitor (Priority: P2)

**Goal**: The new-workspace shortcut switches to the lowest unused workspace number bound to the
focused monitor, and is a no-op when the active workspace is already empty.

**Independent Test**: Press the new-workspace combination and confirm the lowest unused workspace
number is active on the current monitor and appears in the overlay on the next invocation.

### Implementation for User Story 4

- [ ] T075 [US4] Implement `actions::new_workspace_plan(world) -> Option<CommandPlan>` in src/actions.rs — lowest positive integer not among the known workspace ids, dispatched as `focusworkspaceoncurrentmonitor <n>`, returning `None` when the focused monitor's active workspace has `window_count == 0` (FR-020, FR-021, research.md R9)
- [ ] T076 [US4] Unit tests in src/actions.rs for lowest-unused selection with gaps (1, 2, 4 → 3), the empty-workspace `None` case, and that no diagnostic is produced for the no-op
- [ ] T077 [US4] Handle the `new-workspace` shortcut in src/main.rs — dispatch the plan, never open an overlay, ignore the `released` event (contracts/shortcuts.md)

### Tests for User Story 4 (REQUIRED)

- [ ] T078 [P] [US4] E2E `e2e_new_workspace_lowest_unused` — workspaces 1, 2, 4 in use on a two-output setup, shortcut fired from the second monitor, then confirm the new workspace appears in the overlay; covers FR-020, US4-AS1/AS3 — in tests/e2e_new_workspace.rs
- [ ] T079 [P] [US4] E2E `e2e_new_workspace_noop_when_empty` — repeat press on the now-empty workspace changes nothing and creates nothing; covers FR-021, US4-AS2, SC-007 — in tests/e2e_new_workspace.rs

**Checkpoint**: All P1 and P2 stories are independently functional.

---

## Phase 7: User Story 5 - Bind shortcuts and configure overlay presentation (Priority: P3)

**Goal**: The documented bind lines work as written, and the three configuration settings take
effect, with invalid values reported and defaulted rather than fatal.

**Independent Test**: Bind the shortcuts in the compositor configuration, change each setting in the
configuration file, restart the application, and confirm the new bindings, placement and
presentation take effect.

### Implementation for User Story 5

- [ ] T080 [US5] Implement `placement = "all"` in src/ui/mod.rs — one layer surface per connected monitor driven by a single session so every copy shows the same highlight, with exclusive keyboard interactivity on the focused monitor's surface only (FR-017, US5-AS2/AS3)
- [ ] T081 [P] [US5] Write docs/binds.md with the exact bind lines, the `bind` vs `binde` rule, the unbound-shortcut guarantee, and the bare-key sticky-mode note (FR-022b, contracts/shortcuts.md)
- [ ] T082 [P] [US5] Make `--help` in src/main.rs print usage including the bind lines from docs/binds.md so the two never drift (contracts/cli.md, FR-033, Principle III)
- [ ] T083 [US5] Extend the src/config.rs unit tests to cover the exact stderr subjects (`config.presentation`, `config.placement`, `config.order`) and the notify flag on each fallback, matching contracts/diagnostics.md

### Tests for User Story 5 (REQUIRED)

- [ ] T084 [P] [US5] E2E `e2e_placement_all_monitors` — `placement = "all"` with two outputs, asserting a `hypr-swap` layer on each and the same highlight; covers FR-017, US5-AS2/AS3 — in tests/e2e_config.rs
- [ ] T085 [P] [US5] E2E `e2e_defaults_without_config` — no configuration file present; covers FR-023, SC-006, US5-AS4 — in tests/e2e_config.rs
- [ ] T086 [P] [US5] E2E `e2e_invalid_config_falls_back` — `presentation = "tiles"` with `order = "compositor"` honoured; covers FR-024, FR-029, FR-030, US5-AS5 — in tests/e2e_config.rs
- [ ] T087 [P] [US5] E2E `e2e_unbound_shortcut_is_harmless` — only the new-workspace bind present; covers FR-022b, US5-AS6 — in tests/e2e_config.rs
- [ ] T088 [P] [US5] E2E `e2e_no_compositor_at_start` — no `HYPRLAND_INSTANCE_SIGNATURE`, expecting the stderr record, the notification, and exit 3; covers FR-025 — in tests/e2e_config.rs
- [ ] T089 [P] [US5] E2E `e2e_reconnects_after_restart` — kill and restart the nested Hyprland, asserting the shortcuts work again within 10 s, the history is rebuilt, and no notification was raised; covers FR-026a, FR-026b, FR-026c, FR-031, SC-009 — in tests/e2e_config.rs
- [ ] T090 [P] [US5] E2E `e2e_no_notification_daemon` — no notification service reachable, asserting one `WARN notify:` line and normal operation; covers FR-032 — in tests/e2e_config.rs
- [ ] T100 [P] [US5] E2E `e2e_no_overlay_while_disconnected` — with the compositor connection down, firing the switcher shortcut maps no layer surface and consumes no CPU spinning; covers FR-026d — in tests/e2e_config.rs

**Checkpoint**: Every user story is complete and independently testable.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T091 [P] Write README.md — what the tool does, prerequisites, build, the bind lines (linking docs/binds.md), and the configuration schema
- [ ] T092 Measure and record the SC-001 (≤150 ms shortcut → overlay) and SC-002 (≤300 ms release → target workspace visible) budgets against the nested instance, and confirm 0 % idle CPU with no overlay open
- [ ] T093 Audit the plan.md E2E coverage table against the implemented tests — confirm every FR has at least one E2E test or a documented reason (FR-013c unit-only; SC-003/SC-004/SC-007 manual), per Constitution V
- [ ] T094 [P] Make `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean across src/ and tests/
- [ ] T095 Run the quickstart.md manual validation scenarios 1–12 against a live session and record the results
- [ ] T096 [P] Remove the examples/spike_modifiers.rs spike now that its findings are recorded in research.md R4 (Principle II — no dead code paths)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories. Within it, T007 gates T038/T039 and T008 gates T026
- **User Story 1 (Phase 3)**: Depends on Phase 2 only
- **User Story 2 (Phase 4)**: Depends on Phase 2; shares src/actions.rs and the commit path with US1, so it is easiest after US1 but its plans and rollback are independently testable
- **User Story 3 (Phase 5)**: Depends on Phase 2; extends src/ui/layout.rs and src/ui/render.rs which US1 creates
- **User Story 4 (Phase 6)**: Depends on Phase 2 only — genuinely independent of US1–US3 (no overlay involved)
- **User Story 5 (Phase 7)**: Depends on Phase 2; T080 extends the surface management US1 builds
- **Polish (Phase 8)**: Depends on all desired stories

### User Story Dependencies

- **US1 (P1)**: No dependencies on other stories — the MVP
- **US2 (P1)**: Independent decision logic; reuses US1's commit path for its E2E scenarios
- **US3 (P2)**: Independent presentation; reuses US1's session and navigation unchanged
- **US4 (P2)**: Fully independent — implementable straight after Phase 2
- **US5 (P3)**: Independent; its `placement = "all"` work touches the same file as US1's surface mapping

### Within Each User Story

- Tests MAY come before or after the code they cover, but MUST pass before the story is done
- Pure modules (`ordering`, `session`, `actions`, `ui/layout`) before the shell that calls them
- Rendering before surface wiring; surface wiring before the commit path
- Story complete before moving to the next priority

### Parallel Opportunities

- Setup: T002, T004, T005, T006 in parallel (T003 waits on T002)
- Foundational: T009/T011/T013/T017 in parallel (distinct files), then T021/T022/T023 in parallel, and T027/T028 in parallel once T026 lands
- US1: T029–T034 in parallel (three distinct files, pure logic); all fifteen E2E tasks T043–T055, T097 and T098 in parallel once T042 lands
- US2: T062–T066 in parallel once T059/T060 land
- US3: T067/T068 in parallel; T071–T074 in parallel once T070 lands
- US5: T081/T082 in parallel; T084–T090 in parallel
- Across stories: after Phase 2, US4 can be built alongside US1 by a second developer with no file conflicts

---

## Parallel Example: User Story 1

```bash
# Launch the pure-logic modules together (three distinct files):
Task: "Implement entries() in src/ordering.rs"
Task: "Implement the session state machine in src/session.rs"
Task: "Implement list metrics and viewport arithmetic in src/ui/layout.rs"

# Launch their unit tests together:
Task: "Unit tests in src/ordering.rs for all three orders"
Task: "Unit tests in src/session.rs for every transition"
Task: "Unit tests in src/ui/layout.rs for viewport arithmetic"

# Once T042 lands, launch the switcher E2E suite together:
Task: "E2E e2e_activate_same_monitor in tests/e2e_switcher.rs"
Task: "E2E e2e_mru_order_and_highlight in tests/e2e_switcher.rs"
Task: "E2E e2e_cancel_leaves_state in tests/e2e_switcher.rs"
Task: "E2E e2e_navigation_wraps_and_reverses in tests/e2e_switcher.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational — the R4 and R14 spikes first, since the whole interaction rests
   on them
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: run `cargo test --lib` and `cargo test --test e2e_switcher`, then
   quickstart.md scenarios 1–4 against a live session
5. This is a usable Alt-Tab workspace switcher on its own

### Incremental Delivery

1. Setup + Foundational → a daemon that connects, tracks state and reconnects
2. + US1 → the switcher works (MVP)
3. + US2 → cross-monitor swapping, the product's namesake
4. + US4 → new-workspace shortcut (small, independent, ships any time after Phase 2)
5. + US3 → grid miniatures
6. + US5 → configuration and documented binds
7. Polish

### Parallel Team Strategy

1. The team completes Setup + Foundational together — the spikes gate everything
2. Then: Developer A on US1 (the largest story), Developer B on US4 then US2, Developer C on the
   E2E harness scenarios and US5
3. US3 starts once US1's `ui/layout.rs` and `ui/render.rs` exist

---

## Notes

- Unit tests are in-module `#[cfg(test)]` blocks, so a unit-test task names the same `src/*.rs` file
  as the code it covers; they are separate tasks so the constitution's coverage requirement stays
  visible per component
- `main.rs`, `ui/mod.rs`, `ui/shortcuts.rs` and `ui/render.rs` are the deliberately logic-free shell
  and are covered by E2E rather than unit tests — a documented deviation from Principle IV, recorded
  in plan.md → Complexity Tracking. `hypr/ipc.rs` and `hypr/events.rs` are **not** exempt and have
  unit tests (T016–T018)
- Every E2E task names the FR or acceptance scenario it covers, per Constitution V
- The two documented E2E substitutions are headless outputs for physical monitors and `foot` for
  arbitrary applications; the fault-injection hook (T060) is the third and is confined to the
  rollback tests (research.md R14)
- Commit after each task or logical group; the full suite must pass before a change is complete

### Implementation notes recorded during Phase 1–2

Two tasks were satisfied differently from their literal wording. Both are recorded here rather
than applied silently.

- **T003** — `wayland-scanner` 0.31 is a *procedural macro* crate, not a build-script generator:
  there is no `generate_code`/`Side` API to call from `build.rs`. The codegen therefore expands
  in `src/ui/shortcuts.rs` via `generate_interfaces!`/`generate_client_code!`, and `build.rs`
  keeps its reason to exist — declaring the vendored XML with `cargo::rerun-if-changed`, which
  Cargo cannot infer from a macro that reads a file.
- **T004** — the modules are declared in a `src/lib.rs` rather than in `src/main.rs`, with
  `main.rs` keeping exactly the responsibilities its own tasks name (start-up, wiring, the event
  loop, signals, the commit path). This is still one package and one crate, so plan.md's
  Structure Decision — no library/binary *package* split — holds; it is what makes
  quickstart.md's documented `cargo test --lib` work and lets the E2E tests reuse `model` and
  `ipc` for their assertions.

### Implementation notes recorded during Phase 3

- **T040** — the in-overlay key *table* lives in `session.rs` as `action_for(keysym, shift)`, not
  in `src/ui/mod.rs` where the task names it; `ui/mod.rs` keeps only the lookup and the repaint.
  A keysym→action mapping is a decision rule, and the architecture's one rule for the shell is
  that decision rules live in the pure, unit-tested modules. The behaviour is where the task
  asked for it; the table is where it can be tested.
- **`ui/layout.rs` beyond T033's wording** — the module also gained `text_height` (so the renderer
  sizes type from the same geometry as the rows), `rows_that_fit` and `refit`. The last exists
  because a compositor may configure a layer surface at a size other than the one requested;
  without refitting, the row *count* would stay stale and the overlay would paint rows outside the
  surface it agreed to. Refitting changes only how many rows are shown, never their height
  (FR-019).
- **`render::list` paints into the shm canvas directly** via cairo's `create_for_data_unsafe`
  rather than building a buffer and copying it, which keeps a redraw free of a full-overlay
  memcpy. The single `unsafe` block carries its safety argument.
- **Cross-monitor selections currently take the same-monitor shape.** `actions::plan` resolves
  every selection to `workspace <id>` until T056 adds the two cross-monitor shapes. This is the
  documented incremental delivery — US1 is specified against the single-monitor case — not an
  oversight.
- **Two gaps in the Phase 2 E2E harness were closed here**, both found by Phase 3 scenarios:
  `Setup::app_config` was stored but never written to disk, so no test could have exercised a
  configuration setting; and `overlay_surfaces()` was added alongside `overlay_monitors()` because
  FR-018 and FR-019 need the surface's stacking level and geometry, not just its presence.
- **`keyboard::tap_while_held` now holds the key for 60 ms**, with a new `tap_fast` for the
  deliberate FR-005 case. Injecting press and release with no interval makes the compositor
  deliver a bind's `pressed` and `released` in one batch, so the overlay never gets the round trip
  it needs to be focused and *every* gesture took the fast-tap path — a race no human keyboard can
  produce. This was a harness defect, not an application one: the R4 spike measured focus arriving
  at 6 ms, comfortably inside any real tap.
- **`e2e_scrolls_many_workspaces` opens the overlay on a 640×480 headless output.** Twenty rows
  fit inside 80 % of a full-size monitor, so a scenario run there would assert FR-019's "entries
  never shrink" half without ever exercising its scrolling half.

### Implementation notes recorded during Phase 4

- **The R8 [spike] ran first and changed its plan table.** `moveworkspacetomonitor` carries
  keyboard focus to the destination when the workspace being moved is the focused monitor's
  active one, so the documented third row's final `focusworkspaceoncurrentmonitor <sel>` ran on
  the *wrong* monitor and dragged the selection straight back. The row now has an explicit
  `focusmonitor <monA>` before it. Full findings are in research.md R8 → Spike outcome.
- **The rollback is a pre-state restore, not the inverse of the forward commands** (T056). The
  same spike showed a `[[BATCH]]` is not a transaction: a rejected step leaves its predecessors
  applied. An inverse batch assumes the plan landed in full, which is false in exactly the case
  the rollback exists for, so `RollbackPlan` instead drives the compositor to the recorded
  pre-state — bindings, then actives, then focus — from wherever it actually is. Bindings go
  first because `focusworkspaceoncurrentmonitor` moves a workspace bound elsewhere.
- **`CommandPlan.rollback` changed type** from `Vec<String>` to a `RollbackPlan` carrying the
  pre-state, because FR-013c needs something to verify the rollback *against*. The same-monitor
  activation keeps its one-command literal inverse: a single dispatch cannot half-apply.
- **The FR-013b/FR-013c decision is a pure function** (`ipc::classify`) fed by the I/O around it,
  so the double-failure arm — which no live compositor can be made to produce — is unit-tested
  rather than merely reasoned about. T058's half of it lives in `actions.rs`, where the plan and
  its rollback are both shown to be unsatisfied by the same half-swapped world.
- **The fault injection is one-shot per process** (T060), shared across `Ipc` clones through an
  `Arc<AtomicBool>`. Sabotaging every batch would sabotage the rollback too, leaving FR-013b —
  the case where the undo works — with no way to be tested at all.
- **T062 uses the nested instance's own output plus one headless output**, rather than the two
  headless outputs the task text names. That is two monitors either way, and it keeps the
  scenario's monitor names stable across the suite.
- **SC-010 is asserted by sampling, not by inference.** `Nested::sample_layout` watches the
  compositor from a background thread for the whole gesture and the test asserts every layout it
  saw was either the one before or the one after — plus that it saw both, so "nothing bad was
  observed" is not a claim about an idle sampler.
- **The soak found a real defect in `state.rs`, and it is fixed here.** `moveworkspace` was
  applied incrementally as a rebinding, but the event says nothing about what either monitor is
  now *showing*: the destination may switch to the moved workspace and the vacated monitor falls
  back to one the event does not name. Both `active_workspace` fields therefore went stale after
  a swap, and the very next selection looked like the FR-011 no-op and silently did nothing —
  every pass after the first. `moveworkspace` now asks for a full rebuild, like `createworkspace`
  and `monitoradded` already did for the same reason; data-model.md's transition table is
  updated to match.
- **The soak runs under `order = "compositor"`.** Under MRU a swap activates *both* workspaces,
  and which one the history puts first decides whether the next gesture swaps or is a legitimate
  no-op — not something a hundred-pass loop can predict. Compositor order fixes the entry list,
  so "open, advance once, release" selects the other monitor's workspace every time.

### Defect fixes recorded after Phase 4

- **The overlay was sized in the wrong unit system on a scaled monitor (FR-019).**
  `ui/layout.rs` produced device-pixel metrics — correctly, that is what the shm buffer needs —
  and `ui/mod.rs` passed them straight to `zwlr_layer_surface_v1::set_size`, which takes *logical*
  pixels. The two coincide only at scale 1, so the whole E2E suite passed while the overlay came
  out `scale` times too large on any monitor with a scale factor: a 4K panel at scale 2 got a
  3072×264 surface on a 1920×1080 logical desktop instead of 1536×132.

  `Metrics` now carries both sizes (`surface_size()` logical, `buffer_size()` device) plus the
  scale that relates them, `refit` takes the compositor's logical `configure` size, and the ratio
  is declared to the compositor with a `wp_viewport` per overlay surface. FR-019's 80 % cap is
  taken on the logical desktop, which is what the user actually sees. Decision and rejected
  alternatives are in research.md R17; `wp_viewporter` is added to the required-globals table in
  contracts/compositor-ipc.md.

- **`e2e_overlay_scales_with_the_monitor` measures the same 3840×2160 output twice**, at scale 1
  and at scale 2, and asserts the scaled pass is indistinguishable from a real 1920×1080 monitor.
  It restarts the daemon between the two: Hyprland emits no event for a scale change made with
  `hyprctl keyword monitor`, so a daemon that outlived the change would size the overlay from the
  scale it cached at start-up (research.md R17 → Note on staleness).

### Implementation notes recorded during Phase 5

- **`Metrics` now describes both presentations, and a list is the one-column degenerate grid.**
  FR-016 requires identical navigation and selection, and the cheapest way to guarantee that is
  for there to be one implementation rather than two that must be kept in step. So `row_rect`
  became `cell_rect` (a row is a cell that fills the surface), `visible_rows` counts rows of
  entries with `visible_entries()` beside it, and `first_visible_entry` wraps the existing
  `first_visible` — applied to *rows* — so a cell keeps its column as the viewport scrolls. The
  list path is arithmetically unchanged, which
  `the_list_viewport_is_unchanged_by_the_shared_grid_arithmetic` asserts directly.

- **`render::list` became `render::overlay`** and branches on `metrics.presentation` for one thing
  only: how a single entry is drawn. The backdrop, the viewport slice and the highlight are shared
  (T070). `ui/mod.rs` chooses between `list_metrics` and `grid_metrics` in `open_session` and
  nothing else in the session, commit or cancel paths knows which presentation is in use.

- **The miniature is letterboxed to its monitor's aspect ratio inside the fixed cell**, which
  T067's wording does not ask for. FR-015a asks for the proportion a window *occupies*, and
  normalising a 4:3 workspace into a 16:9 cell would stretch every window in it — the requirement
  would be false for any monitor that is not 16:9. The documented 240 × 135 cell is unchanged;
  `layout::miniature_area` fits the monitor's shape inside it and centres it, and the E2E
  scenarios never exercise this because a nested Hyprland's outputs are all 16:9.

- **`miniature_rect` returns `Option` and clamps to the miniature.** A zero-size window is not a
  rectangle and SC-008 counts one rectangle per window, so it is declined rather than painted as a
  degenerate sliver; a window overhanging its monitor is clipped to the panel rather than allowed
  to paint over the neighbouring cell.

- **`e2e_grid_miniature_layout` runs on a headless 1920 × 1080 output.** The nested instance's own
  output is a window on the developer's session and comes up taller than it is wide, so Hyprland's
  default layout splits horizontally first and the side-by-side arrangement US3-AS2 describes never
  occurs there. The three roles are then identified from the geometry the compositor *reported*
  rather than from the order the windows were spawned in — Hyprland puts a new window in the
  opposite half from what the spawn order suggests, and the scenario is about the arrangement
  surviving the mapping, not about which window is where.

- **`e2e_grid_empty_workspace` declares `workspace = 5, persistent:true`.** Hyprland destroys an
  ordinary workspace the moment its last window closes, so an empty workspace cannot otherwise be
  made to exist for the overlay to list (FR-007, US3-AS5).

- **FR-015b is asserted by what truncation is *not*.** Pango's ellipsis is a pixel and research.md
  R14 rejects screenshot comparison, so `e2e_title_truncation` asserts the two halves of the
  requirement's "rather than": the overlong title is not omitted — it reaches the renderer whole
  and its window still gets a rectangle — and it does not overflow, because neither the overlay
  nor the rectangle inside it grows by a pixel to accommodate it.
