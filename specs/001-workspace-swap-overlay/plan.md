# Implementation Plan: Workspace Swap Overlay

**Branch**: `001-workspace-swap-overlay` | **Date**: 2026-07-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-workspace-swap-overlay/spec.md`

## Summary

A long-running user-session daemon that gives Hyprland an Alt-Tab-style workspace switcher with
cross-monitor swapping. The user binds two named actions in their own `hyprland.conf`; the daemon
registers those names through `hyprland-global-shortcuts-v1`. On trigger it maps a
`wlr-layer-shell` surface on the overlay layer with exclusive keyboard interactivity, which is what
lets it observe the modifier release directly and commit on release (FR-022a). All compositor state
(workspaces, windows, monitors) is read from, and all changes are dispatched to, the Hyprland IPC
sockets — no screen capture, no key grabbing. Rendering is software: a shm buffer painted with
cairo/pango, which covers both the flat list and the schematic miniature grid, including ellipsised
window titles.

The design keeps every decision rule (ordering, selection outcome, rollback, new-workspace
resolution) in pure functions with no I/O, so the interesting logic is unit-testable without a
compositor, and the E2E suite drives a nested Hyprland instance through real key events.

## Technical Context

**Language/Version**: Rust 1.96 (edition 2024) — installed and verified on this machine

**Primary Dependencies**:

- `wayland-client`, `smithay-client-toolkit` (registry, seat/keyboard, shm, `wlr-layer-shell`),
  `wayland-protocols-wlr`
- `wayland-scanner` build-time codegen over a vendored `hyprland-global-shortcuts-v1.xml`
- `cairo-rs` + `pango` + `pangocairo` (system libs present: cairo 1.18.4, pango 1.58.0)
- `serde`, `serde_json` (Hyprland IPC responses), `toml` (configuration)
- `calloop` (single event loop over Wayland fd + Hyprland event socket fd)
- No Hyprland client crate — the IPC protocol is line-oriented text and is implemented directly
  (see [research.md](./research.md) R2)

**Storage**: N/A. Configuration is a single TOML file read at start-up; activation history is
in-memory and session-scoped by design (spec Assumptions).

**Testing**: `cargo test`. Unit tests live in-module (`#[cfg(test)]`), Rust's idiom. E2E tests are
integration tests under `tests/` that launch a **nested Hyprland instance** with headless outputs,
inject real key events through `virtual-keyboard-unstable-v1`, and assert compositor state through
the nested instance's IPC socket ([research.md](./research.md) R14).

**Target Platform**: Linux / Wayland / Hyprland ≥ 0.55 (developed and validated against 0.55.4).
Verified available on the target: `hyprland-global-shortcuts-v1`, `wlr-layer-shell-unstable-v1`,
`virtual-keyboard-unstable-v1`, and the `swapactiveworkspaces`, `moveworkspacetomonitor`,
`focusworkspaceoncurrentmonitor`, `focusmonitor`, `workspace` dispatchers.

**Project Type**: Single Rust binary — a background daemon that owns a Wayland presentation layer.

**Performance Goals**: Overlay pixels on screen ≤150 ms after the shortcut fires (SC-001); target
workspace visible ≤300 ms after modifier release (SC-002); 0 % CPU when idle (event-driven, no
polling); no per-frame work while the overlay is closed.

**Constraints**: No screen capture (FR-015a). No key grabbing (FR-022). Must keep running across
compositor restarts (FR-026a). Must run without a notification daemon (FR-032). Must run with no
configuration file (FR-023) and with its shortcuts unbound (FR-022b).

**Scale/Scope**: 1–4 monitors, ≥20 workspaces, a few hundred windows; ~2500 lines of Rust across
the modules listed below.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Gates derived from `.specify/memory/constitution.md` (v1.0.0).

**Initial evaluation (pre-Phase 0)**: PASS with two items flagged for research — the rendering
stack (risk of pulling in a GUI toolkit) and the swap execution strategy (risk of a bespoke
transaction layer). Both were resolved in Phase 0 without new abstractions.

**Post-design re-evaluation (post-Phase 1)**:

- [x] **I. KISS**: PASS. The design adds no framework and no indirection layer. The compositor
      client is a direct socket write/read rather than a client crate; the transaction story is
      "one batch, verify, one inverse batch" rather than a generic transaction engine; the UI is a
      pixel buffer painted by cairo rather than a widget toolkit and its retained tree. Each
      alternative that would have added a layer is recorded in [research.md](./research.md) with
      why it was rejected. No entries in Complexity Tracking.
