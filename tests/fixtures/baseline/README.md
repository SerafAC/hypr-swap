# Pre-feature visual baseline

What the overlay looked like immediately before feature 002 (`specs/002-overlay-visuals`) turned
every colour and geometry constant into a resolved style value. Every "unchanged from before this
feature" assertion — FR-049a, SC-018, SC-019 — compares against these files.

| File | Holds |
|---|---|
| `style.json` | The eleven palette colours, the font family, the renderer's drawing scalars and the ten geometry constants the pre-feature build used. `rgba` is authoritative; `hex` is its 8-bit round-trip. |
| `list.json` | The flat-list overlay: the surface `xywh` `hyprctl layers` reported, and the `ui::layout` metrics that asked for it. |
| `grid.json` | The same for the grid presentation, plus the first cell's label and miniature rectangles. |

Recorded by `tests/e2e_baseline.rs::record_pre_feature_baseline` on a headless output pinned to
1920×1080 at scale 1, so the numbers are a property of the code rather than of the machine.
`surface.x`/`y` are global layout coordinates and therefore *not* reproducible; compare
`x_on_monitor`/`y_on_monitor` instead.

**These files cannot be re-recorded.** Once the foundational phase lands, the renderer they measure
no longer exists. `cargo test --test e2e_baseline` still runs `style_baseline_matches_the_source`,
which fails if a pre-feature constant is edited without the baseline being retaken — that guard
disappears together with the constants it watches.
