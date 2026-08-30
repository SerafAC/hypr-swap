# Contract: Diagnostics (delta to feature 001)

Feature 001's [`contracts/diagnostics.md`](../../001-workspace-swap-overlay/contracts/diagnostics.md)
remains the authority on the record format, the levels and the notification policy. **None of them
changes** (FR-114): the format is still `<LEVEL> <subject>: <message>`, there are still three
levels, notifications are still reserved for the conditions 001 named, and there is still no
verbosity setting. This page adds three conditions to the policy table.

Diagnostic subjects are part of the 1.0.0 stable surface ([versioning.md](./versioning.md)).

## Added conditions

| Condition | Level | Subject | Notifies | Requirement |
|---|---|---|---|---|
| `Started` | `Info` | `daemon` | no | FR-112 |
| `Stopping` | `Info` | `daemon` | no | FR-113 |
| `CompositorVersionUnsupported` | `Warn` | `compositor` | no | FR-118 |

### `Started`

Reported exactly once per connected lifetime, after the world, the Wayland client and the event
stream are all up — that is, at the point the daemon can actually serve a shortcut.

```text
INFO  daemon: hypr-swap 1.0.0 started
```

A reconnection reports its own existing `CompositorConnection` record ("reconnected, state
rebuilt, shortcuts re-registered"), not a second start record: the daemon started once.

### `Stopping`

Reported on **every** exit path, including those that never reach `Started` — a daemon that dies
at start-up is exactly the case a user cannot otherwise diagnose (spec edge case).

```text
INFO  daemon: stopping: SIGTERM
INFO  daemon: stopping: SIGINT
INFO  daemon: stopping: cannot reach the compositor at start-up
INFO  daemon: stopping: another hypr-swap is already running
INFO  daemon: stopping: usage error
```

The cause names the signal, or the fatal condition already reported at `Error` level just above
it. The record is the last line the process writes, and it is written before the exit code is
returned, so `stopping:` is always the final line of a session's output (SC-042).

### `CompositorVersionUnsupported`

Reported at most once, at start-up, when the compositor's reported version is below
`SUPPORTED_HYPRLAND`'s minimum or cannot be parsed. **Not fatal**: the daemon continues, because it
may well work and taking the user's switcher away over a version comparison is a worse failure
than the obscure one this replaces.

```text
WARN  compositor: Hyprland 0.52.1 is below the supported range (>= 0.55); continuing anyway
WARN  compositor: Hyprland version "next" could not be read; supported range is >= 0.55
```

## The full policy table after this feature

Unchanged conditions are listed by name only; see 001 for their messages.

| Condition | Level | Notifies |
|---|---|---|
| `InvalidConfigValue` | Warn | yes |
| `UnknownConfigKey` | Warn | no |
| `ShortcutRegistrationFailed` | Error | yes |
| `SecondInstance` | Error | yes |
| `CompositorUnreachableAtStartup` | Error | yes |
| `UsageError` | Error | no |
| `CompositorConnection` | Info | no |
| `SwapRolledBack` | Error | yes |
| `RollbackFailed` | Error | yes |
| `SelectionTargetVanished` | Info | no |
| `OverlayFocusRefused` | Error | no |
| `NotifyDeliveryFailed` | Warn | no |
| `IconUnreadable` | Warn | no |
| **`Started`** | **Info** | **no** |
| **`Stopping`** | **Info** | **no** |
| **`CompositorVersionUnsupported`** | **Warn** | **no** |

## Where the record goes

Standard error, as always. Started from `exec-once` in `hyprland.conf`, the daemon's stderr is
collected wherever the compositor's own output goes — the site's troubleshooting page states the
exact location and how to retrieve it (FR-115). No file is written by the daemon itself.
