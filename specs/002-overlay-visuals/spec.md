# Feature Specification: Overlay Visuals

**Feature Branch**: `002-overlay-visuals`

**Created**: 2026-08-27

**Status**: Draft

**Input**: User descriptions: "Add program icons to overlay entries" and "Additionally implement a
solution for overlay style / theme definition", merged into one feature — both change what the
overlay looks like, both touch the same drawing and the same configuration file, and they interact
(FR-051, FR-052, SC-018).

**Builds on**: `001-workspace-swap-overlay`. Requirement numbering continues that feature's sequence
(FR-035 onward, SC-011 onward) so that FR references in code stay globally unique.

**Supersedes**: Feature 001's assumption that "theming and appearance customisation beyond the two
prescribed presentations is out of scope". That boundary is lifted here, and only for the icons of
FR-035 and the style values enumerated in FR-045 to FR-047.

## Clarifications

### Session 2026-08-27 — icons

- Q: Which overlay presentations should show program icons? → A: Both — the flat list shows an icon
  beside each window name, and the grid miniatures show an icon inside each window rectangle.
- Q: When a window's icon cannot be resolved, what should the entry show? → A: A generic placeholder
  icon, so labels stay aligned and the absence is visible as such.

### Session 2026-08-27 — theming

- Q: How should a theme be defined and selected? → A: A small set of named built-in themes selected
  by name in the main configuration, plus per-key overrides for individual style values.
- Q: What should a theme be able to change? → A: Colours, fonts, and geometry (spacing and sizing).
- Q: When should a theme change take effect? → A: On restart only — no live reload, consistent with
  feature 001.

### Session 2026-08-27 — clarification

- Q: Which icon image formats must the overlay be able to decode? → A: Both raster bitmaps and
  scalable vector icons, so that vector-first icon sets reach the coverage target of SC-012.
- Q: When should a window's icon be resolved and decoded? → A: Ahead of the overlay opening — when a
  window first appears and at start-up for existing windows — so the overlay always draws from an
  already-resolved cache. The timing is not user-configurable.
- Q: In the flat list, icons consume horizontal space on a fixed-width truncated row. What gives? →
  A: Icons take their space and window names truncate sooner. The row stays a single visibly
  truncated line; no icon or name cap is introduced.
- Q: When a miniature's window rectangle is too small for both, what gets dropped first? → A: The
  title, then the icon — the rectangle degrades icon and title, to icon only, to a bare rectangle.
- Q: What does a built-in theme define — colours only, or the full style set? → A: Colours only. A
  built-in theme is a palette; fonts and geometry have single shared defaults reachable only through
  overrides.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Recognise windows by their program icon in the flat list (Priority: P1)

A user holds the switcher combination and sees the flat list of workspaces. Each window listed on a
workspace now carries the icon of the program that owns it, drawn immediately before the window's
name. The user finds the workspace holding their browser by spotting the browser icon rather than
by reading every title on every row.

**Why this priority**: The flat list is the default presentation, so icons here reach every user
without any configuration change, and scanning by icon is the whole point of adding them.

**Independent Test**: With three workspaces holding windows of visibly different programs, open the
overlay in the default flat list presentation and confirm each window name is preceded by that
program's icon, and that the entries are still the same height and the same count as before.

**Acceptance Scenarios**:

1. **Given** a workspace holding one terminal window and one browser window, **When** the overlay
   opens in the flat list presentation, **Then** the terminal's name is preceded by the terminal's
   program icon and the browser's name is preceded by the browser's program icon.
2. **Given** icons are shown, **When** the overlay opens, **Then** each window name that fits on the
   row is preceded by its icon and is still readable — an icon is added alongside a name, never in
   place of it — and the row is still a single line that truncates visibly when the icons and names
   together exceed its width, exactly as it does without icons.
3. **Given** a workspace holding two windows of the same program, **When** the overlay opens,
   **Then** both entries show that same program's icon.
4. **Given** a window whose program cannot be identified, **When** the overlay opens, **Then** a
   generic placeholder icon is drawn in the icon's place and the window's name is unchanged and
   still aligned with the names of neighbouring windows.
