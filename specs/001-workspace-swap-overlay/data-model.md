# Phase 1 Data Model: Workspace Swap Overlay

**Feature**: `001-workspace-swap-overlay` | **Date**: 2026-07-28

Entities from [spec.md](./spec.md) "Key Entities", made concrete. Field types are Rust types; the
"source" column says where a value comes from, since almost everything here is a projection of
compositor state rather than data this application owns.

The application owns exactly two pieces of state: the **activation history** and the **switcher
session**. Everything else is a cache of what the compositor reports, rebuilt wholesale on
reconnect (FR-026b).

## Workspace

`model::Workspace` — an ordinary compositor workspace.

| Field | Type | Source | Notes |
|---|---|---|---|
| `id` | `i32` | `j/workspaces[].id` | Identity. Negative ids are special/scratchpad workspaces |
| `name` | `String` | `j/workspaces[].name` | Displayed as-is (spec Assumptions); numeric workspaces report their number |
| `monitor` | `MonitorName` | `j/workspaces[].monitor` | The monitor this workspace is bound to |
| `window_count` | `u32` | `j/workspaces[].windows` | Used by the FR-021 emptiness check |

**Validation rules**

- A workspace with `id < 0` is a special/scratchpad workspace: excluded from entries and never
  moved between monitors (FR-007, spec Edge Cases). This is the single place that rule is
  expressed.
- `monitor` must name a known monitor; a workspace naming an unknown monitor is dropped from the
  world on rebuild and reported (this happens transiently when a monitor is unplugged).
- Workspaces with `window_count == 0` are listed like any other (FR-007).

**Relationships**: bound to exactly one `Monitor`; holds zero or more `Window`s.

## Monitor

`model::Monitor` — a connected display.

| Field | Type | Source | Notes |
|---|---|---|---|
| `name` | `MonitorName` | `j/monitors[].name` | Identity, e.g. `eDP-1`, `HEADLESS-2` |
| `id` | `i32` | `j/monitors[].id` | |
| `position` | `(i32, i32)` | `j/monitors[].x/.y` | Layout coordinates |
| `size` | `(u32, u32)` | `j/monitors[].width/.height` | Pixels |
| `scale` | `f32` | `j/monitors[].scale` | Overlay sizing and miniature normalisation |
| `active_workspace` | `i32` | `j/monitors[].activeWorkspace.id` | Exactly one, always |
| `focused` | `bool` | `j/monitors[].focused` | At most one monitor is focused |

**Validation rules**

- Every monitor has exactly one active workspace at all times; after any swap, both affected
  monitors must still satisfy this (FR-013) — it is the post-condition the swap verification
  checks.
- Exactly one monitor is focused whenever a compositor connection is established.

**Relationships**: displays one active `Workspace`; is the binding target of zero or more
`Workspace`s.

## Window

`model::Window` — an application window.

| Field | Type | Source | Notes |
|---|---|---|---|
| `address` | `String` | `j/clients[].address` | Identity |
| `title` | `String` | `j/clients[].title` | Label in both presentations; ellipsised by pango when it does not fit (FR-015b) |
| `class` | `String` | `j/clients[].class` | Fallback label when `title` is empty |
| `workspace` | `i32` | `j/clients[].workspace.id` | Owning workspace |
| `at` | `(i32, i32)` | `j/clients[].at` | Layout coordinates, for miniatures |
| `size` | `(u32, u32)` | `j/clients[].size` | Layout size, for miniatures |
| `floating` | `bool` | `j/clients[].floating` | Painted above tiled windows |
| `mapped` | `bool` | `j/clients[].mapped` | Unmapped windows are excluded from both presentations |

**Validation rules**

- Belongs to exactly one workspace.
- `at` is in global layout coordinates, so miniature geometry is
  `(at - monitor.position) / monitor.size` — normalised to the monitor the *workspace* is bound to,
  not the monitor it is currently shown on. This is what makes an off-screen workspace's miniature
  as accurate as a visible one (FR-015a, SC-008).
