# Contract: Icon lookup

How a window becomes an icon, and the fixture format the E2E suite uses to make that deterministic.

This is a contract because FR-057 makes the icon set user-visible and FR-041 makes the failure mode
user-visible: users need to know why a window shows the placeholder, and tests need a way to force
each outcome.

## The chain

```text
window.class
   → desktop entry            (icons/entries.rs, research R21)
   → icon name  (Icon= key)
   → icon file                (icons/iconset.rs, research R20)
   → cairo surface            (icons/decode.rs, research R18/R19)
```

Any step failing yields the placeholder (FR-041). That is a normal outcome and raises no desktop
notification.

## Step 1 — class to desktop entry

Search path: `$XDG_DATA_HOME/applications`, then each `$XDG_DATA_DIRS/applications`. Four keys are
read per entry: `Icon`, `StartupWMClass`, `Name`, `NoDisplay`.

Matched in this order, first hit wins:

| # | Rule | Example |
|---|---|---|
| 1 | `StartupWMClass` equals the class, case-sensitive | `StartupWMClass=foot` ← class `foot` |
| 2 | `StartupWMClass` equals it, case-insensitive | `StartupWMClass=Foot` ← class `foot` |
| 3 | Entry id (basename without `.desktop`) equals it, case-insensitive | `firefox.desktop` ← class `Firefox` |
| 4 | Entry id's last dot-separated component equals it, case-insensitive | `org.gnome.Nautilus.desktop` ← class `nautilus` |
| 5 | `Name` equals it, case-insensitive | `Name=Foot` ← class `foot` |

Entries with `NoDisplay=true` are indexed but rank last, so a real launcher beats a hidden one. No
match is unresolvable → placeholder.

## Step 2 — icon name to file

The freedesktop icon-set lookup, implemented directly (research R20):

- Search path: `$XDG_DATA_HOME/icons`, each `$XDG_DATA_DIRS/icons`, `~/.icons`, `/usr/share/pixmaps`.
- The set is `icon_set` if given, else the desktop's configured set, else the standard default
  (FR-057).
- `index.theme` gives the directory list and each directory's `Size`, `Scale`, `Type`, `MinSize`,
  `MaxSize`, `Threshold`. Directory choice for the requested size is a pure function over that
  metadata.
- `Inherits` is followed in order, terminating at `hicolor`.
- The requested size is the resolved icon slot — the text height (FR-052) times the monitor scale,
  so a scaled monitor asks for a larger icon rather than upscaling a small one (FR-039).

## Step 3 — file to surface

| Extension | Path | Notes |
|---|---|---|
| `.png` | cairo's own loader | No new dependency (research R19) |
| `.svg` | `resvg` → pixmap → cairo surface | No text or `svgz` support (research R18) |
| anything else | unresolvable → placeholder | Explicitly per FR-040a |

A malformed or unreadable file is reported **once** on stderr and cached as the placeholder, so the
diagnostic cannot repeat on every overlay opening (FR-044).

## Caching

- One resolution per distinct class per run, reused for every window of that program and across
  openings (FR-042).
- Failures are cached too — "we tried and failed" is a result, which is what makes FR-044's
  report-once guarantee hold.
- Memory only. No on-disk cache, ever (FR-043b). Dropped on exit and on connection loss (research
  R28).
- With `icons = false`, none of this runs at all (FR-056).

## Test fixture format

E2E stages a synthetic root into a temporary `XDG_DATA_HOME` so no assertion depends on what the
developer has installed (research R22):

```text
<tmp>/applications/
    fixture-alpha.desktop        # StartupWMClass=fixturealpha, Icon=fixture-alpha
    fixture-beta.desktop         # StartupWMClass=fixturebeta,  Icon=fixture-beta
    fixture-broken.desktop       # StartupWMClass=fixturebroken, Icon=fixture-broken
<tmp>/icons/FixtureSet/
    index.theme                  # Directories=48x48/apps,scalable/apps
    48x48/apps/fixture-beta.png  # a valid PNG          → exercises R19
    scalable/apps/fixture-alpha.svg  # a valid SVG      → exercises R18
    48x48/apps/fixture-broken.png    # truncated bytes  → exercises FR-044
<tmp>/icons/FixtureSetTwo/         # a second set, for the icon_set switching test
```

Windows are spawned with matching classes by the existing `tests/e2e/clients.rs` helper. The
outcomes the fixtures force, one per requirement:

| Fixture | Forces | Covers |
|---|---|---|
| `fixture-alpha` | vector icon resolves | FR-040a |
| `fixture-beta` | raster icon resolves | FR-040a |
| `fixture-broken` | decode fails, reported once | FR-044 |
| a class with no entry | no match | FR-041 |
| empty `XDG_DATA_*` root | no set installed at all | SC-016 |
| `FixtureSetTwo` | `icon_set` selects a different set | FR-057 |

## Observability

Under the environment gate described in research R22, the daemon emits one record per painted entry
naming what it resolved and drew — the file chosen, or that the placeholder was used, or that a
miniature rectangle shed content. This is what makes the visual requirements assertable through
stderr rather than by screenshot. The gate is inert in normal operation, matching the fault-injection
hook `hypr/ipc.rs` already carries for feature 001's rollback tests.