5. **Given** any number of workspaces and windows, **When** the overlay opens, **Then** each entry
   row occupies the same height it did before icons existed, and the number of entries visible
   without scrolling is unchanged.

---

### User Story 2 - Match the overlay to my desktop with one setting (Priority: P1)

A user whose desktop is light-themed finds the overlay's dark appearance jarring. They name a
different built-in theme in their configuration file, restart the application, and the whole overlay
— backdrop, text, highlight, active mark, miniatures and window rectangles — is recoloured
coherently. They wrote one line and changed nothing else.

**Why this priority**: One setting, a coherent result, no need to understand or enumerate individual
style values. Without it, matching the overlay to a light desktop means hand-picking a dozen
colours. Independent of US1 — a user who disables icons still wants this.

**Independent Test**: With a configuration naming a non-default built-in theme, open the overlay in
both presentations and confirm every drawn element uses that theme's values and that no element is
left with the default theme's appearance.

**Acceptance Scenarios**:

1. **Given** a configuration naming a built-in theme, **When** the overlay opens, **Then** every
   themed element — backdrop, entry text, highlighted entry, active-workspace mark, miniature
   background, window rectangles and their edges — uses that theme's values.
2. **Given** a configuration naming a built-in theme, **When** the overlay opens in the grid
   presentation, **Then** the miniatures use that same theme; a theme applies to both presentations.
3. **Given** the overlay is shown on all monitors, **When** it opens, **Then** every copy uses the
   same theme.
4. **Given** the default theme and no overrides, **When** the overlay opens, **Then** its colours
   come from that theme and its fonts and geometry from the shared defaults, and together they are
   exactly the appearance the overlay had before this feature.
5. **Given** any built-in theme, **When** the overlay opens, **Then** entry sizes, spacing and the
   overlay's own size are identical to those under every other built-in theme — switching theme
   recolours the overlay and never moves it.
6. **Given** a configuration naming a theme that does not exist, **When** the application starts,
   **Then** it reports the unknown theme name, falls back to the default theme, applies every other
   setting normally, and keeps running.

---

### User Story 3 - Recognise windows by their program icon in the grid miniatures (Priority: P2)

A user who has configured the grid presentation opens the overlay and sees each workspace as a
miniature. Every window rectangle within a miniature now carries the icon of its program alongside
the window's title, so the user can identify a workspace's contents from the shapes and icons
without reading the truncated titles.

**Why this priority**: The grid is the secondary presentation, and the flat list from US1 already
delivers the icon value on its own. It is also the harder half — the rectangles are small and
already carry a truncated title.

**Independent Test**: Set the presentation to grid, open the overlay, and confirm each window
rectangle in each miniature shows its program's icon in addition to its title, with the title still
legible and still truncated rather than overflowing.

**Acceptance Scenarios**:

1. **Given** the grid presentation and a workspace holding windows of two different programs,
   **When** the overlay opens, **Then** each window's rectangle in that workspace's miniature shows
   that program's icon.
2. **Given** a window rectangle large enough for both, **When** the miniature is drawn, **Then** the
   rectangle shows the program icon and the window's title, with the title truncated with a visible
   indication if it does not fit (FR-015b).
3. **Given** a window rectangle too small to show an icon and a title legibly, **When** the
   miniature is drawn, **Then** the rectangle omits content rather than drawing an illegible
   overlap, and the rectangle itself is still drawn in its correct position and proportion.
4. **Given** a window whose program cannot be identified, **When** the miniature is drawn, **Then**
   the generic placeholder icon is drawn in its rectangle.
5. **Given** any miniature, **When** it is drawn, **Then** every window rectangle is still in the
   same relative position and proportion as the real workspace layout (FR-015a) — adding icons does
   not move or resize any rectangle.

---

### User Story 4 - Override individual colours and fonts (Priority: P2)

A user likes a built-in theme but wants the highlight in their own accent colour and prefers a
different font family. They add just those two values as overrides alongside the theme name. Every
other value still comes from the named theme.

**Why this priority**: The escape valve that makes US2 sufficient rather than limiting — but US2
already delivers a coherent, matched overlay on its own, so this can ship second.

