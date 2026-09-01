---
title: Program icons
description: How each window's icon is found, how to choose an icon set, and why icon_set has nothing to do with theme.
---

Every window in the overlay carries its own program's icon, taken from your desktop's icon set. A
window whose program cannot be identified gets the built-in placeholder, and nothing else about the
layout changes.

## `icon_set` is not `theme`

This is the one thing worth getting straight before anything else, because the names look related
and are not:

| Setting | Selects | Comes from |
|---|---|---|
| `icon_set` | whose **program artwork** is drawn | a freedesktop icon set installed on your system |
| `theme` | the **overlay's own colours** | a built-in palette that ships with hypr-swap |

They are independent. Setting one never affects the other, and neither falls back to the other — a
missing icon set does not change the overlay's colours, and a `theme` name is never tried as an
icon set.

## Turning icons off

```toml
icons = false
```

This draws no icons and no placeholders, and reserves no space for either, so the layout is exactly
what it was before icons existed. It also suppresses icon resolution entirely: with icons off there
is no desktop-entry scan and no icon-set lookup at all.

## Choosing an icon set

```toml
icon_set = "Papirus-Dark"
```

Any installed set in the freedesktop layout works — Papirus, Adwaita, breeze, and so on. Leave the
setting out and hypr-swap follows **your desktop's own configured set**, which is what you almost
certainly want.

That set is read from `gtk-4.0/settings.ini`, then `gtk-3.0/settings.ini`, under each configuration
root (`$XDG_CONFIG_HOME`, then `$XDG_CONFIG_DIRS`), taking `gtk-icon-theme-name` from the
`[Settings]` group. That is the file the tools people use to set an icon set on a bare Wayland
session actually write. `gsettings` is deliberately not consulted, because a minimal session need
not have a running dconf.

The difference between the two sources shows up in what gets reported:

- a set **you** named here that is not installed is reported, and the standard default set
  (`hicolor`) is used instead;
- a set your **desktop** names that is not installed falls back silently, because you did not ask
  for it.

With no set at all — none installed, nothing configured — every window shows the placeholder. That
is the whole cost.

## How a window's icon is found

The window gives its application class; hypr-swap turns that into a program, and the program into
an icon file. The ladder is tried in order and stops at the first match:

1. a desktop entry whose file name matches the class;
2. a desktop entry whose `StartupWMClass` matches the class;
3. a desktop entry whose `Exec` names a program matching the class;
4. the class treated as an icon name directly;
5. the built-in placeholder.

The result is resolved **once per program** and cached for the life of the daemon, so opening the
overlay repeatedly costs no further lookups.

Inside the chosen set, a directory is picked by how well its nominal size fits the size the icon is
being drawn at, following the set's own `index.theme`, and `Inherits` is followed when the set does
not have the icon itself. Both PNG and SVG are handled; SVG is rasterised at the size actually
needed rather than scaled from a bitmap.

## When an icon looks wrong

**Every window shows the placeholder.** No icon set is installed, or none is discoverable. Install
one, or name it with `icon_set`.

**One program shows the placeholder.** Its class does not match any desktop entry. Check what the
window actually reports:

```bash
hyprctl clients | grep -i class
```

If that class is not the desktop entry's file name, the entry usually needs a `StartupWMClass` line
naming it — that is a fix in the program's own `.desktop` file, not in hypr-swap.

**One icon is missing or garbled although the file exists.** The file is malformed or unreadable.
That is reported once per program on standard error and the placeholder is drawn in its place; the
overlay stays perfectly usable, which is why it does not raise a notification. See
[troubleshooting](./troubleshooting.md) for where that output goes.

**The icons are the wrong style.** You are getting your desktop's configured set. Name the one you
want with `icon_set` — and remember it is not `theme`.
