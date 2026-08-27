# Quickstart: Workspace Swap Overlay

How to build, run and validate this feature. The scenarios below are the runnable proof that it
works end to end; they map to the acceptance scenarios in [spec.md](./spec.md) and to the E2E tests
listed in [plan.md](./plan.md).

## Prerequisites

| Requirement | Check | Notes |
|---|---|---|
| Hyprland ≥ 0.55 | `hyprctl version` | Validated on 0.55.4 and 0.56.2 |
| Rust ≥ 1.96 | `cargo --version` | Edition 2024; `Cargo.toml` sets `rust-version` |
| cairo + pango development files | `pkg-config --modversion cairo pango pangocairo` | System libraries |
| `foot` | `command -v foot` | E2E test client only |
| `notify-send` | `command -v notify-send` | Optional — absence is a supported configuration (FR-032) |

## Build and run

```bash
cargo build --release
./target/release/hypr-swap        # runs in the foreground, diagnostics on stderr
```

For everyday use, start it with the session:

```ini
# hyprland.conf
exec-once = hypr-swap
```

## Bind the shortcuts

Add to `hyprland.conf`, then `hyprctl reload`:

```ini
bind = ALT, TAB, global, hypr-swap:switcher
bind = SUPER, N, global, hypr-swap:new-workspace
```

Confirm the application registered them:

```bash
hyprctl globalshortcuts     # expect hypr-swap:switcher and hypr-swap:new-workspace
```

Use `bind`, not `binde`. Full contract in [contracts/shortcuts.md](./contracts/shortcuts.md).

## Configure (optional)

Everything below is optional — with no file at all the application runs on documented defaults
(flat list, active monitor, MRU order).

```bash
mkdir -p ~/.config/hypr-swap
cat > ~/.config/hypr-swap/config.toml <<'EOF'
presentation = "grid"     # "list" (default) | "grid"
placement    = "active"   # "active" (default) | "all"
order        = "mru"      # "mru" (default) | "compositor" | "monitor"
EOF
```

Restart the application to pick it up. Schema in [contracts/config.md](./contracts/config.md).

## Manual validation scenarios

Each names the requirement it proves. Run them against a live session after building.

### 1. Switch to any workspace (US1 — FR-001, FR-002, FR-005)

Open windows on three workspaces. Hold `ALT`, tap `TAB` twice, release `ALT`.
**Expect**: the overlay appears while held, the highlight moves with each tap, and on release the
third entry's workspace is active and focused.

### 2. MRU default (US1-AS1/AS2 — FR-008a, FR-008b)

Visit workspaces 3, then 7, then 1. Hold `ALT`, tap `TAB` once, release.
**Expect**: the overlay opened with workspace 1 first and the highlight already on 7; you land on
7. Repeat the gesture — you bounce back to 1.

### 3. Fast tap (SC-001)

From workspace 1, press and release `ALT+TAB` as fast as you can.
**Expect**: you land on the previous workspace. No overlay lingers on screen.

### 4. Cancel (US1-AS5/AS6 — FR-006)

Hold `ALT`, tap `TAB` a few times, press `Escape`, then release `ALT`.
**Expect**: the overlay closes at once, the workspace is unchanged, and the next overlay opens in
the same order as before — the cancelled session left no trace in the MRU history.

### 5. Cross-monitor swap (US2-AS1 — FR-010, FR-012, FR-013)

With two monitors, workspace A focused on monitor 1 and workspace B active on monitor 2: select B
from monitor 1 and release.
**Expect**: B is on monitor 1, active and focused; A is on monitor 2 and active there; every window
is still open on the workspace it started on.

No second monitor to hand? Create one:

```bash
hyprctl output create headless      # adds HEADLESS-2, 1920x1080
hyprctl monitors -j | jq -c '.[] | {name, activeWorkspace: .activeWorkspace.name}'
```

### 6. Swap a workspace that is not currently shown (US2-AS2 — FR-010)

Bind workspace C to monitor 2 but leave monitor 2 showing D. Select C from monitor 1.
**Expect**: C moves to monitor 1 and becomes active; the workspace you left moves to monitor 2.

### 7. Grid miniatures (US3 — FR-015, FR-015a, SC-008)

Set `presentation = "grid"`, restart, and arrange one workspace with two windows side by side and a
third below the second. Switch away from it, then open the overlay.
**Expect**: that workspace's miniature shows three labelled rectangles in the same relative
arrangement — the same accuracy as a workspace currently on screen, with no screen capture
involved. Long titles are truncated with a visible ellipsis (FR-015b).

### 8. New workspace (US4 — FR-020, FR-021)

With workspaces 1, 2 and 4 in use, press `SUPER+N` from monitor 2.
**Expect**: workspace 3 is active and focused on monitor 2. Press `SUPER+N` again on the now-empty
workspace.
**Expect**: nothing happens — no new workspace, no focus change (FR-021).

### 9. Many workspaces scroll (SC-005 — FR-019)

Create 20 workspaces, then open the overlay.
**Expect**: entries are the same readable size as with three workspaces, the overlay is capped at
80 % of the monitor, and navigating past the visible edge scrolls the highlight into view.

### 10. Invalid configuration (US5-AS5 — FR-024, FR-030)

```bash
echo 'presentation = "tiles"' >> ~/.config/hypr-swap/config.toml
./target/release/hypr-swap
```

**Expect**: a `WARN config.presentation:` line on stderr naming the setting, a desktop
notification, the list presentation in use, and the application running normally.

### 11. Compositor restart (SC-009 — FR-026a, FR-026b)

With the application running, restart Hyprland.
**Expect**: no crash and no manual restart. `INFO compositor:` lines on stderr show the reconnect,
no notification is raised, and the shortcuts work again within 10 seconds.

### 12. No notification daemon (FR-032)

Stop the notification service, then trigger scenario 10.
**Expect**: one `WARN notify:` line, the configuration diagnostic still on stderr, and the
application running normally.

## Automated tests

```bash
cargo test --lib                    # unit tests — no compositor, no display needed
cargo test --test 'e2e_*'           # E2E — launches a nested Hyprland instance
cargo test                          # everything
```

The E2E suite launches its **own nested Hyprland** with its own instance signature and config, adds
headless outputs for multi-monitor scenarios, spawns `foot` windows with known titles, and injects
real key events through `virtual-keyboard-unstable-v1`. It never touches the developer's session
and never asserts through an internal API — every assertion is a question put to the nested
compositor over its own IPC socket. Details and documented substitutions:
[research.md](./research.md) R14.

## Criteria not covered by automated tests

- **SC-003** (100 consecutive swaps lose no windows): run
  `cargo test --test e2e_swap -- --ignored soak`, which repeats the swap scenario 100 times against
  the nested instance and compares the window inventory before and after.
- **SC-007** (8-hour session leaves no orphaned overlays or extra workspaces): leave the
  application running for a working day, then check `hyprctl layers` for stray `hypr-swap`
  namespaces and `hyprctl workspaces` for workspaces you did not create.
- **SC-004** (a new user reaches a named workspace out of ten on the first attempt without
  documentation): a usability check with someone who has not seen the tool, not an assertion.
