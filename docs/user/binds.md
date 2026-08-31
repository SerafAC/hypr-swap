---
title: Binding the shortcuts
description: The two bind lines to copy into hyprland.conf, and the keys the overlay handles itself.
---

`hypr-swap` never grabs keys. It registers two **named global shortcuts** over
`hyprland-global-shortcuts-v1`, and your `hyprland.conf` decides which keys trigger them
(FR-022, FR-022b). This file is the exact text to copy; `hypr-swap --help` prints the same
lines, and a unit test asserts the two cannot drift apart.

## The two lines

```ini
# Hold ALT, tap TAB to browse, release ALT to switch
bind = ALT, TAB, global, hypr-swap:switcher

# Jump to a new empty workspace on the current monitor
bind = SUPER, N, global, hypr-swap:new-workspace
```

Start the daemon with your session as well:

```ini
exec-once = hypr-swap
```

Check the compositor has been told about the shortcuts with `hyprctl globalshortcuts`. The two
names it lists are `hypr-swap:switcher` and `hypr-swap:new-workspace`.

## The key combinations are yours

`ALT, TAB` and `SUPER, N` are **suggestions**. Any combination works. The protocol is anonymous:
the application is never told which keys triggered a shortcut, and never reads the keyboard except
while its own overlay holds focus.

## Use `bind`, not `binde`

`binde` is Hyprland's *repeating* bind. Held down, it fires the shortcut continuously, which this
application reads as continuous navigation — the highlight would race through the workspace list
for as long as you held the key. Use plain `bind`.

## A modifier in the switcher bind is what makes hold-and-release work

The gesture is: hold the modifier, tap the key to move the highlight, release the modifier to
commit. The overlay discovers which modifiers to watch from the ones you are holding when it takes
keyboard focus — it cannot ask the compositor which keys you bound (research.md R15).

Bound to a **bare key** with no modifier, there is no release to commit on, so the overlay falls
back to **sticky mode**: it stays open, `Tab` and the arrow keys move the highlight, `Enter`
commits, and `Escape` cancels.

## Either line may be left out

Both binds are optional and independent. With neither present the daemon starts, runs, and does
nothing until you add one — an unbound shortcut is silently inert, produces no diagnostic, and has
no effect on the other one (FR-022b).

## In-overlay keys are fixed

These are handled by the overlay's own keyboard focus and need no binding at all. They are not
configurable (FR-004a).

| Key | Action |
|---|---|
| `Tab`, `Right`, `Down` | Next entry (wraps to the first) |
| `Shift+Tab`, `Left`, `Up` | Previous entry (wraps to the last) |
| `Escape` | Cancel — no workspace change, no history change |
| `Enter` | Commit. Only reachable in sticky mode; harmless otherwise |

A key the compositor has bound is consumed by the compositor and never reaches the overlay. That
is exactly why tapping the switcher bind again advances the highlight instead of opening a second
overlay.
