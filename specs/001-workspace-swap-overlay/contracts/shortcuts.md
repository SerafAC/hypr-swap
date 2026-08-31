# Contract: Shortcuts

Covers FR-001–FR-006, FR-020–FR-022b. This is the application's primary interface: two names the
user binds in their own compositor configuration, and a fixed set of keys the overlay handles while
it is open.

## Named global shortcuts

Registered over `hyprland_global_shortcuts_manager_v1.register_shortcut` at start-up and again
after every reconnect (FR-026b).

| `app_id` | `id` | `description` | `trigger_description` |
|---|---|---|---|
| `hypr-swap` | `switcher` | `Open the workspace switcher` | `Hold to browse, release to switch` |
| `hypr-swap` | `new-workspace` | `Switch to a new empty workspace` | `Press` |

The compositor addresses these as `hypr-swap:switcher` and `hypr-swap:new-workspace`. They are
visible to the user via `hyprctl globalshortcuts` once the application is running.

## Bind lines (FR-022b)

These exact lines go in the user's `hyprland.conf`. The application ships them in
`docs/user/binds.md` (moved there by feature 003) and they are reproduced in
[quickstart.md](../quickstart.md).

```ini
# Hold ALT, tap TAB to browse, release ALT to switch
bind = ALT, TAB, global, hypr-swap:switcher

# Jump to a new empty workspace on the current monitor
bind = SUPER, N, global, hypr-swap:new-workspace
```

Rules:

- The key combinations above are **suggestions**. Any combination works; the application is never
  told which keys were used (the protocol is anonymous) and never grabs keys itself (FR-022).
- Use `bind`, not `binde`. A repeating bind fires the shortcut continuously while held, which the
  application would read as continuous navigation.
- Either or both lines may be absent. The application starts and runs normally with no binds at
  all; an unbound shortcut is silently inert and does not affect the other (FR-022b).
- **A modifier in the switcher bind is what makes hold-and-release work.** Bound to a bare key, the
  overlay falls back to sticky mode: it stays open, and Enter commits ([research.md](../research.md)
  R15).

## Switcher behaviour

| Event | Behaviour | Requirement |
|---|---|---|
| `switcher` pressed, no session open | Open the overlay, take exclusive keyboard focus, highlight per the configured order | FR-001, FR-002a, FR-008b |
| `switcher` pressed, session already open | Advance the highlight by one. **No second overlay.** | FR-003, FR-028 |
| Any modifier held at overlay focus is released | Commit the highlighted entry, close | FR-002, FR-005 |
| `switcher` released before the overlay ever gained keyboard focus | Commit the initial highlight immediately; the overlay never appears | FR-005 (fast tap) |
| Compositor connection lost while open | Close without committing | FR-026a |

Commit outcomes are in [research.md](../research.md) R8: same-monitor selections activate,
cross-monitor selections swap, and a selection that is already active does nothing (FR-011).

## In-overlay keys (FR-004a)

Handled by the application while it holds keyboard focus. **Fixed, not configurable**, and they
require no compositor binding.

| Key | Action |
|---|---|
| `Tab`, `Right`, `Down` | Next entry (wraps to the first) |
| `Shift+Tab`, `Left`, `Up` | Previous entry (wraps to the last) |
| `Escape` | Cancel — close with no workspace change and no history change |
| `Enter` | Commit. Only reachable in sticky mode; harmless otherwise |

Any other key is ignored while the overlay is open. Note that a key the compositor has bound is
consumed by the compositor and never reaches the overlay — which is exactly why a repeat `switcher`
press advances the highlight.

## New-workspace behaviour

| Event | Behaviour | Requirement |
|---|---|---|
| `new-workspace` pressed, active workspace has windows | Switch to the lowest unused workspace number, bound to the focused monitor, focused | FR-020 |
| `new-workspace` pressed, active workspace is empty | No-op — nothing created, nothing switched, focus unchanged, no diagnostic | FR-021 |

The overlay never appears for this shortcut. The `released` event is ignored.
