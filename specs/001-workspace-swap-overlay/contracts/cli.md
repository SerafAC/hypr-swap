# Contract: Command line and lifecycle

Covers FR-025, FR-026a, and the process contract the user's session manager relies on.

## Invocation

```text
hypr-swap [--config <path>] [--version] [--help]
```

| Flag | Effect |
|---|---|
| `--config <path>` | Use this configuration file instead of the default location. Missing file at an explicit path is an error, unlike the default location |
| `--version` | Print version, exit 0 |
| `--help` | Print usage including the bind lines from [shortcuts.md](./shortcuts.md), exit 0 |

No other flags. There are no subcommands: the binary is the daemon, and every user-facing action
arrives through a shortcut (FR-022). It is started once per session, typically:

```ini
exec-once = hypr-swap
```

## Environment

| Variable | Use | Missing |
|---|---|---|
| `HYPRLAND_INSTANCE_SIGNATURE` | Locates the IPC sockets | Fatal — exit 3 |
| `XDG_RUNTIME_DIR` | Locates the IPC sockets | Fatal — exit 3 |
| `WAYLAND_DISPLAY` | Wayland connection for layer-shell and global shortcuts | Fatal — exit 3 |
| `XDG_CONFIG_HOME` | Configuration location | Falls back to `~/.config` |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Clean shutdown on `SIGTERM`/`SIGINT`, or `--version`/`--help` |
| 2 | Invalid command line, or `--config` naming a file that does not exist |
| 3 | Cannot reach the compositor **at start-up** — Wayland connection refused, missing environment, or the IPC socket absent (FR-025). Reported on stderr and as a notification (FR-030) |

**Losing a connection while running is never fatal** (FR-025, FR-026a). The application closes any
open overlay without committing, retries with backoff (100 ms doubling to a 5 s cap, indefinitely),
and on success rebuilds state and re-registers its shortcuts. It does not exit and does not require
a restart.

## Runtime characteristics

- Single process, single thread, one event loop over the Wayland fd and the Hyprland event socket.
  No polling — idle CPU is 0 %.
- Exactly one instance per compositor should run. A second instance registering the same shortcut
  ids is not an error at the protocol level, so the second instance detects the collision via
  `hyprctl globalshortcuts` at start-up, reports it, and exits 3.
- `SIGTERM`/`SIGINT` close any open overlay without committing, destroy the surfaces, and exit 0.
- Standard output is unused. Every diagnostic goes to stderr
  ([diagnostics.md](./diagnostics.md)), which the session manager is expected to capture (spec
  Assumptions).