- A window with zero width or height is skipped rather than drawn as a degenerate rectangle.

**Relationships**: belongs to one `Workspace`.

## World

`state::World` — the whole cached compositor view. Not a spec entity; it is the container that lets
the pure functions take a single argument.

| Field | Type | Notes |
|---|---|---|
| `monitors` | `Vec<Monitor>` | |
| `workspaces` | `Vec<Workspace>` | In compositor-reported order — this *is* "compositor order" for FR-008a |
| `windows` | `Vec<Window>` | |
| `history` | `ActivationHistory` | See below |
| `connected_at` | `Instant` | Diagnostics only |

**State transitions** — the world is mutated only by compositor events (`hypr::events`) and
wholesale rebuilds:

| Event (socket2) | Effect |
|---|---|
| `workspace`, `workspacev2` | Set focused monitor's active workspace; push to history |
| `focusedmon` | Change focused monitor; push its active workspace to history |
| `createworkspace`, `destroyworkspace` | Add/remove workspace; drop destroyed ids from history |
| `moveworkspace` | Rebind workspace to a monitor |
| `openwindow`, `closewindow`, `movewindow` | Add/remove/rebind window |
| `windowtitle` | Update title |
| `monitoradded`, `monitorremoved` | Full rebuild (cheap, and monitor changes reshuffle bindings) |
| connection lost | Drop the world entirely; no overlay may be shown while absent (FR-026d) |
| connection established | Full rebuild from `j/monitors`, `j/workspaces`, `j/clients`; history empty (FR-026c) |

## Activation History

`state::ActivationHistory` — the session-scoped MRU record (FR-008c).

| Field | Type | Notes |
|---|---|---|
| `order` | `Vec<i32>` | Workspace ids, most recently active first, no duplicates |

**Rules**

- `push(id)` moves `id` to the front, removing any earlier occurrence.
- Fed **only** from observed compositor activations, never from this application's own commands —
  the mechanism that makes external switches count (FR-008c) and cancelled sessions not count.
- Ids of destroyed workspaces are removed.
- Cleared on connection loss and rebuilt from post-reconnect activations (FR-026c).
- Workspaces absent from `order` are "never active this session" and sort after all present ones,
  in compositor order (FR-008d).

## Entry

`ordering::Entry` — one row or cell in the overlay. Derived, never stored across sessions.

| Field | Type | Notes |
|---|---|---|
| `workspace_id` | `i32` | |
| `label` | `String` | Workspace name |
| `windows` | `Vec<WindowRef>` | Titles for the list; titles + normalised rects for the grid |
| `monitor` | `MonitorName` | Binding at the time the session opened |
| `is_active` | `bool` | Active workspace of its monitor — rendered distinctly (FR-008) |

Produced by `ordering::entries(world, order) -> (Vec<Entry>, usize)`, where the `usize` is the
initial highlight index:

| `order` | Sequence | Initial highlight |
|---|---|---|
| `Mru` (default) | History order, then never-active workspaces in compositor order | Index **1** (FR-008b) |
| `Compositor` | Compositor-reported order | Index of the active workspace |
| `Monitor` | Grouped by monitor, compositor order within each group | Index of the active workspace |

Special/scratchpad workspaces are filtered out before ordering (FR-007). With a single workspace,
the MRU highlight clamps to index 0 (spec Edge Cases).

## Switcher Session

`session::Session` — the transient state of one open overlay.

| Field | Type | Notes |
|---|---|---|
| `entries` | `Vec<Entry>` | Snapshot taken when the session opened |
| `highlight` | `usize` | Index into `entries`; wraps in both directions (FR-003, FR-004) |
| `origin_monitor` | `MonitorName` | The focused monitor when the session opened |
| `initial_mods` | `ModMask` | Modifiers depressed at keyboard focus (R4); empty ⇒ sticky mode (R15) |
| `focus_state` | `FocusState` | `AwaitingFocus` \| `Focused` \| `NeverFocused` — drives the fast-tap path |
| `outcome` | `Outcome` | `Open` \| `Committed(i32)` \| `Cancelled` |