- [x] **II. YAGNI**: PASS. Configuration is exactly the three settings FR-023 names — no theming,
      no key remapping, no live reload, no plugin surface, all of which the spec puts out of scope.
      Every module below traces to requirements; the trace is in
      [contracts/README.md](./contracts/README.md). The one behaviour not directly demanded by a
      requirement — sticky mode when the shortcut is bound without a modifier
      ([research.md](./research.md) R15) — exists because commit-on-release is undefined in that
      configuration, and is documented as a decision rather than added silently.
- [x] **III. DRY**: PASS. Each piece of knowledge has one home: the defaults live only in
      `config.rs`; entry ordering only in `ordering.rs`; the swap decision rule only in
      `actions.rs`; the entry/viewport geometry only in `ui/layout.rs`; every diagnostic string
      passes through `diag.rs`. The two presentations share one navigation path and one session
      state machine — `ui/render.rs` differs only in how an entry is painted.
- [x] **IV. Unit tests**: PASS with a documented deviation. The decision logic is I/O-free and
      unit-tested directly: `ordering.rs`, `actions.rs` (including the generated rollback plan),
      `session.rs`, `config.rs`, `ui/layout.rs`, `hypr/events.rs` (line parsing) and the IPC
      response deserialisers. `main.rs` and the Wayland/cairo modules are the thin, deliberately
      logic-free shell and are covered by E2E instead. This is a deviation from Principle IV,
      recorded in Complexity Tracking below with its rationale and the rejected alternative; the
      rationale is expanded in [research.md](./research.md) R14. Note `hypr/ipc.rs` and
      `hypr/events.rs` are unit-tested and are **not** part of the exemption.
- [x] **V. E2E coverage**: PASS. Every major requirement maps to at least one E2E test that drives
      the real external interface (a compositor bind → a real key press → the compositor's own
      reported state). The mapping is the table below.

### E2E coverage mapping

