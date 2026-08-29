# Phase 0 Research: Overlay Visuals

Decisions are numbered continuing feature 001's sequence (which ended at R17), so a citation in a
code comment is unambiguous across both features.

Facts marked **[verified]** were checked on this machine during planning. Facts marked
**[assumed]** are reasoned from documentation and must be confirmed during implementation.

---

## R18 — Vector icon rasterisation (FR-040a, SC-012)

**Decision**: Add `resvg` as the only new direct dependency. Decode SVG into a `tiny_skia::Pixmap`,
convert once into a cairo `ImageSurface` at decode time, and cache the surface. Do not enable the
`text` or `svgz` features.

Why this is unavoidable: the development machine's configured icon set is Papirus-Dark, which
contains **3970 SVG files and zero PNG files** **[verified]**. A raster-only build would show the
placeholder for essentially every window, making SC-012 unreachable. The user chose raster + vector
during clarification with this trade-off stated.

Feature choice: `resvg` 0.48.1 declares `text`, `system-fonts`, `memmap-fonts` and `svgz` as opt-in,
with no default feature set **[verified]** via the crates.io sparse index. Leaving them off keeps
the tree small; the cost is that an icon containing a `<text>` element renders without that text.
Application icons essentially never carry live text, and FR-041's placeholder covers a total
failure, so this is the KISS choice.

**Rationale**: Rasterising SVG cannot be hand-rolled at any reasonable cost, so a dependency is
genuinely required rather than convenient — this is the entry in Complexity Tracking. `resvg` is
pure Rust, adds no system library, and hands back a plain pixel buffer, so it cannot conflict with
the `glib`/`cairo` generation this project already pins.

**Alternatives considered**: **`librsvg`**, which is installed system-wide at 2.62.3 **[verified]**
and renders directly into a `cairo::Context` — on paper the better fit, since we already own a
cairo context. Rejected because its Rust bindings carry their own pinned `glib` and `cairo`
generations; if those disagree with our `glib` 0.22 / `cairo-rs` 0.22, the tree holds two
incompatible `cairo::Context` types and the "renders straight into our context" advantage evaporates
— replaced by a version-skew problem on every future bump. **Raster-only** (offered and declined in
clarification) — the measurement above. **Shelling out** to `rsvg-convert` — a process spawn and an
output-parsing surface per icon, for no benefit.

**Pixel format note**: `tiny_skia::Pixmap` is premultiplied RGBA8; cairo `ARgb32` is premultiplied
BGRA on little-endian. A channel swap is required when building the surface **[verified in
implementation — T092]**: `icons/decode.rs::surface_from_pixmap` assembles cairo's native-endian
`u32` from the pixmap's `R, G, B, A` bytes, and
`decode::tests::a_valid_svg_is_rasterised_at_the_slot_size` decodes an SVG filled with `#339966`
and reads `(0x33, 0x99, 0x66, 0xff)` back out of the surface. Red and blue differ in that fixture,
so a swap the wrong way round would fail rather than pass unnoticed. The conversion runs once per
program per run, so its cost is irrelevant.

---

## R19 — Raster icon decoding (FR-040a)

**Decision**: Use cairo's own PNG loader — `ImageSurface::create_from_png` — by enabling the `png`
feature already declared by `cairo-rs` (`png = ["cairo-sys-rs/png"]`) **[verified]**.

**Rationale**: Zero new crates for the raster half, and the result is already the surface type the
renderer blits. hicolor, the standard fallback set, is 344 PNG / 827 SVG **[verified]**, so the
raster path is genuinely exercised and worth having.

**Alternatives considered**: The **`png` crate** or **`image`** — both would decode into a buffer we
would then have to convert into a cairo surface by hand, adding a dependency to do worse than a
feature flag on a dependency we already have. Formats beyond PNG (XPM, ICO) — not present in any
installed set **[verified]**, and FR-040a already classes them as unresolvable.

---

## R20 — Icon-set lookup (FR-040, FR-057)

**Decision**: Implement the freedesktop icon-theme lookup directly, in `icons/iconset.rs`. Search
`$XDG_DATA_HOME/icons`, `$XDG_DATA_DIRS/icons`, `~/.icons` and `/usr/share/pixmaps`; parse the set's
`index.theme` for its directory list and each directory's `Size`, `Scale`, `Type`, `MinSize`,
`MaxSize` and `Threshold`; follow `Inherits`; fall back to `hicolor`.