**Independent Test**: With a configuration naming a theme plus an override for the highlight colour
and the font family, open the overlay and confirm the highlight and font are the overridden values
while every other element still matches the named theme.

**Acceptance Scenarios**:

1. **Given** a theme name and an override for one colour, **When** the overlay opens, **Then** that
   one element uses the override and every other element uses the named theme's value.
2. **Given** overrides but no theme name, **When** the overlay opens, **Then** the overrides are
   applied on top of the default theme.
3. **Given** an override for the font family, **When** the overlay opens, **Then** all overlay text
   in both presentations is rendered in that family.
4. **Given** an override whose value cannot be understood, **When** the application starts, **Then**
   it reports which setting was invalid and why, that one setting falls back to the value it would
   have had, every other override and setting is still applied, and the application keeps running.
5. **Given** an override naming a font family that is not installed, **When** the overlay opens,
   **Then** text is rendered in a substitute family, remains readable, and no error is raised.

---

### User Story 5 - Resize the overlay for readability (Priority: P3)

A user on a large high-resolution monitor finds the entries small. They raise the text size and
entry height, and widen the overlay's size cap. Entries get bigger and stay bigger no matter how
many workspaces exist, the overlay still refuses to exceed its cap, and it still scrolls to keep the
highlighted entry in view.

**Why this priority**: A real accessibility and large-display need, but the narrowest audience and
the one that most risks breaking the overlay's layout guarantees, so it goes late.

**Independent Test**: With geometry overrides raising the text size, entry height and size cap, open
the overlay with more workspaces than fit and confirm entries are drawn at the requested larger
size, the overlay does not exceed the configured cap, and the highlighted entry is scrolled into
view.

**Acceptance Scenarios**:

1. **Given** an override raising the entry text size and height, **When** the overlay opens, **Then**
   entries are drawn at the requested size and remain that size regardless of how many workspaces
   exist (FR-019 is preserved, not replaced).
2. **Given** geometry overrides and more workspaces than fit within the cap, **When** the overlay
   opens, **Then** the overlay is still capped at the configured fraction of the monitor and still
   scrolls so the highlighted entry is in view; entries are still never scaled down to fit.
3. **Given** overrides to the grid cell size and gap, **When** the overlay opens in the grid
   presentation, **Then** miniatures are drawn at the requested cell size with the requested gap,
   and each window rectangle keeps the relative position and proportion of the real workspace layout
   (FR-015a is preserved).
4. **Given** a geometry value outside its documented valid range, **When** the application starts,
   **Then** the value is brought within range, the adjustment is reported naming the setting and the
   value used, and the overlay is drawn with a usable layout rather than a broken one.
5. **Given** any geometry values, **When** the overlay is shown on a monitor with a non-unity or
   fractional scale, **Then** the geometry is scaled for that monitor exactly as the built-in
   geometry is today, and the overlay is drawn at the monitor's device resolution.

---

### User Story 6 - Turn icons off, or choose which icon set they come from (Priority: P3)

A user on a minimal system with no icon set installed, or a user who simply prefers a text-only
overlay, turns icons off in configuration and gets exactly the overlay they had before. A user whose
desktop icon set is not discoverable from their session names the icon set they want and gets its
icons.

**Why this priority**: An escape hatch, not the value. It matters because without it a user with no
usable icon set is stuck with a column of placeholders — strictly worse than the text-only overlay
they had before — and has no way back.

**Independent Test**: With icons disabled in configuration, open the overlay and confirm it is
identical to the pre-feature overlay; then name a specific icon set in configuration and confirm the
icons drawn come from that set.

**Acceptance Scenarios**:

1. **Given** icons are disabled in configuration, **When** the overlay opens in either presentation,
   **Then** no icons and no placeholders are drawn, and the layout is exactly the text-only layout.
2. **Given** no configuration file is present, **When** the overlay opens, **Then** icons are shown
   — icons are on by default.
3. **Given** a configuration naming an installed icon set, **When** the overlay opens, **Then** the
   icons drawn are that set's icons.
4. **Given** a configuration naming an icon set that is not installed, **When** the application
   starts, **Then** it reports the invalid value, falls back to its default icon source, and keeps
   running (FR-024).

