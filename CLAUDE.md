# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`hypr-swap` is an Alt-Tab-style workspace switcher with cross-monitor swapping for Hyprland — a
single Rust binary (edition 2024, Rust 1.96) that runs as a user-session daemon. The user binds two
named shortcuts in `hyprland.conf`; the daemon registers them via `hyprland-global-shortcuts-v1`,
shows a `wlr-layer-shell` overlay with exclusive keyboard focus (which is what lets it commit on
modifier release), and drives all workspace changes through the Hyprland IPC sockets. No screen
capture, no key grabbing, no GUI toolkit — rendering is cairo/pango into an shm buffer.

## Commands

```bash
cargo build                          # needs system cairo/pango dev libraries
cargo test --lib                     # unit tests — no compositor or display needed
cargo test --test 'e2e_*'            # E2E — launches a nested Hyprland; see below
cargo test --lib ordering            # single module's unit tests
cargo test --test e2e_harness -- nested_instance_starts   # single E2E test
cargo clippy --all-targets -- -D warnings   # clippy pedantic is enabled at warn level
cargo fmt --check
```

E2E tests require a **live Wayland session with Hyprland ≥ 0.55 and `foot` installed** — the
harness (`tests/e2e/harness.rs`) starts a nested Hyprland as a client of the developer's session,
adds headless outputs, spawns `foot` windows, and injects real key events via
`virtual-keyboard-unstable-v1`. All assertions go through the nested instance's IPC socket. Tests
serialize on an internal lock (only one nested compositor at a time), so don't fight the harness
with `--test-threads`. In a headless environment (CI, container), only `cargo test --lib` can run.

## Spec-driven workflow

This project is developed with spec-kit. The active feature lives in
`specs/001-workspace-swap-overlay/`:

- `spec.md` — requirements (FR-xxx) and acceptance scenarios; the authority on behaviour
- `plan.md` — architecture decisions, module map, E2E coverage table mapping tests to FRs
- `tasks.md` — the task list with `[X]` completion markers; **update it as tasks are completed**
- `contracts/` — the external surface: shortcut names, config schema, CLI, diagnostics, the exact
  Hyprland IPC commands used
- `research.md` — numbered decisions (R1–R15) with rejected alternatives; cite these rather than
  re-litigating

The constitution (`.specify/memory/constitution.md`) is binding: KISS/YAGNI/DRY, unit tests for
all code, and E2E coverage of major requirements are non-negotiable. New abstractions or
dependencies must be justified in plan.md's Complexity Tracking table. Code comments reference FR
numbers from the spec — keep that convention.

## Architecture

The module split follows one seam: **pure decision logic vs thin I/O shell**.

- **Pure, I/O-free, unit-tested in-module**: `config.rs` (TOML load, per-setting fallback),
  `model.rs` (Workspace/Monitor/Window + IPC deserialisation), `state.rs` (`World` — cached
  compositor view + MRU activation history, updated by applying events), `ordering.rs` (entry
  order and initial highlight), `actions.rs` (selection → command plan + rollback plan;
  new-workspace plan), `session.rs` (switcher state machine), `ui/layout.rs` (entry metrics,
  scroll viewport, miniature rect mapping).
- **I/O but still unit-tested**: `hypr/ipc.rs` (request/response + batch dispatch on socket1;
  carries an env-gated fault-injection hook used only by the E2E rollback tests) and
  `hypr/events.rs` (socket2 event stream, line parsing, backoff).
- **Shell, E2E-covered only, deliberately logic-free**: `main.rs` (start-up, calloop event loop,
  reconnection), `ui/mod.rs` (Wayland registry/seat/layer surfaces/shm), `ui/shortcuts.rs`
  (global-shortcut registration), `ui/render.rs` (cairo painting). Any new decision rule belongs
  in the pure modules, not here.

Cross-cutting facts worth knowing before editing:

- **One event loop** (calloop in `main.rs`) over three sources: the Wayland fd, the Hyprland event
  socket, and signals. The Wayland shell never does IPC directly — it records `Request`s in
  `app.outbox`, and `main.rs` acts on them after dispatch (`handle_request`).
- **Reconnection is teardown**: losing the compositor drops the whole client (surfaces, world,
  history) and `run()` rebuilds everything after backoff. Don't add partial-reconnect state.
- **All diagnostics go through `diag.rs`** (`diag::report(Condition, subject, message)`), which
  owns the stderr format and the notification policy. Never `eprintln!` directly outside it.
- `build.rs` + the vendored `protocols/hyprland-global-shortcuts-v1.xml` generate the protocol
  bindings via `wayland-scanner` (a proc-macro expansion, not a generated file).
- Bind lines, shortcut names, and the usage text all derive from `ui/shortcuts.rs::Shortcut` —
  change shortcuts there, nowhere else.
