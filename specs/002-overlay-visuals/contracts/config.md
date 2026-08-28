# Contract: Configuration — visual settings

Extends feature 001's [config contract](../../001-workspace-swap-overlay/contracts/config.md). That
document's rules still hold: one user-editable TOML file, read once at start-up, per-setting
fallback, and no live reload.

This document adds the four settings this feature introduces. The value catalogue they draw on is
[style-values.md](./style-values.md).

## Schema

```toml
# --- feature 001, unchanged ---
presentation = "list"        # "list" | "grid"
placement    = "active"      # "active" | "all"
order        = "mru"         # "mru" | "compositor"

# --- feature 002 ---
icons    = true              # FR-056
icon_set = "Papirus-Dark"    # FR-057; omit to follow the desktop's configured set
theme    = "dark"            # FR-049; "dark" | "light" | any built-in name

[style]                      # FR-050 — every key optional, each independent
highlight   = "#3569b8"
text        = "#ebebf0"
font_family = "JetBrains Mono"
text_size   = 0.9
text_line_height = 28
width_fraction   = 0.9
```

## Settings

| Key | Type | Default | Requirement |
|---|---|---|---|
| `icons` | boolean | `true` | FR-056 |
| `icon_set` | string | the desktop's configured icon set, else the standard default set | FR-057 |
| `theme` | string | `"dark"` | FR-049 |
| `[style].*` | see [style-values.md](./style-values.md) | per value | FR-050 |

### `icons`

`false` draws no icons and no placeholders, and reserves no space for either — the layout is exactly
the pre-feature layout (FR-056, SC-019). It also suppresses all icon resolution: with icons off the
daemon performs no desktop-entry scan and no icon-set lookup at all.

### `icon_set`

Names an installed icon set. Omitted, the application follows the desktop's configured set; if that
is not discoverable, it uses the standard default set (FR-057). A named set that is not installed is
reported and falls back to the same default while every other setting still applies (FR-024).

This is **not** the overlay theme. `icon_set` selects whose program artwork is drawn; `theme` selects
the overlay's own colours. Neither affects the other (FR-057).

### `theme`

Names a built-in theme. A built-in theme is a palette — colours only. It cannot change fonts or
geometry, so switching theme can never move the layout (FR-049, SC-023). An unknown name is reported
and falls back to the default theme (FR-058).

### `[style]`

Per-key overrides, each independent. Resolution is one chain (FR-050):

```text
[style] override  →  named theme (colours only)  →  default
```

Overrides may be given without a `theme`, in which case they apply on top of the default theme
(US4-AS2).

## Validation and diagnostics

Per FR-059, and consistent with feature 001's FR-024:

- An unparseable value is reported naming the setting, what was wrong with it, and the value used
  instead. **Only that setting** falls back; every other setting still applies (SC-022).
- A geometry value outside its documented range is **clamped** to the nearer bound rather than
  rejected, and the clamp is reported the same way (FR-054).
- An unknown `theme` or `icon_set` name falls back and is reported (FR-058, FR-024).
- Diagnostics go through `diag.rs` like every other message; none of these conditions raises a
  desktop notification, since all are recovered from automatically (FR-031).

Example, for `highlight = "not-a-colour"` and `grid_cell_width = 0`:

```text
hypr-swap: config: style.highlight: expected #rgb, #rrggbb or #rrggbbaa, got "not-a-colour"; using #3466b8
hypr-swap: config: style.grid_cell_width: 0 is below the minimum 40; using 40
```

The exact record format is owned by feature 001's
[diagnostics contract](../../001-workspace-swap-overlay/contracts/diagnostics.md); these are
illustrations of content, not a new format.

## Compatibility

Every setting here is optional with a documented default, so a configuration file written for
feature 001 remains valid and produces the pre-feature appearance plus icons (SC-018). Running with
no configuration file at all is still fully supported (FR-023).