---

### Edge Cases

**Icons**

- **No icon set at all on the system**: every window resolves to the placeholder. The overlay is
  still fully usable and no error is raised — the user can turn icons off if they prefer.
- **Window reports an empty or unrecognised program identity**: treated as unresolvable — the
  placeholder is drawn, and this is a normal outcome, not a reported failure.
- **Icon file exists but is unreadable or malformed**: the placeholder is drawn for that program; a
  diagnostic is written to standard error but no desktop notification is raised (FR-031's
  self-recovering class of condition), and the overlay still opens.
- **Icon available only at a size that does not match the drawing size**: it is scaled to fit its
  slot without distorting its aspect ratio.
- **Many workspaces each holding many windows**: the overlay still appears within its opening budget
  (SC-011). Resolving and decoding icons must not be what makes the overlay late.
- **The same program owns windows on many workspaces**: its icon is resolved once, not once per
  window.
- **A program is installed or uninstalled while the daemon is running**: the overlay reflects
  whatever it can resolve; a stale unresolvable icon degrades to the placeholder rather than to a
  broken entry.
- **Grid miniature of a workspace with a single fullscreen window**: the icon is drawn inside that
  one large rectangle at its normal size, not scaled up to fill it.

**Theming**

- **Theme name valid but an override is not**: the theme is applied in full and only the bad
  override falls back — one invalid value never discards the rest of the user's style.
- **Overrides with no theme named**: applied on top of the default theme.
- **A colour given with full transparency, or a highlight identical to the backdrop**: rendered as
  asked. The application validates that values are parseable and within layout-safe ranges; it does
  not police contrast or aesthetics.
- **Geometry value of zero or negative** (entry height, cell size, font size): brought to the
  documented minimum and reported, rather than collapsing the overlay or dividing by zero.
- **Size cap set above the whole monitor**: brought to the documented maximum so the overlay can
  never exceed the monitor it is drawn on.
- **Font family not installed on the system**: the platform substitutes a family; text stays readable
  and no error is raised.
- **Font size raised beyond what one entry row can hold**: the row height follows the text size so
  the text is never clipped by its own row.
- **User edits any visual setting while the application is running**: nothing changes until the
  application is restarted, consistent with feature 001's configuration handling.

**Where the two halves meet**

- **A theme value that would make the miniature's window titles illegible**: the existing minimum
  legible text rule still applies — a title too small to read is omitted rather than drawn illegibly
  (FR-015b, FR-038), and the same rule governs whether the icon is drawn.
- **Program icons under a theme**: a program's own icon is drawn as the program supplies it and is
  never recoloured; only the generic placeholder follows the theme (FR-051).
- **Geometry overrides and icon size**: the icon slot follows the themed text height, so raising the
  text size raises the icons with it and the row stays proportioned.
- **Icons enabled with a theme selected, and no configuration file**: the default theme reproduces
  the prior appearance and icons are shown, so the only visible difference from the pre-feature
  overlay is the icons themselves (SC-018, SC-019).

## Requirements *(mandatory)*

### Functional Requirements

**Icon display**

- **FR-035**: Every window shown in the overlay MUST be accompanied by the icon of the program that
  owns it, in both the flat list and the grid presentations.
- **FR-036**: In the flat list, a window's icon MUST be drawn immediately before that window's name.
  Icons MUST NOT change the height of an entry row, the total number of entries, or the number of
  entries visible without scrolling — the fixed readable entry size of FR-019 is unchanged.
- **FR-036a**: Icons MUST occupy horizontal space on the entry row, so a row holding many windows
  truncates its window names sooner than the same row without icons. The row MUST remain a single
  line that truncates with a visible indication rather than wrapping or overflowing. The application
  MUST NOT cap the number of icons on a row, and MUST NOT drop a window's name in favour of its icon.
- **FR-037**: In the grid presentation, a window's icon MUST be drawn inside that window's rectangle
  in the miniature, alongside its title. Icons MUST NOT change the position, size, or proportion of
  any rectangle in a miniature (FR-015a).
