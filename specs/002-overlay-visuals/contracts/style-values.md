# Contract: Style values

This document is the FR-061 catalogue: every visual setting, its accepted form, its valid range
where one applies, and its default. A user can write a complete custom appearance from this page
alone, without reading the source (SC-024).

The ranges and defaults here are authoritative in `theme.rs`. A unit test walks this catalogue
against the values `theme.rs` actually resolves, so a setting cannot be documented but inert, nor
implemented but undocumented (SC-025).

All keys below live under `[style]` in the configuration file. See
[config.md](./config.md) for the file's shape.

---

## Colours

**Form**: `#rgb`, `#rrggbb`, or `#rrggbbaa`. Alpha defaults to opaque when omitted. No other
notation is accepted (research R25).

**Set by a theme**: yes — these eleven values are what a built-in theme sets (FR-045, FR-049).

| Key | Draws | Default (`dark`) | `light` |
|---|---|---|---|
| `backdrop` | Overlay background | `#17171ced` | `#f7f7faed` |
| `highlight` | Highlighted entry background | `#336bb8` | `#2e70cc` |
| `active_mark` | Active-workspace mark | `#6bb873` | `#298c47` |
| `text` | Primary entry text (workspace name) | `#ebebf0` | `#1c1c24` |
| `text_highlighted` | Primary text on the highlighted entry | `#ffffff` | `#ffffff` |
| `text_dim` | Secondary text (window names) | `#a8a8b3` | `#595966` |
| `text_dim_highlighted` | Secondary text on the highlighted entry | `#dbe6f5` | `#e0ebfa` |
| `miniature` | Miniature background | `#292930` | `#e6e6ed` |
| `window` | Tiled window rectangle fill | `#4d5261` | `#c2c7d6` |
| `window_floating` | Floating window rectangle fill | `#61667a` | `#adb5c9` |
| `window_edge` | Window rectangle edge | `#858c9e` | `#737a8f` |

## Built-in themes

`theme = "dark"` (the default) and `theme = "light"`, whose values are the two columns above. A
built-in theme is a **palette and nothing else**: it sets those eleven colours and never a font or
a geometry value, so switching theme recolours the overlay and never moves it (FR-049, SC-023).
`text_highlighted` is white in both, because the highlight stays a saturated blue in both.

An unknown name is reported, the default theme applies, and every other setting still applies
(FR-058). Any of the eleven can still be overridden on top of a theme (FR-050).

The `dark` defaults are the constants `ui/render.rs` uses today, so an unconfigured overlay is
unchanged (FR-049a, SC-018). Those constants are floats; the hex above is their 8-bit round-trip,
rounded half away from zero. If the two ever disagree, the constants win and this table is wrong.

**The `dark` theme must therefore be built from the float constants, not by parsing the hex above.**
Two channels land exactly on a half-step — `text_dim`'s blue (`0.70` → 178.5) and `window`'s red
(`0.30` → 76.5) — so a half-to-even convention would render them one value lower than the renderer
does. Going through the floats has no tie to break, which is what makes FR-049a's "byte for byte"
claim safe to rely on. The recorded pre-feature values are in
`tests/fixtures/baseline/style.json`, whose `rgba` field is authoritative for exactly this reason.

Contrast is not validated — a low-contrast or fully transparent combination renders as asked (spec
Assumptions).

---

## Fonts

**Set by a theme**: no. Fonts are shared defaults, not per-theme values (FR-049, research R24). They are
reachable only as overrides.

| Key | Form | Default | Range | Notes |
|---|---|---|---|---|
| `font_family` | string | `Sans` | — | Any family name. An absent family is substituted by the platform; text stays readable and nothing is reported (US4-AS5). |
| `text_size` | float | `0.78` | `0.3..=1.0` | Fraction of the row's text height. The row height follows it, so text is never clipped by its own row. |

---

## Geometry

**Set by a theme**: no — same reason as fonts. Values are logical units, scaled per monitor by the
existing rule (FR-055), so no per-monitor variants exist.

An out-of-range value is **clamped** to the nearer bound and reported; it is not rejected and does
not fall back to the default (FR-054, research R26).

| Key | Form | Default | Range | Governs |
|---|---|---|---|---|
| `text_line_height` | integer | `20` | `8..=200` | Entry text height; drives row height |
| `row_padding` | integer | `8` | `0..=100` | Vertical padding within a row |
| `overlay_padding` | integer | `12` | `0..=200` | Overlay's outer padding |
| `width_fraction` | float | `0.8` | `0.1..=1.0` | Overlay width cap, as a fraction of the monitor |
| `height_fraction` | float | `0.8` | `0.1..=1.0` | Overlay height cap, as a fraction of the monitor |
| `grid_cell_width` | integer | `240` | `40..=2000` | Miniature cell width |
| `grid_cell_height` | integer | `135` | `40..=2000` | Miniature cell height |
| `grid_gap` | integer | `12` | `0..=200` | Gap between grid cells |
| `corner_radius` | float | `0.28` | `0.0..=1.0` | Corner rounding, as a fraction of row height |
| `mark_width` | float | `0.12` | `0.0..=1.0` | Active-mark width, as a fraction of row height |

### Why these ranges

The bounds are not cosmetic; they are what makes SC-023 provable — that no valid combination can
produce an unusable overlay:

- `width_fraction` and `height_fraction` cap at `1.0`, so the overlay can never exceed its monitor.
- `text_line_height` and the grid cell sizes have non-zero minimums, so the viewport arithmetic in
  `ui/layout.rs` cannot divide by zero and a row can always hold its text.
- Nothing here can make entry size depend on the number of workspaces — entries stay fixed-size and
  the overlay scrolls, exactly as FR-019 requires (FR-053).

---

## Values that are deliberately absent

Listed so their absence reads as a decision rather than an oversight (Principle II):

| Not a setting | Why |
|---|---|
| Icon size | Follows the resolved text height (FR-052), so it cannot disagree with the row it sits in. |
| Icon tint or recolouring | Program artwork is drawn as supplied (FR-051). Only the placeholder follows `text`. |
| Per-monitor or per-presentation values | One appearance applies everywhere (FR-048). |
| Background blur, shadows, gradients, animation | Out of scope — this feature makes existing drawing configurable, it adds no new visual elements (spec Assumptions). |
| Resolution timing | Not user-facing; settled in research R27 and the spec's Assumptions. |
| A second colour notation | One form, by decision (research R25). |