| E2E test | Drives | Covers |
|---|---|---|
| `e2e_activate_same_monitor` | bind → hold, tap, release | FR-001, FR-002, FR-005, FR-009, FR-022a, US1-AS4 |
| `e2e_mru_order_and_highlight` | overlay open on MRU default | FR-008a, FR-008b, FR-008d, US1-AS1/2 |
| `e2e_configured_order` | `order = "compositor"` | FR-008a, FR-008b, US1-AS3, US5-AS7 |
| `e2e_external_switch_tracked` | compositor keybind, then overlay | FR-008c, FR-026, US1-AS9 |
| `e2e_cancel_leaves_state` | Escape while held | FR-006, US1-AS5/6 |
| `e2e_navigation_wraps_and_reverses` | Tab past end, Shift+Tab | FR-003, FR-004, FR-004a, US1-AS8 |
| `e2e_select_active_is_noop` | select current workspace | FR-011, US1-AS7 |
| `e2e_swap_active_workspaces` | two headless outputs | FR-010, FR-012, FR-013, US2-AS1 |
| `e2e_swap_inactive_target` | target bound elsewhere, not shown | FR-010, US2-AS2 |
| `e2e_swap_single_monitor_degrades` | one output | FR-009, US2-AS5 |
| `e2e_swap_rollback_on_failure` | fault-injected second step | FR-013a, FR-013b, SC-010, US2-AS6 |
| `e2e_list_shows_window_names` | default presentation | FR-014, US3-AS6 |
| `e2e_grid_miniature_layout` | `presentation = "grid"`, 3 windows | FR-015, FR-015a, SC-008, US3-AS1/2 |
| `e2e_grid_offscreen_workspace` | workspace never displayed | FR-015a, US3-AS3 |
| `e2e_grid_empty_workspace` | workspace with no windows | FR-007, US3-AS5 |
| `e2e_title_truncation` | very long window title | FR-015b |
| `e2e_placement_all_monitors` | `placement = "all"` | FR-017, US5-AS2/3 |
| `e2e_above_fullscreen` | fullscreen client | FR-018 |
| `e2e_scrolls_many_workspaces` | 20 workspaces | FR-019, SC-005 |
| `e2e_overlay_scales_with_the_monitor` | 4K output at scale 1, then at scale 2 | FR-019, research.md R17 |
| `e2e_new_workspace_lowest_unused` | new-workspace bind | FR-020, US4-AS1/3 |
| `e2e_new_workspace_noop_when_empty` | repeat press | FR-021, US4-AS2 |
| `e2e_defaults_without_config` | no config file | FR-023, SC-006, US5-AS4 |
| `e2e_invalid_config_falls_back` | bad value in TOML | FR-024, FR-029, FR-030, US5-AS5 |
| `e2e_unbound_shortcut_is_harmless` | only one bind present | FR-022b, US5-AS6 |
| `e2e_sticky_mode_commits_on_enter` | switcher bound to a modifierless key | FR-022c |
| `e2e_second_instance_refuses_to_start` | a daemon already holding the names | FR-025a |
| `e2e_version_and_help` | `--version`, `--help`, an unknown flag | FR-033, FR-030 (no notification) |
| `e2e_explicit_config_path_is_used_and_must_exist` | `--config` present, then absent | FR-034 |
| `e2e_no_compositor_at_start` | no `HYPRLAND_INSTANCE_SIGNATURE` | FR-025 |
| `e2e_reconnects_after_restart` | kill and restart nested Hyprland | FR-026a, FR-026b, FR-026c, SC-009 |
| `e2e_repeat_trigger_advances` | shortcut fired while open | FR-003, FR-028 |
| `e2e_fast_tap_commits` | press and release inside 20 ms | FR-005, SC-001 |
| `e2e_vanished_target_cancels` | destroy target while open | FR-027 |
| `e2e_no_notification_daemon` | no notification service on the bus | FR-032 |
| `e2e_special_workspaces_excluded` | scratchpad present | FR-007 |
| `e2e_focus_returns_on_close` | overlay opens over a focused client, then closes | FR-002a |
| `e2e_grid_commit_matches_list` | `presentation = "grid"`, navigate and release | FR-016, US3-AS4 |
| `e2e_no_overlay_while_disconnected` | shortcut fired with the compositor gone | FR-026d |
| `e2e_monitor_removed_degrades` | destroy a headless output while the overlay is open | FR-027, US2 edge case |
| `the_application_registers_both_named_shortcuts` | a started daemon, seen through `hyprctl globalshortcuts` | FR-022 |
| `a_held_modifier_with_taps_is_delivered_as_one_gesture` | hold, tap, tap, release through the virtual keyboard | FR-022a |

Requirements deliberately not E2E-covered: **FR-013c** (rollback itself fails) is unit-tested
against an injected double failure, because provoking it end-to-end would require corrupting the
compositor; **FR-008** (the overlay indicates the highlighted entry and the active workspace) is a
purely visual property, and screenshot comparison is rejected in [research.md](./research.md) R14 as
brittle across fonts and scaling — it is covered by `ui/layout.rs` unit tests for the highlight
index and by manual quickstart validation; **FR-030's shortcut-registration-failure notification**
cannot be provoked against a compositor that accepts the registration, so the notification path is
unit-tested in `diag.rs` and the registration path is exercised positively by every switcher E2E
test; **FR-031** (recovery is stderr-only, no notification) is asserted as part of
`e2e_reconnects_after_restart`; **SC-003**, **SC-004** and **SC-007** are longevity/usability
criteria measured by the soak and usability checks described in
[quickstart.md](./quickstart.md), not by an automated assertion.

## Project Structure

### Documentation (this feature)