- **FR-038**: When a window rectangle in a miniature is too small to show its icon and its title
  legibly, the rendering MUST omit content rather than draw an illegible overlap, in this order: the
  title is dropped first, then the icon. A rectangle therefore shows its icon and title, or its icon
  alone, or neither, as it gets smaller — and MUST still be drawn in every case.
- **FR-039**: An icon MUST be scaled to fit the space allotted to it without distorting its aspect
  ratio, and MUST be drawn at the monitor's device resolution so that it is not blurred on scaled
  monitors.

**Icon resolution**

- **FR-040**: The application MUST determine which icon belongs to a window from the identity the
  compositor reports for that window, resolved against the desktop's installed application entries
  and icon set by the platform's standard lookup rules.
- **FR-040a**: The application MUST decode both raster bitmap icons and scalable vector icons, so
  that programs whose icon set supplies only a vector icon still show their own icon rather than the
  placeholder. An icon in any other format MUST be treated as unresolvable (FR-041).
- **FR-041**: When no icon can be resolved for a window, the application MUST draw a generic
  placeholder icon in the same slot, at the same size, so that neighbouring window names stay
  aligned. An unresolvable icon is a normal outcome and MUST NOT raise a desktop notification.
- **FR-042**: Icon resolution MUST be performed at most once per distinct program per run and the
  result reused for every window of that program, including across repeated overlay openings.
- **FR-043**: Icon resolution and decoding MUST happen ahead of the overlay opening — when a window
  first appears, and at start-up for windows that already exist — so that opening the overlay only
  reads already-resolved icons. Opening the overlay MUST NOT itself resolve or decode an icon, and
  MUST NOT be delayed beyond its opening budget (SC-001) by icon work.
- **FR-043a**: A window whose icon has not yet been resolved when the overlay opens MUST be drawn
  with the placeholder rather than the overlay being held back. The overlay MUST NOT repaint an
  entry to swap a placeholder for a real icon while it is open; the resolved icon appears the next
  time the overlay opens.
- **FR-043b**: Resolved icons MUST be held in memory only. The application MUST NOT write an icon
  cache to disk. The cache MUST be discarded when the application exits and when the compositor
  connection is lost, consistent with FR-026c's treatment of derived state.
- **FR-044**: A malformed or unreadable icon MUST be reported once on standard error and MUST be
  treated as unresolvable from then on. It MUST NOT abort the overlay, crash the application, or
  repeat the diagnostic on every overlay opening.

**Configurable style values**

- **FR-045**: The following colours MUST be configurable: the overlay backdrop, the highlighted entry,
  the active-workspace mark, primary entry text in its normal and highlighted states, secondary
  entry text in its normal and highlighted states, the miniature background, the tiled and floating
  window rectangle fills, and the window rectangle edge. Every colour MUST support an opacity
  component.
- **FR-046**: The text font family and the text size MUST be configurable, and MUST apply to all overlay
  text in both presentations.
- **FR-047**: The following geometry MUST be configurable: the entry text height and row padding, the
  overlay's outer padding, the overlay's width and height size cap as a fraction of the monitor, the
  grid cell width and height, the grid gap, the corner radius, and the active-mark width.
- **FR-048**: A theme MUST apply identically to both presentations and to every monitor the overlay
  is shown on.

**Theme selection**

- **FR-049**: The application MUST provide a documented set of built-in themes selectable by name in
  configuration, including at least a dark theme and a light theme. A built-in theme MUST define
  only the colours of FR-045; the font values of FR-046 and the geometry values of FR-047 MUST have
  a single shared default each, independent of the selected theme and reachable only through an
  override. Selecting a different built-in theme MUST therefore never change the overlay's layout.
- **FR-049a**: The default theme's colours, together with the shared font and geometry defaults,
  MUST reproduce the overlay's appearance prior to this feature.
- **FR-050**: The application MUST allow any individual style value to be overridden in configuration
  independently of the named theme. The resolved value of each style value MUST follow one
  documented precedence chain: an explicit override, otherwise the named theme's value, otherwise
  the default theme's value.

**Where icons and theming meet**

- **FR-051**: A program's own icon (FR-035) MUST be drawn as supplied and MUST NOT be recoloured by a
  theme. The generic placeholder icon (FR-041) MUST follow the theme's primary text colour.
