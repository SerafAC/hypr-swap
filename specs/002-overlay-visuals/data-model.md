# Data Model: Overlay Visuals

Entities added or changed by this feature. Everything from feature 001's
[data-model.md](../001-workspace-swap-overlay/data-model.md) is unchanged unless named here.

Types are described by shape and rule, not by Rust syntax; the field names are the ones the
implementation should use so that the contract documents, the code and the tests agree.

---

## Theme (new) — `theme.rs`

A named palette. **Colours only** (FR-049, research R24).

| Field | Type | Notes |
|---|---|---|
| `name` | string | The value a user writes as `theme = "…"`. Identity. |
| `backdrop` | Colour | Overlay background |
| `highlight` | Colour | Highlighted entry background |
| `active_mark` | Colour | Active-workspace mark |
| `text` | Colour | Primary entry text |
| `text_highlighted` | Colour | Primary text on the highlighted entry |
| `text_dim` | Colour | Secondary text (window names) |
| `text_dim_highlighted` | Colour | Secondary text on the highlighted entry |
| `miniature` | Colour | Miniature background |
| `window` | Colour | Tiled window rectangle fill |
| `window_floating` | Colour | Floating window rectangle fill |
| `window_edge` | Colour | Window rectangle edge |

**Rules**

- Built-in themes are compiled in; at least `dark` and `light` exist (FR-049).
- `dark` is the default, and its eleven values are exactly today's constants in `ui/render.rs`, so
  the default configuration reproduces the prior appearance (FR-049a, SC-018).
- A theme defines no font and no geometry. Selecting a different theme cannot change layout
  (SC-023) — a structural property, not something tests must police.
- An unknown name falls back to the default and is reported (FR-058).

## Colour (new) — `theme.rs`

Straight RGBA, each channel `0.0..=1.0`, matching the `Rgba` tuple `ui/render.rs` already paints
with.

**Rules**

- Parsed from exactly one textual form: `#rgb`, `#rrggbb`, or `#rrggbbaa` (research R25). Alpha
  defaults to fully opaque when absent.
- Parsing is total: any other input is an invalid value under FR-059 — reported, that one setting
  falls back, every other setting still applies.
- Fully transparent and low-contrast values are legal. The application validates form and range, not
  aesthetics (spec Assumptions).

## Style (new) — `theme.rs`

The fully resolved appearance handed to the renderer: one palette plus the font and geometry values.
This is what `ui/` sees; it never sees overrides or theme names.

| Field | Type | Source |
|---|---|---|
| `palette` | Theme | Resolved per FR-050 |
| `font_family` | string | Default `Sans` — today's hard-coded family |
| `text_size` | float | Fraction of the row's text height; default `0.78` (today's `FONT_FRACTION`) |
| `geometry` | Geometry | See below |

**Rules**

- Produced once at start-up by `resolve(config)`; immutable thereafter (FR-060).
- Every field is non-optional. Resolution has already applied the FR-050 chain, so the renderer has
  no defaults of its own — the single-source-of-truth requirement of Principle III.

## Geometry (new) — `theme.rs`, consumed by `ui/layout.rs`

The ten values of FR-047, each with a documented range (FR-054). These are today's `pub const`s in
`ui/layout.rs`, which become fields.

