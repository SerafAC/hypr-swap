---
title: Troubleshooting
description: Where the daemon's output goes, how to read it, and the five failures worth recognising on sight.
---

Almost everything that goes wrong says so on standard error before you notice it. So the first step
is always the same one: find the output.

## Where the output goes

hypr-swap writes every diagnostic to **standard error** and nowhere else. It keeps no log file of
its own, so where those records end up is decided entirely by whatever started the daemon.

Started the usual way, from `exec-once` in `hyprland.conf`, standard error goes to **Hyprland's own
log**:

```bash
# The current session's log
cat "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/hyprland.log"

# Just this program's records
grep hypr-swap "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/hyprland.log"

# Follow them as they happen
tail -f "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/hyprland.log" | grep hypr-swap
```

Started from a systemd user unit instead, they are in the journal:

```bash
journalctl --user -u hypr-swap -f
```

And if you are diagnosing something specific, the most direct thing you can do is stop the daemon
and run it in a terminal, where the records arrive in front of you:

```bash
pkill hypr-swap
hypr-swap
```

## What a record looks like

```text
WARN  config.presentation: unknown value "tiles", using "list"
ERROR hypr-swap: another instance already holds the named shortcuts
INFO  compositor: connection lost, retrying in 1s
```

One line each: a level, the subject the record is about, then what happened. There is no timestamp
— whatever supervises the process adds one, and the journal and Hyprland's log both do.

`WARN` means something was wrong and was recovered from. `ERROR` means something you have to act
on. `INFO` means the daemon is telling you what it is doing while it handles it itself.

### The two records that bracket a run

Every run begins and ends with a record under the subject `daemon`, so you can always tell a daemon
that never started from one that started and stopped:

```text
INFO  daemon: hypr-swap 1.0.0 started
INFO  daemon: stopping: SIGTERM
```

The start record appears once the daemon can actually serve a shortcut, and carries the exact
version — which is the first thing a bug report needs. The stopping record is the **last** line the
process writes, and it names the cause: `SIGTERM`, `SIGINT`, `cannot reach the compositor at
start-up`, `another hypr-swap is already running`, or `usage error`. A run that died before it was
ready has no start record and a stopping record all the same.

Reconnection is not a restart: the daemon says `INFO  compositor: reconnected, state rebuilt,
shortcuts re-registered` and there is no second start record, because it started once.

## Before anything else

```bash
hypr-swap --environment
```

This reports what the daemon can see — the compositor it found and its version, the configuration
file it read, the icon set it resolved — and it is the fastest way to find out that the thing you
thought was configured is not.

---

## The shortcuts do nothing

Nothing happens when you press the combination. The overlay never appears and there is no output at
the moment you press it.

Check that the compositor knows about the shortcuts at all:

```bash
hyprctl globalshortcuts
```

If `hypr-swap:switcher` and `hypr-swap:new-workspace` are not listed, the daemon never registered
them, and the reason is in the log at start-up.

| What you find | What it means |
|---|---|
| `ERROR` naming a shortcut, condition `ShortcutRegistrationFailed` | The compositor refused the registration. Usually a second instance — see below. |
| `ERROR` about another instance, condition `SecondInstance` | Something else already holds the names. |
| No hypr-swap records at all | The daemon is not running. Check `exec-once` and try running it in a terminal. |
| The shortcuts *are* listed | The daemon is fine and your `bind` lines are the problem. |

That last case is the common one. Re-read [binding the shortcuts](./binds.md), and check the two
mistakes that account for most of it: the directive is **`bind`**, not `binde` — a repeating bind
re-fires while held and the overlay never settles — and the shortcut name after `global,` must
match exactly, including the `hypr-swap:` prefix.

## The overlay does not appear

The shortcut fires — you can see it in the log, or the new-workspace shortcut works — but nothing is
drawn.

| Condition in the log | What happened |
|---|---|
| `OverlayFocusRefused` | The overlay could not take exclusive keyboard focus, so the session was abandoned rather than left unusable. Another layer-shell client holding an exclusive grab is the usual cause. |
| `CompositorConnection` | The connection dropped. The daemon reconnects on its own with backoff; the overlay works again once it does. |

If there is no record at all and the highlight *does* move when you tap, the overlay is being drawn
somewhere you are not looking — check `placement`, which decides whether it appears on the active
monitor only or on all of them.