**Rationale**: This mirrors R2's reasoning from feature 001 — the format is small, frozen, and fully
specified, and the alternative is a dependency we would still have to understand. The directory
scoring rule is a pure function over parsed metadata, which is exactly the shape this project's
Principle IV wants: unit-testable without touching a filesystem. Measured scale on this machine:
`index.theme` for Papirus-Dark is 18 KB, parsed once **[verified]**.

**Alternatives considered**: The **`freedesktop-icons` crate** — a reasonable fit, and unlike the
Hyprland case there is no release treadmill because the spec is frozen. Rejected on the narrower
ground that it would own the one piece of logic FR-057 makes user-visible (which set is in effect,
and what happens when it is missing), pushing our diagnostics for FR-057's fallback behind someone
else's error type. **`linicon`** — same objection, less maintained.

**Survey of installed sets [verified in implementation — T092]**: the parser was run against every
icon set on the development machine — Adwaita, AdwaitaLegacy, HighContrast, Papirus, Papirus-Dark,
Papirus-Light, breeze, breeze-dark, default, hicolor and locolor, 1610 directories in total. Each
set's parsed directory list, `Size`, `Scale`, `Type`, `MinSize`, `MaxSize`, `Threshold` and
`Inherits` were compared against an independent naive reading of the same `index.theme`, and every
chain was loaded and probed with five icon names. **No set was mishandled** and no chain failed to
terminate at `hicolor`. Two cases worth recording because they exercise the degradation paths
rather than the happy one: `default` lists no directories at all and is purely an `Inherits`
redirect, which the parser resolves to `default → Adwaita → AdwaitaLegacy → hicolor`; and
`locolor` ships no `index.theme`, which `SetIndex::default()` turns into a set holding nothing
findable rather than an error. The assumption is therefore closed in the affirmative, and the
survey is reproducible from the description above rather than kept as a machine-dependent test.

**Symlink note**: 4664 of the 8509 entries in Papirus-Dark's `48x48/apps` are symlinks
**[verified]**, so several classes routinely resolve to one real file. The cache is keyed by
program identity, not by path, so this costs a small amount of duplicate decoding for aliased
programs; deduplicating by resolved path is a possible refinement but no requirement asks for it
(Principle II).

---

## R21 — Program identity and desktop-entry matching (FR-040, SC-012)

**Decision**: Take the window's `class` — already deserialised on `model::Window` — and match it
against the desktop-entry index in this order, stopping at the first hit:

1. `StartupWMClass` equal to the class, case-sensitively;
2. `StartupWMClass` equal case-insensitively;
3. desktop file basename (the entry id, minus `.desktop`) equal case-insensitively;
4. the entry id's last dot-separated component equal case-insensitively (catches reverse-DNS ids
   such as `org.gnome.Nautilus` against a class of `nautilus`);
5. `Name` equal case-insensitively.

Entries marked `NoDisplay=true` are indexed but ranked last, so a real launcher wins over a hidden
one. No match means unresolvable, which FR-041 defines as the placeholder — a normal outcome, not a
reported failure.

**Rationale**: `StartupWMClass` exists precisely to tie a toplevel to its launcher and is the only
authoritative link; the later steps are the conventional fallbacks and are what make the difference
between a good and a poor hit rate. The whole rule is a pure function from (class, index) to an
optional icon name, so every step above is a unit test.

**Alternatives considered**: **Using `initialClass` instead of `class`** — `class` is what
`model.rs` already carries and what the compositor reports for the window's current state; adding a
second identity field would mean a second matching path and a second source of truth (Principle
III). Worth revisiting only if the quickstart survey shows a poor hit rate. **Fuzzy or
prefix matching** — would produce confidently wrong icons, which is worse than the placeholder.

**Index cost**: 143 desktop entries totalling 408 KB on this machine **[verified]**, parsed once at
start-up for four keys (`Icon`, `StartupWMClass`, `Name`, `NoDisplay`). A minimal INI reader covers
it; no dependency.

---

## R22 — Making a visual feature E2E-testable (Principle V)

**Decision**: Two real external interfaces carry the assertions, and neither is a screenshot.

1. **Geometry** is read from the compositor. `hyprctl layers` reports `xywh` for every layer
   surface **[verified]** on a live 0.55 instance, so themed geometry, the size cap, and
   "switching theme never moves the layout" (SC-023) are assertable against the compositor's own
   view of our surface.
2. **Everything else** is asserted through the daemon's stderr, which FR-029 already defines as its
   diagnostic interface. Under an environment gate, the daemon emits one record per painted entry
   naming what it resolved and drew — the icon file chosen, or that the placeholder was used, or
   that content was shed from a miniature rectangle.