**State transitions**

```text
                 shortcut pressed
   (no session) ──────────────────► Open/AwaitingFocus
                                      │
              keyboard enter ─────────┤────────► Open/Focused   (record initial_mods)
              shortcut released ──────┘                          (fast-tap: commit highlight)
                                      │
   Open ── shortcut pressed ──────────┤ highlight += 1 (wrapping)   [FR-005 repeat, FR-028]
   Open ── Tab / Right / Down ────────┤ highlight += 1 (wrapping)
   Open ── Shift+Tab / Left / Up ─────┤ highlight -= 1 (wrapping)
   Open ── Escape ────────────────────┤ Cancelled
   Open ── any initial_mod released ──┤ Committed(entries[highlight].workspace_id)
   Open ── Enter (sticky mode only) ──┤ Committed(…)
   Open ── connection lost ───────────┘ Cancelled                    [FR-026a]
```

**Rules**

- At most one session exists at a time (FR-028).
- `Cancelled` performs no dispatch and leaves the activation history untouched (US1-AS5).
- On commit, if the target workspace no longer exists in the current world, the session is treated
  as cancelled (FR-027) — the entries are a snapshot, so this is a real case. If the workspace
  survives but the monitor recorded in its `Entry` no longer exists, the plan degrades to plain
  activation on the focused monitor rather than cancelling (FR-027, FR-009).
- On close, keyboard focus returns to the previously focused window, which happens implicitly when
  the layer surface is destroyed (FR-002a).

## Command Plan

`actions::CommandPlan` — the outcome of a committed selection, and the unit of atomicity (R8).

| Field | Type | Notes |
|---|---|---|
| `commands` | `Vec<String>` | Dispatchers, sent as one batch |
| `expected` | `ExpectedState` | Workspace→monitor bindings, per-monitor active workspace, focused monitor |
| `rollback` | `Vec<String>` | Inverse batch, computed from the pre-state *before* dispatch |

Produced by `actions::plan(world, origin_monitor, selected_id) -> Option<CommandPlan>`, returning
`None` for a no-op (selection is already active, FR-011). The three shapes are in
[research.md](./research.md) R8. `actions::new_workspace_plan(world) -> Option<CommandPlan>`
follows the same contract and returns `None` when the active workspace is already empty (FR-021).

**Rules**

- `expected` is what the post-dispatch read-back is compared against; a mismatch triggers
  `rollback` (FR-013a).
- `rollback` restores workspace→monitor bindings, per-monitor active workspaces, and focus
  (FR-013a). A same-monitor activation has a trivial rollback and, in practice, never needs it.
- `plan` resolves the selected workspace's monitor from the **current** world, not from the `Entry`
  snapshot. A snapshot monitor that no longer exists therefore falls through to the same-monitor
  activation shape, which is what makes FR-027's degradation fall out of the existing code path
  rather than needing a special case.
- Plans never touch workspaces other than the pair involved (spec Assumptions).

## Configuration

`config::Configuration` — user settings. Schema and defaults in
[contracts/config.md](./contracts/config.md).

| Field | Type | Default | Requirement |
|---|---|---|---|
| `presentation` | `Presentation::{List, Grid}` | `List` | FR-016, FR-023 |
| `placement` | `Placement::{ActiveMonitor, AllMonitors}` | `ActiveMonitor` | FR-017, FR-023 |
| `order` | `Order::{Mru, Compositor, Monitor}` | `Mru` | FR-008a, FR-023 |

**Validation rules**

- Each setting is validated independently; an invalid value falls back to that setting's default
  and is reported by name on stderr and as a notification (FR-024, FR-030), leaving the other
  settings' user-supplied values in force.
- A missing file yields all defaults with no diagnostic (FR-023).
- Key combinations are deliberately **not** part of this entity — they live in the compositor's
  configuration (spec Key Entities, FR-022).
