# Contracts: Workspace Swap Overlay

The interfaces this application exposes to its users and to other systems, plus the one interface
it consumes. These are the surfaces the E2E suite drives; anything not listed here is internal and
may change freely.

| Contract | Direction | What it covers |
|---|---|---|
| [shortcuts.md](./shortcuts.md) | exposed | The two named global shortcuts, the bind lines users write, and the fixed in-overlay keys |
| [config.md](./config.md) | exposed | Configuration file location, schema, defaults, and invalid-value behaviour |
| [cli.md](./cli.md) | exposed | Binary invocation, environment, exit codes, lifecycle |
| [diagnostics.md](./diagnostics.md) | exposed | stderr record format and which conditions notify |
| [compositor-ipc.md](./compositor-ipc.md) | consumed | The Hyprland IPC and Wayland protocol surface depended on, and the version floor |

## Requirement trace

Every functional requirement is either realised by a contract above or is internal behaviour
implemented in the module named.

| Requirement | Where |
|---|---|
| FR-001, FR-002, FR-005 | [shortcuts.md](./shortcuts.md) — switcher shortcut and commit-on-release |
| FR-002a | [shortcuts.md](./shortcuts.md); `ui/mod.rs` (exclusive keyboard interactivity) |
| FR-003, FR-004, FR-004a, FR-006 | [shortcuts.md](./shortcuts.md) — in-overlay key map; `session.rs` |
| FR-007, FR-008, FR-008a–d | `ordering.rs`, `state.rs` ([data-model.md](../data-model.md)) |
| FR-009–FR-013c | `actions.rs`, `hypr/ipc.rs` ([research.md](../research.md) R8) |
| FR-014–FR-016, FR-018, FR-019 | `ui/render.rs`, `ui/layout.rs`; presentation selected via [config.md](./config.md) |
| FR-015a, FR-015b | `ui/render.rs` from `j/clients` geometry ([compositor-ipc.md](./compositor-ipc.md)) |
| FR-017 | [config.md](./config.md) — `placement`; `ui/mod.rs` (one surface per monitor) |
| FR-020, FR-021 | [shortcuts.md](./shortcuts.md) — new-workspace shortcut; `actions.rs` |
| FR-022, FR-022a, FR-022b | [shortcuts.md](./shortcuts.md) |
| FR-023, FR-024 | [config.md](./config.md) |
| FR-025, FR-026a–d | [cli.md](./cli.md) (exit codes), [compositor-ipc.md](./compositor-ipc.md) (reconnect) |
| FR-026, FR-027, FR-028 | `state.rs`, `session.rs` |
| FR-029–FR-032 | [diagnostics.md](./diagnostics.md) |
