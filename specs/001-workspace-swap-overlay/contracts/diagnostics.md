# Contract: Diagnostics

Covers FR-013b, FR-013c, FR-024, FR-029–FR-032. The user has no terminal attached, so stderr is the
complete record and notifications are reserved for what the user must act on.

## stderr record format

One line per record, always in this shape:

```text
<LEVEL> <subject>: <message>
```

- `LEVEL` is `ERROR`, `WARN`, or `INFO`.
- `subject` names the specific setting, shortcut, or condition concerned (FR-029) — e.g.
  `config.presentation`, `shortcut.switcher`, `compositor`, `swap`, `notify`.
- No timestamps: whatever supervises the process adds them (spec Assumptions).

Examples:

```text
WARN  config.presentation: unknown value "tiles", using default "list"
ERROR compositor: cannot connect at start-up: no HYPRLAND_INSTANCE_SIGNATURE in environment
INFO  compositor: connection lost, reconnecting (attempt 1, next in 100ms)
INFO  compositor: reconnected, state rebuilt, shortcuts re-registered
ERROR swap: moving workspace 4 to HEADLESS-2 failed, rolled back to the previous layout
ERROR swap: rollback failed; workspace 4 is on HEADLESS-2 and workspace 2 is on eDP-1
WARN  notify: notify-send unavailable, diagnostics continue on stderr only
```

## Notification policy

A desktop notification accompanies the stderr record **only** for conditions the user has to act on
(FR-030), and never for conditions the application recovers from on its own (FR-031).

| Condition | stderr | Notification |
|---|---|---|
| Invalid configuration value (FR-024) | `WARN` | ✅ |
| Failure to register a named shortcut | `ERROR` | ✅ |
| Cannot reach the compositor at start-up (FR-025) | `ERROR` | ✅ |
| Swap failed and was rolled back (FR-013b) | `ERROR` | ✅ |
| Rollback itself failed (FR-013c) | `ERROR` | ✅ |
| Connection lost / retrying / reconnected (FR-026a) | `INFO` | ❌ — self-recovering (FR-031) |
| Selection cancelled because its target vanished (FR-027) | `INFO` | ❌ |
| Unknown key in the configuration file | `WARN` | ❌ |
| Notification delivery itself failed | `WARN` | ❌ — never recurse |

Notification bodies restate the stderr message, with summaries `hypr-swap: configuration problem`,
`hypr-swap: shortcut not registered`, `hypr-swap: cannot reach Hyprland`, and
`hypr-swap: swap failed`.

## Delivery

Notifications are raised by spawning `notify-send` **detached**; the child is never waited on, so a
wedged notification daemon cannot stall the event loop. If the spawn fails — no binary, no service
— one `WARN notify:` line is written and the application continues normally (FR-032). The failure
is reported at most once per process; the underlying diagnostic still reaches stderr every time.