**Rationale**: 001's R14 rejected screenshot comparison as brittle across fonts and scaling, and
that reasoning is unchanged. Without an observable signal the feature's headline requirements would
have no E2E coverage at all, failing Principle V. The env gate follows the precedent already in the
tree: `hypr/ipc.rs` carries an env-gated fault-injection hook used only by the 001 rollback tests.

**Alternatives considered**: **Screenshots** — rejected in R14, no less brittle here. **A permanent
query surface** (socket or CLI subcommand to dump overlay state) — a new external interface no
requirement asks for, forbidden by Principle II. **Unit tests only** — leaves FR-035, FR-037,
FR-038, FR-041 and FR-043a with no end-to-end evidence.

**Fixtures**: E2E stages a synthetic icon set and synthetic desktop entries into a temporary
`XDG_DATA_HOME`, so no assertion depends on what the developer has installed. The fixture format is
in [contracts/icon-lookup.md](./contracts/icon-lookup.md).

---

## R23 — Drawing icons inside a single ellipsised line (FR-036, FR-036a)

**Decision**: Keep the row as one pango layout. Build the row text with a `U+FFFC` OBJECT
REPLACEMENT CHARACTER standing in for each icon, attach a `pango::AttrShape` over each of those
characters to reserve the icon's box, then paint the icons with cairo at the positions pango reports
via `Layout::index_to_pos`.

Concretely: `pango::parse_markup` yields the attribute list and plain text for the existing markup;
`AttrShape::new(ink, logical)` with `set_start_index`/`set_end_index` reserves each slot;
`AttrList::insert` merges them; `Layout::set_text` + `set_attributes` replaces today's `set_markup`.
Every one of those APIs is present in pango 0.22.8 **[verified]**.

**Rationale**: This is the only approach that keeps pango's own single-line ellipsisation — the
property `ui/render.rs` relies on to guarantee a row is exactly one line tall (FR-036) and truncates
visibly rather than overflowing (FR-036a). The icons occupy real space in the layout, so the text
ellipsises around them automatically, which is precisely the behaviour clarified into FR-036a.

An icon whose reserved slot falls past the ellipsis is simply not drawn: after layout, compare each
slot's x-offset against the line's actual extent and skip those beyond it.

**Alternatives considered**: **`pango_cairo_context_set_shape_renderer`**, the textbook mechanism —
rejected because it is **commented out and unimplemented** in the `pangocairo` 0.22.8 Rust bindings
**[verified]**; using it means a raw FFI callback with a C function pointer and a raw user-data
pointer, which is unsafe code in a module the constitution wants simple. **Manual segmentation** —
measure and draw each name and icon in sequence — rejected because it means hand-rolling
ellipsisation, the one thing pango is here to do, and it would have to be re-derived for every
font.

---

## R24 — Theme model: palette only (FR-045, FR-049)

**Decision**: A `Theme` is a palette — the eleven colours of FR-045 and nothing else. Fonts
(FR-046) and geometry (FR-047) are configurable style values with a single shared default each, not
per-theme values. Resolution is one chain, written once: explicit override → named theme (colours
only) → default.

**Rationale**: Clarified with the user and recorded in the spec. It gives the smallest data model
that satisfies every requirement, and it makes SC-023's "switching theme never moves the layout" a
structural property rather than something tests must police. Modelling a theme as a full style set
when the shipped themes would differ only in colour is exactly the speculative generality Principle
II forbids.

**Alternatives considered**: **Complete style sets per theme** — would let a future theme ship a
matching font and spacing as one package, but nothing asks for that and it makes every theme carry
values identical to every other theme's. **Palette plus font** — needs a documented reason why
typeface is themeable and spacing is not, and no such reason exists.

---

## R25 — Colour notation (FR-045, FR-059)

**Decision**: One textual form — `#rgb`, `#rrggbb`, `#rrggbbaa` — parsed by a pure function in
`theme.rs`. Anything else is an invalid value under FR-059: reported, that one setting falls back,
everything else still applies.

**Rationale**: The spec's Assumptions already put multiple interchangeable notations out of scope.
Hex with an optional alpha byte is what users of every other Wayland overlay tool already write, it
is about forty lines to parse, and it is trivially unit-testable. Note the existing renderer already
formats colours as hex for pango markup (`fn hex` in `ui/render.rs`), so parsing and formatting stay
symmetric.

**Alternatives considered**: **CSS-style `rgba()` function syntax** — a second grammar for the same
knowledge. **Named colours** — a table to maintain for no requirement. **Separate opacity setting
per colour** — doubles the setting count and lets colour and opacity disagree.

---

## R26 — Geometry ranges and clamping (FR-054, FR-056)