- **FR-052**: The space allotted to an icon MUST follow the themed text height (FR-046), so that
  raising the text size raises the icons with it and the entry row stays proportioned.

**Layout invariants**

- **FR-053**: Neither icons nor theming MUST weaken the overlay's existing layout guarantees. With
  any valid configuration: entries are still rendered at a fixed size that does not change with the
  number of workspaces, the overlay is still capped at its configured fraction of the monitor, it
  still scrolls to keep the highlighted entry in view, and entries are still never scaled down to fit
  (FR-019). Miniature window rectangles still keep the relative position and proportion of the real
  workspace layout (FR-015a).
- **FR-054**: Every configurable geometry value MUST have a documented valid range that guarantees a
  usable overlay. A value outside its range MUST be brought within range rather than rejected or
  applied as given.
- **FR-055**: Themed geometry MUST be interpreted in the same units as the built-in geometry and
  scaled per monitor the same way, so that a theme behaves identically across monitors of different
  scales.

**Configuration and diagnostics**

- **FR-056**: Whether icons are shown MUST be selectable through configuration, defaulting to shown.
  When icons are disabled, the overlay MUST render with no icons, no placeholders, and no space
  reserved for either.
- **FR-057**: The icon set the application draws from MUST be selectable through configuration,
  defaulting to the icon set configured for the user's desktop, and falling back to the platform's
  standard default set when none is discoverable. The icon set is independent of the overlay theme:
  neither setting changes the other.
- **FR-058**: An unknown theme name MUST be reported per FR-024, with the theme falling back to the
  default while every other setting is still applied, and the application MUST keep running.
- **FR-059**: Any visual setting that is unparseable or out of range MUST be reported per FR-024,
  naming the setting, what was wrong with it, and the value used instead. Only that setting MUST fall
  back; every other visual setting MUST still be applied. One bad value MUST NOT discard the rest.
- **FR-060**: Every visual setting MUST be read at start-up together with the rest of the
  configuration. Changes MUST take effect on the next start of the application; the application MUST
  NOT reload them while running.
- **FR-061**: The application MUST document every visual setting, its accepted form, its valid range
  where one applies, and its default, so that a user can write a complete appearance without reading
  the source.

### Key Entities

- **Program**: The application that owns a window, identified by the identity the compositor reports
  for that window. Many windows may belong to one program. This is the unit that an icon is resolved
  for once and reused for every window it owns.
- **Icon**: The visual mark drawn for a program — either the program's own icon resolved from the
  desktop's icon set, or the generic placeholder when none could be resolved. Has a source size and
  is drawn scaled to whatever slot the presentation gives it.
- **Theme**: A named palette — the complete set of colours the overlay draws with, and nothing else.
  Fonts and geometry are not part of a theme; they have one shared default each. Built-in themes are
  supplied by the application, and exactly one is the default.
- **Style Value**: One individually addressable appearance setting — a colour, a font property, or a
  geometry measurement. Has a documented accepted form, a default, and, for geometry, a valid range.
- **Style Override**: A user-supplied value for one style value, taking precedence over the named
  theme. Overrides are independent of one another: an invalid override affects only itself.
- **Configuration** *(extends the entity of feature 001)*: gains whether icons are shown (default:
  shown), which icon set to draw from (default: the desktop's configured set), a theme name (default:
  the default built-in theme), and a set of style overrides (default: none).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-011**: The overlay is still visible within 150 ms of pressing the switcher combination
  (SC-001) with icons enabled, under any built-in theme and any valid set of overrides, with at least
  20 workspaces holding at least 60 windows in total across at least 10 distinct programs.
- **SC-012**: For a workspace holding windows of common desktop programs installed on the system, at
  least 90% of those windows show that program's own icon rather than the placeholder.
- **SC-013**: Every window listed in either presentation shows exactly one icon — its program's icon
  or the placeholder — with no window ever drawn iconless while icons are enabled.
- **SC-014**: A user asked to find the workspace holding a named program, out of at least ten
  workspaces, does so faster with icons enabled than with icons disabled.
- **SC-015**: With icons enabled, the flat list shows the same number of entries in the same positions
  and at the same size as with icons disabled — icons cost no entry visibility.
