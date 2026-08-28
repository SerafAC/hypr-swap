# Quickstart: Overlay Visuals

How to build, run and validate this feature by hand. Automated coverage is the unit tests and the
E2E table in [plan.md](./plan.md); this document covers what those deliberately do not — the pixel
properties that 001's R14 ruled out asserting mechanically, and the two success criteria that are
measurements of a real desktop rather than of this code.

## Prerequisites

Everything feature 001 requires, plus:

- System cairo built with PNG support (standard on every distribution; `cairo-rs`'s `png` feature
  binds to it).
- At least one icon set installed. Any freedesktop-layout set works; the survey below is more
  interesting on a set you actually use.

No new system library is needed for vector icons — `resvg` is pure Rust.

## Build and test

```bash
cargo build
cargo test --lib                     # unit tests; no compositor or display needed
cargo test --lib theme               # the style values, colour parsing, ranges, precedence
cargo test --lib icons               # the matching ladder, set lookup, cache, decoding
cargo test --test 'e2e_*'            # nested Hyprland; see CLAUDE.md for requirements
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Scenario 1 — icons appear, and defaults are unchanged (US1, SC-018)

```bash
rm -f ~/.config/hypr-swap/config.toml     # or move it aside
cargo run
```

Open a terminal, a browser and a file manager on different workspaces, then hold your switcher bind.

**Expect**: each window name preceded by its program's icon; workspace rows the same height as
before; the same number of rows visible. Colours, spacing and font identical to the pre-feature
overlay — the icons are the only visible difference (SC-018).

**Also confirm by eye**, since no automated test asserts these (FR-039, FR-051, FR-052):

- Icons are not stretched — a square icon is square.
- Icons are crisp, not blurry, on a scaled monitor.
- Program artwork keeps its own colours; it is not tinted to match the overlay.
- The icon is about the height of the text beside it.

## Scenario 2 — the placeholder (FR-041, SC-016)

```bash
XDG_DATA_HOME=/tmp/empty XDG_DATA_DIRS=/tmp/empty cargo run
```

**Expect**: every window shows the placeholder; every window name is still readable; the overlay
opens normally and **no** error is raised. This is the "no icon set installed" degradation.

## Scenario 3 — icons off (FR-056, SC-019)

```toml
icons = false
```

**Expect**: an overlay pixel-identical to the pre-feature one — no icons, no placeholders, and no
reserved space. Compare against a screenshot taken before the feature; this is the one place a
screenshot is a reasonable check, because it is a whole-image equality test rather than a
tolerance-based comparison.

## Scenario 4 — theming (US2, US4, SC-020)

```toml
theme = "light"
```

**Expect**: the whole overlay recoloured coherently in both presentations, and — importantly —
**the same size and position as under `dark`**. Switching theme must never move the layout (SC-023).
Confirm with the compositor's own view:

```bash
hyprctl layers | grep -A2 hypr-swap    # compare xywh under each theme
```

Then add an override and restart:

```toml
theme = "light"

[style]
highlight = "#c04a2f"
```

**Expect**: the highlight is your colour, everything else is still the light theme (FR-050).

## Scenario 5 — geometry (US5, SC-023)

```toml
[style]
text_line_height = 32
text_size        = 0.85
width_fraction   = 0.95
```

**Expect**: visibly larger entries, a wider overlay, and — with more workspaces than fit — the
overlay still capped, still scrolling, and the highlighted entry still in view. Entries must stay
the same size whether you have three workspaces or thirty (FR-053).

Now force a clamp:

```toml
[style]
grid_cell_width = 0
height_fraction = 5.0
```

**Expect**: two diagnostics on stderr naming the setting and the value used, and a perfectly usable
overlay (FR-054).

## Scenario 6 — one bad value does not poison the rest (FR-059, SC-022)

```toml
theme = "light"

[style]
highlight   = "not-a-colour"
font_family = "JetBrains Mono"
```

**Expect**: one diagnostic about `style.highlight`; the font override **still applied**; the light
theme still in effect; the daemon running normally.

## Scenario 7 — restart semantics (FR-060)

With the daemon running, edit any visual setting and open the overlay.

**Expect**: no change. Restart, open again: the change is there. There is no live reload, by design.

---

## Measurements, not assertions

These two success criteria are properties of a real desktop and are measured here rather than
asserted in CI.

### SC-012 — icon coverage on your system

Run the daemon with the paint records enabled (see research R22 for the gate), open the overlay with
a representative set of programs running, and count how many windows resolved a real icon versus the
placeholder.

**Target**: at least 90 % of windows of installed desktop programs show their own icon.

If the figure is low, the likely cause is the matching ladder in
[contracts/icon-lookup.md](./contracts/icon-lookup.md) — most often a program whose window class
matches no `StartupWMClass` and no entry id. Record the failing classes; research R21 lists
`initialClass` as the documented next avenue, deliberately not taken up front.

### SC-014 — does it actually help

The usability check feature 001 uses for SC-004: with at least ten workspaces open, ask someone to
find the one holding a named program, once with `icons = true` and once with `icons = false`.

**Target**: faster with icons. This is a judgement, not a stopwatch threshold.

## Soak

Alongside feature 001's 8-hour soak (SC-007), confirm the two icon guarantees that only show up over
time:

- **SC-017** — open the overlay 100+ times across a session with programs starting and stopping.
  Memory attributable to icons should not grow; each distinct program should resolve exactly once.
- **FR-043b** — nothing appears under `~/.cache` or any other location. The cache is memory-only.
