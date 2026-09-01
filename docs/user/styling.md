---
title: Appearance and themes
description: The complete catalogue of overlay colours, fonts and dimensions — every key, its form, its range and its default.
---

Everything about how the overlay *looks* is one `theme` name plus an optional `[style]` table. This
page is the whole catalogue: with it alone you can assemble a complete custom appearance without
reading any source.

## The three-step chain

Every visual value is resolved once, at start-up, along one chain:

```text
[style] override  →  named theme (colours only)  →  built-in default
```

So an override wins over the theme, the theme wins over the default, and a value you do not mention
falls through to whatever the next step supplies. `[style]` may be given with no `theme` at all, in
which case the overrides land on top of the default theme.

Two consequences worth stating outright:

- **A theme is a palette and nothing else.** It can change colours; it cannot change a font or a
  dimension. Switching theme therefore recolours the overlay and never moves it — `dark` and
  `light` produce a surface of exactly the same size and position.
- **A dimension outside its range is clamped, not rejected.** The clamp is reported, the nearer
  bound is used, and everything else in your file still applies.

```toml
theme = "light"

[style]
highlight        = "#c04a2f"
font_family      = "JetBrains Mono"
text_size        = 0.85
width_fraction   = 0.95
```

## Colours

Eleven keys. Each accepts `#rgb`, `#rrggbb` or `#rrggbbaa`, and the two columns are what each
built-in theme sets it to.

::include[../../specs/002-overlay-visuals/contracts/style-values.md#colours]

## The built-in themes

There are two, and their values are the two columns of the table above: `theme = "dark"` (the
default) and `theme = "light"`. Naming one sets all eleven colours at once; any of them can still
be overridden individually on top of it. An unknown name is reported, the default applies, and
every other setting in your file still applies.

`text_highlighted` is white in both, because the highlight stays a saturated blue in both.

Contrast is not validated. A low-contrast or fully transparent combination renders exactly as
asked — if the overlay comes up unreadable, that is the palette you wrote.

## Fonts

::include[../../specs/002-overlay-visuals/contracts/style-values.md#fonts]

## Dimensions

Ten values, each a number with a documented range. These are the ones that are clamped rather than
rejected, so an out-of-range value is safe to experiment with — you will be told what was used.

::include[../../specs/002-overlay-visuals/contracts/style-values.md#geometry]

## What is deliberately not a setting

::include[../../specs/002-overlay-visuals/contracts/style-values.md#values-that-are-deliberately-absent]