| Field | Default (today's constant) | Range |
|---|---|---|
| `text_line_height` | 20 | 8..=200 |
| `row_padding` | 8 | 0..=100 |
| `overlay_padding` | 12 | 0..=200 |
| `width_fraction` | 0.8 | 0.1..=1.0 |
| `height_fraction` | 0.8 | 0.1..=1.0 |
| `grid_cell_width` | 240 | 40..=2000 |
| `grid_cell_height` | 135 | 40..=2000 |
| `grid_gap` | 12 | 0..=200 |
| `corner_radius` | 0.28 | 0.0..=1.0 |
| `mark_width` | 0.12 | 0.0..=1.0 |

Ranges are authoritative in `theme.rs` and reproduced in
[contracts/style-values.md](./contracts/style-values.md); a unit test asserts the two agree, which
is what keeps SC-025 honest.

**Rules**

- Values are logical units, scaled per monitor by the existing rule in `ui/layout.rs` (FR-055), so
  no per-monitor variants exist.
- An out-of-range value is clamped to the nearer bound and reported (FR-054, FR-059, research R26).
- Ranges are chosen so that no combination can exceed the monitor, hide the highlighted entry, or
  make entry size depend on workspace count (SC-023, FR-053).

## StyleOverride (new) — `config.rs` → `theme.rs`

A user-supplied value for one style value. Independent of every other override: an invalid one
affects only itself (FR-059).

**Resolution order** (FR-050), expressed once in `theme.rs::resolve`:

```text
override  →  named theme (colours only)  →  default
```

---

## Program (new) — `icons/mod.rs`

The unit an icon is resolved for and cached against.

| Field | Type | Notes |
|---|---|---|
| `class` | string | `model::Window.class`, already deserialised. Cache key. |

**Rules**

- One resolution per distinct class per run, reused for every window of that program and across
  overlay openings (FR-042).
- Identity is the class as reported, compared case-sensitively for cache lookup; case-insensitive
  comparison happens inside the matching rule only (research R21).

## Icon (new) — `icons/mod.rs`

What gets drawn for a program. A resolution result, not merely an image — "we tried and failed" is a
first-class value so FR-042's once-per-run guarantee also holds for failures.

| Variant | Meaning |
|---|---|
| `Resolved(surface)` | The program's own icon, decoded to a cairo `ImageSurface` |
| `Placeholder` | No entry matched, no file found, or decoding failed (FR-041, FR-044) |

**Rules**

- The placeholder ships with the application and is always available (spec Assumptions), and it is
  the only icon a theme may recolour — program artwork never is (FR-051).
- A malformed file is reported once and then cached as `Placeholder`, so the diagnostic cannot
  repeat on every overlay opening (FR-044).
- Held in memory only; never written to disk; dropped on exit and on connection loss (FR-043b,
  research R28).
- Drawn scaled to its slot without aspect distortion, at device resolution (FR-039).

## IconStore (new) — `icons/mod.rs`

The cache and its two operations.

| Operation | When | Behaviour |
|---|---|---|
| `ensure(classes)` | Start-up, and on the world-rebuild path (research R27) | Resolves only classes not already present |
| `get(class)` | Paint | Pure lookup. Never resolves, never touches the filesystem (FR-043) |

**Rules**

- A class absent at paint time yields the placeholder for that opening; the overlay is never held
  back and is never repainted to swap the icon in (FR-043a).
- Empty when icons are disabled — `ensure` is not called at all, so no filesystem work happens
  (FR-056).

## DesktopEntryIndex (new) — `icons/entries.rs`

Built once at start-up from the desktop-entry search path. Four keys per entry: `Icon`,
`StartupWMClass`, `Name`, `NoDisplay`.

**Rules**

- The class → icon-name rule is the five-step ladder in research R21, stopping at the first hit.
- `NoDisplay=true` entries are indexed but rank last, so a real launcher beats a hidden one.
- No match is a normal outcome yielding `Placeholder`, not a reported failure (FR-041).
- The rule is a pure function from `(class, index)` to an optional icon name — it is unit-tested
  without a filesystem.

## IconSet (new) — `icons/iconset.rs`

The installed icon set a name resolves within, plus its inheritance chain.

| Field | Notes |
|---|---|
| `name` | From configuration, else the desktop's configured set, else the standard default (FR-057) |
| `directories` | Parsed from `index.theme`: `Size`, `Scale`, `Type`, `MinSize`, `MaxSize`, `Threshold` |
| `inherits` | Followed in order, terminating at `hicolor` |

**Rules**

- Independent of the overlay `Theme`. Neither setting changes the other (FR-057); the module names
  keep the distinction visible.
- Directory choice for a requested size is a pure function over parsed metadata (research R20).
- An unknown or absent set falls back to the default and is reported (FR-057, FR-024).

---

## Configuration (changed) — `config.rs`

Feature 001's three settings, plus four visual ones. Defaults live only in `theme.rs`; `config.rs`
parses and delegates.

| Setting | Type | Default | Requirement |
|---|---|---|---|
| `icons` | bool | `true` | FR-056 |
| `icon_set` | string | the desktop's configured set | FR-057 |
| `theme` | string | `dark` | FR-049 |
| `[style]` overrides | table | empty | FR-050 |

**Rules**

- Read once at start-up; changes take effect on the next start (FR-060).
- One invalid setting falls back alone, is reported naming the setting, what was wrong and the value
  used, and every other setting still applies (FR-059, SC-022).
- The full schema is [contracts/config.md](./contracts/config.md); the value catalogue required by
  FR-061 is [contracts/style-values.md](./contracts/style-values.md).

## Entry (changed) — `ui/`

The per-workspace row or cell the renderer paints. Unchanged except that each of its windows now
carries the icon resolved for that window's program, so the renderer receives everything it needs
and performs no lookup of its own.
