# Contract: Configuration

Covers FR-008a, FR-016, FR-017, FR-023, FR-024.

## Location

`$XDG_CONFIG_HOME/hypr-swap/config.toml`, falling back to `~/.config/hypr-swap/config.toml` when
`XDG_CONFIG_HOME` is unset. Read once at start-up; live reload is out of scope (spec Assumptions).
A missing file is normal and produces no diagnostic — the application runs on defaults (FR-023).

## Schema

TOML, flat, three optional keys. Unknown keys are reported on stderr and ignored.

```toml
# How workspaces are presented in the overlay.
#   "list" — one row per workspace: its name followed by the titles of its windows
#   "grid" — a miniature of each workspace's layout, its name underneath
presentation = "list"          # default: "list"

# Where the overlay is shown.
#   "active" — only on the monitor holding the focused workspace
#   "all"    — on every connected monitor, all showing the same highlight
placement = "active"           # default: "active"

# The order entries appear in.
#   "mru"        — most recently active first; the highlight opens on the second entry
#   "compositor" — the compositor's stable order; the highlight opens on the active workspace
#   "monitor"    — grouped by monitor, stable within each group; highlight on the active workspace
order = "mru"                  # default: "mru"
```

| Key | Type | Values | Default |
|---|---|---|---|
| `presentation` | string | `list`, `grid` | `list` |
| `placement` | string | `active`, `all` | `active` |
| `order` | string | `mru`, `compositor`, `monitor` | `mru` |

The defaults are the ones FR-023 documents: flat list, active monitor only, MRU order.

## Invalid values (FR-024)

Validation is **per setting**. An invalid or misspelled value affects only its own key:

1. The offending setting is named on stderr and in a desktop notification (FR-029, FR-030).
2. That setting falls back to its default.
3. Every other setting keeps its user-supplied value.
4. The application continues running.

A file that is not valid TOML at all cannot be attributed to one setting: the parse error is
reported with its line and column, all three settings fall back to their defaults, and the
application continues.

Example — given `presentation = "tiles"` and `order = "compositor"`, the application runs with the
**list** presentation (fallback, reported) and **compositor** order (honoured).

## Not configurable

Deliberately absent, and each for a reason recorded in the spec or the constitution:

- **Key combinations** — they live in the compositor's configuration (FR-022,
  [shortcuts.md](./shortcuts.md)).
- **In-overlay keys** — fixed by FR-004a.
- **Theming, colours, fonts, animations** — out of scope (spec Assumptions).
- **Overlay size and entry size** — documented constants, not settings (FR-019). They live in one
  place in `ui/layout.rs`:

  | Constant | Value | Why |
  |---|---|---|
  | Overlay cap | 80 % of monitor width × 80 % of monitor height | The documented fraction FR-019 requires |
  | List row height | one text line + 8 px padding above and below | Fixed regardless of workspace count |
  | Grid cell | 240 × 135 logical px (16:9) + label line | Fixed; miniatures keep the monitor's aspect ratio |
  | Grid gap | 12 logical px | Separates cells, and insets a miniature from its highlight |
  | Scroll margin | 1 entry | The highlight never sits flush against a scrolled edge |

  All are multiplied by the monitor's `scale`. Entries are never scaled down to make the set fit —
  the overlay scrolls instead (FR-019).
