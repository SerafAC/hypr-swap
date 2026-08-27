# hypr-swap

Alt-Tab-style workspace switcher with cross-monitor swapping for [Hyprland](https://hyprland.org/).

Hold a hotkey and an overlay lists every workspace with the windows it contains. Tap to move the
highlight, release to switch. If the selected workspace lives on another monitor, the two
workspaces trade places: the one you were on moves there, the one you picked comes to your monitor
and becomes active. A second hotkey drops you onto the lowest-numbered unused workspace on the
current monitor.

## Features

- **Hold-and-release switching** — the overlay stays up while the modifier is held and commits the
  highlighted workspace the moment you release it, like Alt-Tab for windows.
- **Cross-monitor swap** — selecting a workspace bound to another monitor swaps it with your
  current one, all-or-nothing: a partial failure is rolled back and reported.
- **Two presentations** — a flat list (workspace name plus window titles) or a grid of schematic
  miniatures showing each window's real position and proportion, with no screen capture involved.
- **Configurable order** — most-recently-used (default, so one tap bounces you back), compositor
  order, or grouped by monitor.
- **New-workspace hotkey** — jumps to the lowest unused workspace number on the focused monitor;
  a no-op if you're already on an empty one.
- **No key grabbing** — hotkeys are ordinary Hyprland binds delivered as named global shortcuts;
  the in-overlay keys are handled by the overlay's own exclusive keyboard focus.
- **Survives compositor restarts** — reconnects with backoff, rebuilds its state, and re-registers
  its shortcuts without being restarted.

## Requirements

- Hyprland ≥ 0.55 (Wayland; other compositors are out of scope)
- Rust ≥ 1.96 to build
- System libraries: cairo, pango, pangocairo (development files)
- Optional: a desktop notification service (`notify-send`) — used for problems that need your
  attention; absence is fine

## Build

```bash
cargo build --release
./target/release/hypr-swap --help
```

## Setup

Start the daemon with your session and bind the two shortcuts in `hyprland.conf` (the key
combinations are yours to choose — these are suggestions):

```ini
exec-once = hypr-swap

# Hold ALT, tap TAB to browse, release ALT to switch
bind = ALT, TAB, global, hypr-swap:switcher
bind = SUPER, N, global, hypr-swap:new-workspace
```

Use `bind`, not `binde`. Either line may be left out. Verify registration with
`hyprctl globalshortcuts`. The key combinations above are suggestions — any combination works, and
[`docs/binds.md`](docs/binds.md) is the full reference for what each line does and why.

A modifier in the switcher bind is what makes hold-and-release work. Bound to a bare key with no
modifier there is no release to commit on, so the overlay falls back to **sticky mode**: it stays
open, navigation works as usual, `Enter` commits and `Escape` cancels.

### In-overlay keys (fixed)

| Key | Action |
|---|---|
| `Tab`, `Right`, `Down` | Next entry (wraps) |
| `Shift+Tab`, `Left`, `Up` | Previous entry (wraps) |
| `Escape` | Cancel |
| `Enter` | Commit (sticky mode) |

## Configuration

Optional — with no file present the defaults below apply. The file is
`$XDG_CONFIG_HOME/hypr-swap/config.toml` (or pass `--config <path>`):

```toml
presentation = "list"     # "list" | "grid"
placement    = "active"   # "active" (monitor) | "all" (monitors)
order        = "mru"      # "mru" | "compositor" | "monitor"
```

An invalid value is reported and falls back to that setting's default; the daemon keeps running.
Diagnostics go to stderr. Exit codes: `0` success, `2` usage error, `3` compositor unreachable at
start-up (or a second instance already running).

## Development

The project is spec-driven: requirements, architecture decisions, and contracts live under
[`specs/001-workspace-swap-overlay/`](specs/001-workspace-swap-overlay/), governed by the project
[constitution](.specify/memory/constitution.md).

```bash
cargo test --lib                # unit tests — no compositor needed
cargo test --test 'e2e_*'       # E2E — needs a live Hyprland session and `foot`
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The E2E suite starts its own nested Hyprland instance with headless outputs and drives it with
real injected key events; it never touches your session.

## License

MIT