```text
specs/001-workspace-swap-overlay/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── README.md        # Contract index + requirement trace
│   ├── shortcuts.md     # Named global shortcuts, bind lines, in-overlay keys
│   ├── config.md        # Configuration file schema and defaults
│   ├── cli.md           # Binary invocation and exit codes
│   ├── diagnostics.md   # stderr format and notification policy
│   └── compositor-ipc.md# The Hyprland IPC surface this app depends on
├── checklists/
│   └── requirements.md  # Pre-existing
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
Cargo.toml
build.rs                     # wayland-scanner codegen for the vendored protocol
protocols/
└── hyprland-global-shortcuts-v1.xml   # vendored, pinned

src/
├── main.rs                  # start-up, wiring, calloop event loop, signal handling
├── config.rs                # TOML load, per-setting validation and fallback, defaults
├── diag.rs                  # stderr records + notification policy (FR-029..032)
├── model.rs                 # Workspace, Monitor, Window, Configuration; IPC deserialisation
├── state.rs                 # World state + activation history; applies compositor events
├── ordering.rs              # entry order and initial highlight (FR-008a/b/d)
├── actions.rs               # pure: selection → CommandPlan + RollbackPlan; new-workspace plan
├── session.rs               # pure: switcher session state machine (open/navigate/commit/cancel)
├── hypr/
│   ├── mod.rs
│   ├── ipc.rs               # socket1: request/response, batch dispatch
│   └── events.rs            # socket2: event stream, line parsing, backoff reconnect
└── ui/
    ├── mod.rs               # Wayland client: registry, seat/keyboard, layer surfaces, shm
    ├── shortcuts.rs         # hyprland-global-shortcuts-v1 registration and events
    ├── layout.rs            # pure: entry metrics, viewport/scroll, miniature rect mapping
    └── render.rs            # cairo/pango painting of list entries and miniatures

tests/
├── e2e/
│   ├── harness.rs           # nested Hyprland lifecycle, headless outputs, IPC assertions
│   ├── keyboard.rs          # virtual-keyboard-unstable-v1 injection helper
│   ├── clients.rs           # spawns `foot` toplevels with known titles/geometry
│   └── notify.rs            # recording `notify-send` stub on the daemon's PATH
├── e2e_harness.rs           # the harness's own self-tests
├── e2e_switcher.rs          # US1 scenarios
├── e2e_swap.rs              # US2 scenarios
├── e2e_presentation.rs      # US3 scenarios
├── e2e_new_workspace.rs     # US4 scenarios
├── e2e_config.rs            # US5, process interface, diagnostics, robustness scenarios
└── e2e_budgets.rs           # SC-001/SC-002 latency and the idle-cost claim

docs/
└── binds.md                 # the documented bind lines (FR-022b), generated from contracts
```

**Structure Decision**: A single Rust binary crate. The spec describes one process with one job;
splitting it into a library crate plus a binary, or into a core/UI workspace, would add a package
boundary that nothing yet needs (Principle II). The module split above is along the seam that
matters for testing: `config`, `model`, `state`, `ordering`, `actions`, `session` and `ui::layout`
are I/O-free and unit-tested directly. `hypr/ipc` and `hypr/events` do I/O but keep their encoding
and parsing separable, so they are unit-tested too. Only `main.rs` and `ui/{mod,shortcuts,render}`
are the thin shell covered by E2E alone — the exemption recorded in Complexity Tracking below.
Integration tests live in `tests/` per Cargo convention, with the nested-compositor harness shared
as a `tests/e2e/` module.

## Complexity Tracking

> Two documented deviations from the constitution. Both are testing-strategy deviations, not
> design complexity; neither introduces an abstraction or a dependency.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **Principle IV** — `main.rs`, `ui/mod.rs`, `ui/shortcuts.rs` and `ui/render.rs` carry no unit tests; they are covered by E2E only | These four files are the Wayland/cairo shell. Unit-testing them means constructing a `wl_display`, a seat and a layer surface in-process — i.e. a mock compositor, which [research.md](./research.md) R14 rejects as testing the application against our own beliefs about Hyprland. Every decision they contain has been pushed into `session.rs`, `ordering.rs`, `actions.rs` and `ui/layout.rs`, which are unit-tested directly | A mock Wayland server would let the shell be unit-tested, but the tests would assert against a fake whose fidelity is unverifiable, and the class of bug they would catch (protocol misuse) is exactly what the nested-Hyprland E2E suite catches for real. Note the exemption is narrower than the shell as a whole: `hypr/ipc.rs` and `hypr/events.rs` *are* unit-tested (T016–T018) |
| **Testing Standards** — `hypr/ipc.rs` carries an environment-gated fault-injection hook used only by the E2E rollback tests (T060) | FR-013a/FR-013b/SC-010 require all-or-nothing swap semantics with rollback. A genuine dispatcher failure cannot be provoked from outside the compositor, so the failure must be injected. This is the third documented substitution alongside headless outputs and `foot` | Corrupting the compositor to force a real dispatch failure is not reproducible and risks the developer's session. Skipping the tests would leave the rollback path — the one path the user can never observe succeeding — unexercised |