- **SC-016**: With no icon set installed on the system, the overlay still opens, every window shows
  the placeholder, every window name is readable, and no error is raised.
- **SC-017**: Opening the overlay 100 times in a session resolves each distinct program's icon no more
  than once and shows no growth in the application's memory footprint attributable to icons.
- **SC-018**: With no configuration file present, the overlay uses the default theme and shows icons,
  and every colour, font and geometry value matches the pre-feature overlay exactly — the icons are
  the only visible difference.
- **SC-019**: With icons disabled and the default theme selected, the overlay is pixel-identical to
  the overlay produced before this feature, in both presentations.
- **SC-020**: A user can switch the overlay between a dark and a light appearance by changing a single
  line of configuration, with no other edits.
- **SC-021**: Selecting any built-in theme leaves no element drawn in another theme's colours — 100%
  of themed elements in both presentations follow the selected theme.
- **SC-022**: A configuration containing one invalid visual setting still applies 100% of the
  remaining valid settings, and the application starts and operates normally.
- **SC-023**: No combination of valid visual settings can produce an overlay that exceeds its monitor,
  hides the highlighted entry, or renders entries at a size that varies with the number of
  workspaces.
- **SC-024**: A user can write a complete custom appearance using only the documented list of visual
  settings, without consulting the source code.
- **SC-025**: Every visual setting listed in the documentation is observably reflected in the rendered
  overlay when changed — no documented setting is inert.

## Assumptions

**Icons**

- The desktop's installed application entries and icon set follow the platform's standard freedesktop
  layout and lookup rules; systems that do not are treated as having no icons available, which
  degrades to the placeholder (SC-016).
- The identity the compositor reports for a window is sufficient to find its application entry for the
  large majority of desktop programs. Programs that report an identity matching no installed entry are
  expected and get the placeholder.
- Icons accompany window names; they never replace them. Nothing readable in the overlay today is
  removed by this feature.
- The workspace name in a flat-list entry gets no icon of its own — a workspace is not a program.
  Icons attach to windows only.
- The placeholder icon ships with the application, so it is always available even on a system with no
  icon set at all.
- Per-window custom icons, user-supplied icon overrides for specific programs, and animated icons are
  out of scope.
- When icons are resolved is an internal matter, not a user-facing setting: only whether icons are
  shown (FR-056) and which icon set they come from (FR-057) are configurable. Measured cost on a
  representative system is a few milliseconds per distinct program, one-time, against an event that
  already triggers a full compositor rebuild — so a second resolution strategy would add a code path
  and tests without solving any user-visible problem.

**Theming**

- Built-in themes are compiled into the application, so a valid theme is always available even with no
  configuration present.
- A built-in theme carries colours only (FR-049). Shipping a theme that also rebinds fonts or
  spacing is out of scope; a user wanting that combines a theme name with overrides.
- The application validates that style values are parseable and within layout-safe ranges. It does not
  evaluate contrast, readability, or aesthetics — a user who configures an unreadable colour
  combination gets exactly what they asked for.
- Colours are given in a single documented textual form supporting an opacity component; supporting
  several interchangeable colour notations is out of scope.
- Geometry values are expressed in the same logical units the overlay already uses and are scaled per
  monitor by the existing rule, so a theme needs no per-monitor variants.
- Per-monitor themes, per-presentation themes, and per-workspace styling are out of scope — one theme
  applies everywhere (FR-048).
- Background blur, drop shadows, gradients, animations, and per-element borders beyond those already
  drawn are out of scope — this feature makes the existing drawing configurable, it does not add new
  visual elements.
- A separate theme file format and a theme search path are out of scope; themes are named and
  overridden in the application's own configuration file.

**Both**

- Every visual setting lives in the same single user-editable configuration file as the rest of the
  application's settings, read once at start-up.
- Live reload, theme hot-swapping, and following the desktop's light/dark preference automatically are
  out of scope; the appearance is chosen explicitly and applied at start-up (FR-060).
- Navigation, selection, swapping, screen capture, mouse interaction, and every other behaviour are
  unchanged. This feature changes only what the overlay looks like.
