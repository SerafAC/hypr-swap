---
title: Configuration
description: Every setting the configuration file accepts, its values, its default, and how an invalid one is handled.
---

Two things are configurable, and they live in different places. The **key combinations** are
Hyprland's business and go in `hyprland.conf` — see [binding the shortcuts](./binds.md). Everything
else lives in one optional TOML file.

## Where the file goes

`$XDG_CONFIG_HOME/hypr-swap/config.toml`, falling back to `~/.config/hypr-swap/config.toml` when
`XDG_CONFIG_HOME` is unset, or wherever `--config <path>` points. **No file is a normal state**:
with none present every setting takes its default and nothing is reported.

The file is read **once, at start-up**. There is no live reload — restart the daemon to apply a
change.

## An invalid value costs you only that value

Validation is per setting, which is worth knowing before you experiment. A value the daemon cannot
make sense of is named on standard error, *that one setting* falls back to its default, every other
setting in the file still applies, and the daemon keeps running. A dimension outside its range is
**clamped** to the nearer bound rather than rejected. A file that is not valid TOML at all cannot
be blamed on one setting, so the parse error is reported with its line and column and the whole
file falls back.

## The behavioural settings

These decide what the overlay shows and in what order. They are the settings feature 001
introduced, and this is its contract, included here rather than restated:

::include[../../specs/001-workspace-swap-overlay/contracts/config.md]

## The visual settings

Icons, the icon set and the overlay's palette. The `[style]` table is large enough to have its own
page — see [appearance and themes](./styling.md) for the catalogue of what may go in it.

::include[../../specs/002-overlay-visuals/contracts/config.md]

## A worked example

```toml
presentation = "grid"      # miniatures rather than a flat list
placement    = "all"       # on every monitor at once
order        = "monitor"   # grouped by the monitor each workspace belongs to

icons    = true
icon_set = "Papirus-Dark"
theme    = "light"

[style]
highlight   = "#c04a2f"    # one colour, on top of the light palette
font_family = "JetBrains Mono"
text_size   = 0.85
```

Everything under `[style]` is optional and independent; anything you leave out comes from the named
theme, and anything the theme does not set comes from the built-in default.