## The daemon exits at start-up

It runs and stops immediately. The exit code says which of the three it was:

| Code | Meaning | Condition |
|---|---|---|
| `2` | The command line could not be parsed | `UsageError` |
| `3` | Hyprland could not be reached | `CompositorUnreachableAtStartup` |
| `3` | A second instance already holds the shortcuts | `SecondInstance` |

```bash
hypr-swap; echo "exit: $?"
```

**Exit 3, cannot reach Hyprland.** Either you are not in a Hyprland session, or
`HYPRLAND_INSTANCE_SIGNATURE` is not set in the environment the daemon inherited — which is what
happens when it is started from outside the session, from a login shell or a system unit rather
than `exec-once`.

**Exit 3, second instance.** One is already running:

```bash
pgrep -a hypr-swap
```

Two `exec-once` lines, or one left over from a previous session. Only one daemon can hold the named
shortcuts.

**Exit 2.** The message names the argument. `hypr-swap --help` lists the accepted ones.

An unsupported compositor version is also reported here, naming the version found and the range
required, rather than the daemon guessing at an interface it does not know:

```text
WARN  compositor: Hyprland 0.52.1 is below the supported range (>= 0.55); continuing anyway
```

Note the level and the last three words: this is a warning, not an exit. The daemon carries on,
because it may well work — and losing your switcher over a version comparison would be a worse
failure than the obscure one this record exists to replace. If it then misbehaves, that line is the
first thing to put in the report.

## The icons are wrong or missing

| Condition | What happened |
|---|---|
| `IconUnreadable` | An icon file exists but is malformed or unreadable. The placeholder is drawn in its slot; reported once per program and then cached, so it cannot repeat on every opening. |

Everything else about icons — every window showing the placeholder, one program showing it, the
wrong style entirely — is [program icons](./icons.md), which covers the matching ladder and how to
fix each case. Note that no icon problem is ever fatal: the overlay is fully usable with
placeholders in every slot.

## A workspace swap did not happen

You released the modifier on a workspace on another monitor and the two did not trade places.

| Condition | What happened |
|---|---|
| `SwapRolledBack` | The swap failed part-way and was undone. You are back where you started — that is the intended outcome, not a second failure. |
| `RollbackFailed` | The undo itself failed. This is the one case where the workspaces may be somewhere you did not ask for; the record names what was left where. |
| `SelectionTargetVanished` | The workspace you picked was gone by the time you released. Nothing was changed. |

`SwapRolledBack` and `RollbackFailed` both raise a desktop notification, because they are the two
you need to know about without reading a log.

## A setting is being ignored

```text
WARN  config.presentation: unknown value "tiles", using "list"
WARN  config.style.grid_cell_width: 0 is below the minimum 40, using 40
WARN  config.animations: unknown key, ignored
```

| Condition | What happened |
|---|---|
| `InvalidConfigValue` | The value could not be used. That setting alone fell back to its default; everything else in the file still applies. A dimension outside its range is clamped to the nearer bound rather than rejected. |
| `UnknownConfigKey` | The key is not one hypr-swap accepts — usually a typo, occasionally a setting that does not exist. It is ignored. |

Both are `WARN`, both name the exact key, and both are recovered from. See
[configuration](./configuration.md) for every key that is accepted and
[appearance and themes](./styling.md) for the ranges.

Remember that the file is read **once, at start-up**. If a change appears to have had no effect,
the most likely reason is that the daemon has not been restarted.

## No desktop notifications arrive

| Condition | What happened |
|---|---|
| `NotifyDeliveryFailed` | Delivering a notification failed. It is reported on standard error only — notifying you that notifications are broken would not work. |

`notify-send` is missing, or no notification service is running. Diagnostics continue on standard
error exactly as before; this is the optional dependency doing without.

## None of the above

Collect these three things before [opening an issue](https://github.com/SerafAC/hypr-swap/issues) —
the bug form asks for them, and they are what makes a report actionable:

```bash
hypr-swap --version
hypr-swap --environment
grep hypr-swap "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/hyprland.log" | tail -50
```

If what you found looks like a security problem rather than a bug, use the private channel in
`SECURITY.md` instead of the issue tracker.
