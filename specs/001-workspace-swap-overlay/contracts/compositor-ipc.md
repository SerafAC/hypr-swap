# Contract: Compositor interface (consumed)

The Hyprland surface this application depends on. Not exposed to users, but it is the external
contract most likely to break under a compositor upgrade, so it is pinned and stated explicitly.

**Version floor**: Hyprland ≥ 0.55. Verified against 0.55.4.

## Wayland protocols required

| Protocol | Use | If absent |
|---|---|---|
| `hyprland_global_shortcuts_manager_v1` | Register `switcher` and `new-workspace` | Fatal at start-up, exit 3 |
| `zwlr_layer_shell_v1` (v4) | Overlay-layer surface, exclusive keyboard interactivity | Fatal at start-up, exit 3 |
| `wp_viewporter` (v1) | Display the device-pixel buffer at the surface's logical size, so the overlay is the same physical size on a scaled monitor (FR-019) | Fatal at start-up, exit 3 |
| `wl_shm`, `wl_compositor`, `wl_seat`, `wl_output` | Buffers, input, per-monitor surfaces | Fatal at start-up, exit 3 |

`virtual_keyboard_manager_v1` is used by the **E2E suite only**, never by the application.

Layer surface parameters: layer `overlay` (above fullscreen windows, FR-018), keyboard
interactivity `exclusive` (FR-002a), anchored centre with no exclusive zone, namespace
`hypr-swap`. One surface per monitor the overlay is shown on (FR-017).

**Units.** `set_size` and `configure`'s reply are in *logical* pixels; the shm buffer is in
*device* pixels, i.e. logical × the monitor's `j/monitors[].scale`. `wp_viewport::set_destination`
is set to the logical size on every frame, which is what declares the ratio between the two.
`wl_surface::set_buffer_scale` is deliberately left at 1: it takes an integer, and Hyprland's
fractional scales (1.25, 1.5, …) could not be expressed with it.

## IPC sockets

Both under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`.

### `.socket.sock` — requests

One connection per request; write the request, read to EOF.

| Request | Returns | Used for |
|---|---|---|
| `j/monitors` | JSON array | Monitor name, position, size, scale, active workspace, focus |
| `j/workspaces` | JSON array | Workspace id, name, monitor binding, window count |
| `j/clients` | JSON array | Window address, title, class, workspace, `at`, `size`, `floating`, `mapped` |
| `/dispatch <dispatcher> <args>` | `ok` or an error string | Every state change |
| `[[BATCH]]<cmd>;<cmd>;…` | Concatenated results | Applying a swap in one pass (FR-013a, SC-010) |

Dispatchers used, all verified present on 0.55.4:

| Dispatcher | Use |
|---|---|
| `workspace <id>` | Same-monitor activation (FR-009) |
| `swapactiveworkspaces <monA> <monB>` | Cross-monitor swap when the target is active on its monitor (FR-010) |
| `moveworkspacetomonitor <ws> <mon>` | Cross-monitor swap when the target is not active on its monitor; also the rollback primitive |
| `focusworkspaceoncurrentmonitor <ws>` | Activating on the focused monitor; new workspace creation (FR-020) |
| `focusmonitor <mon>` | Restoring focus after a swap and during rollback (FR-010, FR-013a) |

### `.socket2.sock` — events

A persistent connection streaming `EVENT>>DATA` lines. Consumed to keep state current (FR-026) and
to maintain MRU (FR-008c): `workspace`, `workspacev2`, `focusedmon`, `createworkspace`,
`destroyworkspace`, `moveworkspace`, `openwindow`, `closewindow`, `movewindow`, `windowtitle`,
`monitoradded`, `monitorremoved`. Unknown event names are ignored, so new Hyprland events are
non-breaking.

## Failure and reconnection

| Situation | Behaviour | Requirement |
|---|---|---|
| Socket absent or connection refused at start-up | Report, exit 3 | FR-025 |
| Either connection drops while running | Close any open overlay uncommitted; retry 100 ms doubling to a 5 s cap, indefinitely | FR-026a, FR-026d |
| Reconnected | Rebuild from `j/monitors` + `j/workspaces` + `j/clients`, re-register both shortcuts, clear activation history | FR-026b, FR-026c |
| A dispatch returns a non-`ok` result | Treat as a failed step; roll back the batch and report | FR-013a, FR-013b |
| A workspace or monitor named in a plan no longer exists | Treat the selection as cancelled | FR-027 |

## Assumptions this contract rests on

Each is validated by an E2E test, so a compositor upgrade that breaks one fails the suite rather
than the user's session:

1. `j/clients` reports layout geometry for windows on workspaces that are **not currently
   displayed** — the basis of FR-015a miniatures.
2. `wl_keyboard.modifiers` is delivered to an exclusive-mode layer surface on `enter` and on every
   change — the basis of commit-on-release (FR-002a, FR-022a).
3. A key consumed by a compositor bind is **not** forwarded to the focused surface — the reason a
   repeat `switcher` press, rather than a `Tab` key event, advances the highlight
   ([research.md](../research.md) R5).
4. `[[BATCH]]` applies its commands without an intermediate presented frame — the basis of
   SC-010's "no half-swapped state is ever observable".

Assumptions 2 and 3 are confirmed by the R4 spike before the switcher is built; assumption 1 is
confirmed by `e2e_grid_offscreen_workspace`; assumption 4 by `e2e_swap_active_workspaces`.