**Decision**: Every geometry value carries a documented `[min, max]` in
[contracts/style-values.md](./contracts/style-values.md). An out-of-range value is clamped to the
nearer bound, and the clamp is reported per FR-059 naming the setting and the value actually used.
Clamping is a pure function; the ranges are `const` in `theme.rs` and are the single source the
contract document is checked against by unit test.

**Rationale**: FR-054 requires bringing values within range rather than rejecting them, so a user who
writes a silly number still gets a working overlay and a message explaining what happened. Making the
ranges data rather than scattered `if`s is what lets one unit test prove SC-023 — that no valid
combination can exceed the monitor, hide the highlighted entry, or make entry size depend on
workspace count.

**Alternatives considered**: **Rejecting out-of-range values** and falling back to the default —
contradicts FR-054's wording and is more surprising: a user who asks for a cap of 5.0 more likely
meant "as large as possible" than "the default". **No ranges, trust the user** — permits a zero row
height, which divides by zero in the viewport maths, and a cap above 1.0, which draws off-monitor.

---

## R27 — Where resolution is triggered (FR-042, FR-043)

**Decision**: `IconStore::ensure(classes)` is called from `main.rs` on the same path that already
rebuilds the world — `Applied::ByRebuilding`, which `state.rs` returns for `Event::WindowOpened`
**[verified]** — and once at start-up. It resolves only classes absent from the cache. The overlay's
paint path can then only read; it never resolves.

**Rationale**: This is the cheapest place that is guaranteed to run before any overlay can open, and
it costs nothing in the common case because the class is already cached. Measured: window-open
already triggers a full IPC round-trip and a JSON parse of every client, so an occasional icon
decode is smaller than the work already happening on that event. It keeps `ui/` free of filesystem
access, preserving the split `CLAUDE.md` describes.

**Alternatives considered**: **Resolving lazily in the paint path** — puts filesystem I/O and SVG
rasterisation inside the 150 ms budget of SC-011, and inside the module that is supposed to be a
painter. **A background thread with repaint** — rejected during clarification; it adds a fourth
wake-up source to the calloop loop and makes entries change under the user mid-selection.

---

## R28 — Cache lifetime and reconnection (FR-043b, FR-026c)

**Decision**: The icon cache lives in memory for the life of the process and is dropped with
everything else when the compositor connection is lost, matching `CLAUDE.md`'s "reconnection is
teardown" and FR-026c's treatment of derived state. No on-disk cache, ever (FR-043b).

**Rationale**: Keeping the cache across a reconnect would save roughly 30 ms and would carve an
exception into an otherwise clean teardown rule — a bad trade. An on-disk cache is forbidden by the
spec and would be caching a few milliseconds of work behind an invalidation problem.

**Alternatives considered**: **Surviving reconnect** — the exception is not worth 30 ms. **An XDG
cache directory of pre-rasterised icons** — forbidden by FR-043b, and it would have to be
invalidated on icon-set changes, theme changes, scale changes and package upgrades.

---

## Resolved unknowns

| Unknown from Technical Context | Resolution |
|---|---|
| Which vector rasteriser, and at what dependency cost | R18 — `resvg`, no text/svgz features; `librsvg` rejected on binding version skew |
| Whether a raster decoder is needed at all | R19 — no: `cairo-rs`'s existing `png` feature |
| Hand-roll or adopt the icon-set lookup | R20 — hand-rolled, as 001 R2 did for IPC |
| How a window maps to a desktop entry | R21 — five-step rule on `class`, `StartupWMClass` first |
| How a visual feature satisfies Principle V | R22 — `hyprctl layers` for geometry, env-gated stderr records for the rest |
| How icons coexist with single-line ellipsisation | R23 — `U+FFFC` + `AttrShape` + `index_to_pos`; shape-renderer FFI rejected |
| What a built-in theme contains | R24 — a palette only |
| Colour syntax | R25 — `#rgb` / `#rrggbb` / `#rrggbbaa` |
| Geometry validity | R26 — documented ranges, clamp and report |
| When resolution runs | R27 — on the existing world-rebuild path and at start-up |
| Cache lifetime | R28 — in memory, dropped on teardown, never on disk |

Two items were carried into implementation as **[assumed]** and have both since been confirmed
(T092): the `tiny_skia` → cairo channel order in R18 is the swap the note describes, proved by a
decode test reading a known colour back out of the surface; and no icon set installed on the
development machine ships an `index.theme` this parser mishandles, across all eleven sets. Neither
would have changed the design had it gone the other way. Nothing in this document is still
assumed.
