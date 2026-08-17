# Feature Specification: Workspace Swap Overlay

**Feature Branch**: `001-workspace-swap-overlay`

**Created**: 2026-07-27

**Status**: Draft

**Input**: User description: "Build an application that will integrate with hyprland compositor, that will allow to more easily manage workspaces. Its functionalities should be: Swap two workspaces with a hot key (same monitor → selected workspace is activated; separate monitors → they are swapped: currently active workspace goes to the monitor of the selected workspace, selected workspace is moved to the active monitor and activated). When swapping is started with a hot key, all workspaces list is shown on the active monitor (or all monitors, via config). Hotkey is configurable. Hotkey is like Alt-Tab. When swapping is started, workspaces list is shown until e.g. Alt is released. There are two kinds of a workspace list: a flat list with a workspace name and names of windows as an entry name; a grid of miniatures with names of workspaces underneath. A hot key that will spawn a new empty workspace on a current monitor."

## Clarifications

### Session 2026-07-27

- Q: In what order should workspaces appear in the overlay, and where does the highlight start? → A: User-configurable, with most-recently-used (MRU) as the default.
- Q: What should the application do if the compositor connection drops while it is running? → A: Reconnect automatically with backoff, rebuilding state and re-registering its named shortcuts.
- Q: Where do user-facing diagnostics go, given there is no attached terminal? → A: All diagnostics to stderr, plus a desktop notification for problems the user must act on.
- Q: What happens when one half of a cross-monitor swap fails? → A: Roll back the half that succeeded, restore the prior state, and report the failure.
- Q: Must the application claim its own hotkeys, given Wayland offers clients no global-hotkey mechanism? → A: No — FR-022 is released. Key combinations are bound in the compositor's own configuration and routed to the application as named shortcuts. Commit-on-release remains mandatory: the application MUST observe the modifier release itself.
- Q: While the overlay is open, who handles the selection, backwards and cancel keys? → A: The overlay takes exclusive keyboard focus and handles them itself with fixed defaults; only two actions are bound in the compositor (open switcher, new workspace).
- Q: What exactly does the new-workspace shortcut create, and what does a repeat press do? → A: It switches to the lowest unused workspace number bound to the focused monitor; if the active workspace is already empty, the shortcut is a no-op.
- Q: How does the overlay absorb more workspaces than fit on screen? → A: Entries keep a fixed readable size; the overlay is capped at a fraction of the monitor and scrolls to keep the highlighted entry in view. Entries are never scaled down to fit.
- Q: What are the documented defaults for presentation and overlay placement? → A: Flat list presentation, shown on the active monitor only (entry order already defaults to MRU).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Switch to any workspace with a hold-and-release hotkey (Priority: P1)

A user working across several workspaces presses and holds a modifier key combination. An overlay
appears listing every workspace along with the windows it contains. While still holding the
modifier, the user taps the selection key repeatedly to move the highlight through the list. When
the user releases the modifier, the overlay disappears and the highlighted workspace becomes the
active workspace on the monitor the user is working on.

**Why this priority**: This is the core value of the product — reaching any workspace by name and
content instead of memorising numeric bindings. Without it there is no product.

**Independent Test**: With three workspaces containing distinct windows, hold the switcher
combination, tap
through to the third entry, release, and confirm the third workspace is now active and focused.

**Acceptance Scenarios**:

1. **Given** the default MRU order and workspaces used in the order 3 (current), 7, 1, **When** the
   user taps the selection key once and releases immediately, **Then** workspace 7 becomes active.
2. **Given** the default MRU order, **When** the overlay opens, **Then** the current workspace is
   the first entry and the highlight is on the second entry.
3. **Given** compositor order is configured and workspace 3 is active, **When** the overlay opens,
   **Then** entries appear in the compositor's stable order and the highlight is on workspace 3.
4. **Given** the overlay is open, **When** the user holds the modifier and taps the selection key
   twice, then releases, **Then** the overlay closes and the third entry's workspace is active.
5. **Given** the overlay is open, **When** the user cancels rather than committing, **Then** the
   MRU history is unchanged and the next overlay opens with the same order.
6. **Given** the overlay is open, **When** the user presses the cancel key while still holding the
   modifier, **Then** the overlay closes immediately and no workspace change occurs.
7. **Given** the user selects the workspace that is already active, **When** the modifier is
   released, **Then** no workspace movement occurs and focus is unchanged.
8. **Given** the overlay is open, **When** the user taps the selection key past the last entry,
   **Then** the highlight wraps around to the first entry.
9. **Given** the user switched workspaces using the compositor's own keybinding rather than this
   application, **When** the overlay is next opened in MRU order, **Then** that switch is reflected
   in the order.

---

### User Story 2 - Swap workspaces between monitors (Priority: P1)

A user with two or more monitors is working on a workspace on the left monitor and wants the
content of a workspace currently on the right monitor to be in front of them. They hold the
hotkey, highlight that workspace, and release. The two workspaces trade places: the workspace they
were on moves to the right monitor, and the selected workspace moves to the left monitor and
becomes active there.

**Why this priority**: This is the behaviour that gives the product its name and distinguishes it
from a plain workspace switcher. It is as essential as US1 for multi-monitor users.

**Independent Test**: With workspace A active on monitor 1 and workspace B active on monitor 2,
select B from monitor 1 and confirm that after release B is on monitor 1 and active, and A is on
monitor 2.

**Acceptance Scenarios**:

1. **Given** workspace A is active and focused on monitor 1 and workspace B is active on monitor 2,
   **When** the user opens the overlay from monitor 1, highlights B and releases, **Then**
   workspace B is on monitor 1, active and focused, and workspace A is on monitor 2 and active
   there.
2. **Given** workspace C is bound to monitor 2 but is not the workspace currently displayed on
   monitor 2, **When** the user selects C from monitor 1 and releases, **Then** C moves to
   monitor 1 and becomes active, and the previously active workspace of monitor 1 moves to
   monitor 2.
3. **Given** the selected workspace is on the same monitor as the active workspace, **When** the
   modifier is released, **Then** the selected workspace is simply activated and no workspace is
   moved between monitors.
4. **Given** any successful swap, **When** the swap completes, **Then** every window that was open
   before the swap is still open, on the same workspace it was on, with no window closed or
   orphaned.
5. **Given** only one monitor is connected, **When** any workspace is selected, **Then** the
   selection behaves as a plain activation (US1) and no error is shown.
6. **Given** a cross-monitor swap in which the first move succeeds and the second fails, **When**
   the failure is detected, **Then** the first move is undone, both monitors show the workspaces
   they showed before the hotkey was pressed, focus is where it was, and the failure is reported.

---

### User Story 3 - Preview workspaces as a grid of miniatures (Priority: P2)

A user who recognises their workspaces visually rather than by window title configures the overlay
to show a grid of miniature previews, each labelled with its workspace name underneath, and
navigates the grid with the same hold-and-release interaction.

**Why this priority**: A significant usability improvement, but the flat list from US1 already
delivers a complete working switcher, so this can ship second.

**Independent Test**: Set the presentation to grid, open the overlay, and confirm a grid of
labelled miniatures appears and that selection and release behave exactly as in the flat list.

**Acceptance Scenarios**:

1. **Given** the grid presentation is configured, **When** the overlay opens, **Then** each
   workspace is shown as a miniature preview with its workspace name displayed beneath it.
2. **Given** a workspace holds two windows side by side and a third stacked below the second,
   **When** its miniature is shown, **Then** three labelled rectangles appear in those same
   relative positions and proportions.
3. **Given** a workspace that has not been visible on any monitor during this session, **When** its
   miniature is shown, **Then** it is rendered with the same accuracy as a currently visible one.
4. **Given** the grid presentation is configured, **When** the user navigates and releases the
   modifier, **Then** activation and swap behaviour is identical to the flat list presentation.
5. **Given** a workspace contains no windows, **When** the grid is shown, **Then** that workspace
   appears as a clearly-marked empty miniature rather than being omitted or shown blank without
   explanation.
6. **Given** the flat list presentation is configured, **When** the overlay opens, **Then** each
   entry shows the workspace name followed by the names of the windows it contains.

---

### User Story 4 - Create a new empty workspace on the current monitor (Priority: P2)

A user who wants a clean slate presses a dedicated key combination and is immediately placed on a
new, empty workspace bound to the monitor they are currently working on, without the overlay
appearing.

**Why this priority**: Frequently needed and independent of the switcher, but the product is still
useful without it.

**Independent Test**: Press the new-workspace combination and confirm the lowest unused workspace
number is active on the current monitor and appears in the overlay on the next invocation.

**Acceptance Scenarios**:

1. **Given** the user is working on monitor 2 and workspaces 1, 2 and 4 are in use, **When** the
   new-workspace shortcut is triggered, **Then** workspace 3 becomes active and focused on
   monitor 2.
2. **Given** a new empty workspace was just created and is still empty, **When** the user triggers
   the new-workspace shortcut again, **Then** nothing changes — the same workspace stays active,
   focus is unchanged, and no additional workspace is created.
3. **Given** a new workspace was created, **When** the overlay is opened, **Then** the new
   workspace appears in the list.

---

### User Story 5 - Bind shortcuts and configure overlay presentation (Priority: P3)

A user adds the documented bind lines to their compositor configuration to choose which key
combination opens the switcher and which one creates a new workspace, then edits the application's
own configuration file to choose whether the overlay appears only on the active monitor or on all
monitors, which presentation (flat list or grid) is used, and in what order workspaces are listed.

**Why this priority**: Sensible defaults make the product usable out of the box; configuration
broadens its audience but is not required for first value.

**Independent Test**: Bind the shortcuts in the compositor configuration, change each setting in the
application's configuration file, restart the application, and confirm the new bindings, placement
and presentation take effect.

**Acceptance Scenarios**:

1. **Given** the documented bind line for the switcher shortcut is present in the compositor
   configuration, **When** the user presses that combination, **Then** the overlay opens, and
   **When** the user releases the modifier, **Then** the highlighted selection is committed.
2. **Given** the overlay placement is set to all monitors, **When** the overlay opens, **Then** it
   is displayed on every connected monitor simultaneously and every copy shows the same highlighted
   entry.
3. **Given** the overlay placement is set to the active monitor, **When** the overlay opens,
   **Then** it appears only on the monitor holding the focused workspace.
4. **Given** no configuration file exists, **When** the application starts, **Then** it runs with
   documented defaults and does not fail.
5. **Given** a configuration file containing an invalid value, **When** the application starts,
   **Then** it names the offending setting on standard error and in a desktop notification, falls
   back to the default for that setting, and continues running.
6. **Given** the compositor configuration binds only the new-workspace shortcut and leaves the
   switcher shortcut unbound, **When** the application starts, **Then** it runs normally, the
   new-workspace shortcut works, and the unbound switcher shortcut causes no error.
7. **Given** the entry order is set to compositor order or grouped by monitor, **When** the overlay
   opens, **Then** entries appear in that order and the highlight opens on the active workspace
   rather than on the second entry.

---

### Edge Cases

- **Only one workspace exists**: the overlay opens showing that single entry and releasing changes
  nothing.
- **Compositor unavailable at start-up**: the compositor is not running or refuses the connection —
  the application reports a clear error and exits rather than hanging or silently doing nothing.
- **Compositor restarts while the application is running**: the connection drops, any open overlay
  closes without committing, and the application reconnects and resumes working without the user
  restarting it.
- **Connection lost with an overlay open and the modifier still held**: the overlay closes without
  committing, and releasing the modifier afterwards has no effect.
- **Monitor disconnected mid-session**: a monitor holding listed workspaces is unplugged while the
  overlay is open — the overlay refreshes to the surviving monitors, and a selection targeting a
  vanished monitor resolves to plain activation on the current monitor.
- **Compositor state changes while the overlay is open**: a window opens or closes, or a workspace
  is created by another tool — the overlay content stays consistent, and a selection whose
  workspace disappeared is treated as cancelled.
- **More workspaces than fit on screen**: entries keep their normal size, the overlay scrolls, and
  every entry remains reachable through navigation with the highlighted entry always scrolled into
  view.
- **Modifier released before the overlay has finished appearing**: the fast tap case — the
  selection still resolves correctly and the overlay does not linger on screen.
- **Switcher invoked while an overlay is already open**: no second overlay is opened and no stale
  overlay is left on screen.
- **Fullscreen window active**: the overlay is visible above a fullscreen window on the target
  monitor.
- **Exclusive keyboard focus refused**: something else already holds exclusive keyboard focus (a
  lock screen, for example) — the overlay does not open, the condition is reported on standard
  error, and no workspace change occurs.
- **Degenerate self-swap**: the selected workspace is the active workspace of the focused monitor —
  treated as plain activation.
- **Special or scratchpad workspaces**: excluded from the list and never moved between monitors.
- **Workspace emptied by a swap**: handled by the compositor's normal workspace lifecycle and never
  leaves a phantom entry in the list.

## Requirements *(mandatory)*

### Functional Requirements

**Switcher interaction**

- **FR-001**: The application MUST open a workspace overlay when the switcher shortcut is
  triggered.
- **FR-002**: The overlay MUST remain visible for as long as the modifier key of the combination
  that opened it is held down, and MUST close when that modifier is released.
- **FR-002a**: While the overlay is open it MUST hold exclusive keyboard focus, so that the
  application receives every key press and the current modifier state directly rather than through
  the compositor's shortcut bindings. On close, keyboard focus MUST return to the window that held
  it before the overlay opened.
- **FR-003**: While the overlay is open, each tap of the selection key MUST advance the highlight
  to the next workspace entry, wrapping from the last entry to the first.
- **FR-004**: While the overlay is open, the user MUST be able to move the highlight backwards
  through the entries.
- **FR-004a**: The selection key, the backwards-navigation key, and the cancel key are handled by
  the application itself while it holds keyboard focus. They MUST be fixed, documented defaults and
  MUST NOT require or accept any compositor binding.
- **FR-005**: The application MUST commit the highlighted selection when the modifier is released.
- **FR-006**: The user MUST be able to cancel an open overlay without changing workspaces.
- **FR-007**: The overlay MUST list every ordinary workspace known to the compositor, including
  workspaces that currently contain no windows; special and scratchpad workspaces MUST be excluded.
- **FR-008**: The overlay MUST indicate which entry is currently highlighted and which workspace is
  currently active.
- **FR-008a**: The order of entries MUST be user-configurable, with most-recently-used (MRU) order
  as the default. The supported orders are: MRU (most recently active first), compositor order (the
  stable order the compositor reports), and grouped by monitor (stable order within each monitor's
  group).
- **FR-008b**: In MRU order, the highlight MUST open on the second entry, so that a single tap and
  release returns the user to the workspace they were previously on. In every other order, the
  highlight MUST open on the currently active workspace.
- **FR-008c**: The application MUST maintain a most-recently-used history of workspace activations
  for the lifetime of the session, updated whenever the active workspace changes — including
  changes made by other tools or by the compositor's own keybindings, not only those made through
  this application.
- **FR-008d**: A workspace that has never been active during the session MUST still appear in MRU
  order, after all workspaces that have been active.

**Selection outcome**

- **FR-009**: When the selected workspace is bound to the same monitor as the active workspace, the
  application MUST activate the selected workspace on that monitor and move no workspace between
  monitors.
- **FR-010**: When the selected workspace is bound to a different monitor than the active
  workspace, the application MUST move the active workspace to the selected workspace's monitor,
  move the selected workspace to the active monitor, activate the selected workspace, and leave
  keyboard focus on it.
- **FR-011**: When the selected workspace is already the active workspace, the application MUST
  make no change.
- **FR-012**: A swap MUST NOT close, minimise, or lose any window; every window MUST remain on the
  workspace it was on before the swap.
- **FR-013**: After a swap, both affected monitors MUST display an active workspace — neither
  monitor may be left showing nothing.
- **FR-013a**: A cross-monitor swap MUST be all-or-nothing. If any part of it fails, the
  application MUST undo the parts that succeeded and restore the monitor bindings, active
  workspaces, and keyboard focus that were in place before the selection was committed.
- **FR-013b**: A failed and rolled-back swap MUST be reported on standard error and as a desktop
  notification, since the user asked for a change that did not happen.
- **FR-013c**: If the rollback itself cannot complete, the application MUST report the resulting
  state on standard error and as a desktop notification rather than leaving the user with no
  indication that their layout changed unexpectedly.

**Presentation**

- **FR-014**: The application MUST support a flat list presentation in which each entry shows the
  workspace name followed by the names of the windows on that workspace.
- **FR-015**: The application MUST support a grid presentation in which each workspace is shown as
  a miniature preview with the workspace name displayed beneath it.
- **FR-015a**: A miniature MUST be a schematic rendering of the workspace's layout: each window
  drawn as a rectangle in the same relative position and proportion it occupies on that workspace,
  labelled with that window's title. Miniatures MUST NOT depend on screen capture, and MUST be
  equally accurate for workspaces that are not currently visible on any monitor.
- **FR-015b**: A window title too long for its rectangle MUST be truncated with a visible
  indication rather than overflowing or being omitted.
- **FR-016**: The presentation MUST be selectable through configuration, and both presentations
  MUST support identical navigation and selection behaviour.
- **FR-017**: The overlay MUST be displayed either only on the monitor holding the active workspace
  or on all connected monitors, selectable through configuration; when shown on all monitors, every
  copy MUST reflect the same highlighted entry.
- **FR-018**: The overlay MUST render above all other windows on the monitors where it is shown,
  including fullscreen windows.
- **FR-019**: Entries MUST be rendered at a fixed, readable size that does not change with the
  number of workspaces. The overlay MUST be capped at a documented fraction of the monitor it is
  shown on, and when the entries exceed that space the overlay MUST scroll so that the highlighted
  entry is always in view. Entries MUST NOT be scaled down to make them all fit at once.

**New workspace**

- **FR-020**: When the new-workspace shortcut is triggered, the application MUST switch to the
  lowest workspace number not currently in use, bind it to the focused monitor, and make it active
  and focused.
- **FR-021**: If the currently active workspace is already empty, the new-workspace shortcut MUST
  be a no-op — no workspace is created, none is switched to, and focus is unchanged — so that
  repeat presses cannot accumulate unused empty workspaces.

**Configuration**

- **FR-022**: Exactly two actions MUST be exposed as named shortcuts that the user binds to key
  combinations in the compositor's own configuration: opening the switcher, and creating a new
  workspace. The application MUST NOT attempt to claim or grab key combinations itself. Backwards
  navigation is not a bound shortcut; it is an in-overlay key handled under FR-004a.
- **FR-022a**: Commit-on-release is mandatory. Once the switcher shortcut has opened the overlay,
  the application MUST itself observe the state of the modifier key and commit the highlighted
  selection at the moment that modifier is released (FR-002, FR-005). A shortcut delivery
  mechanism that reports only that a combination was pressed, without allowing the application to
  observe the modifier release, does not satisfy this requirement.
- **FR-022b**: The application MUST document the exact bind lines required for its shortcuts, and
  MUST start and run normally when some or all of those shortcuts are left unbound — an unbound
  shortcut MUST NOT produce an error or prevent the other shortcuts from working.
- **FR-023**: The application MUST run with documented defaults for every setting when no
  configuration is present. The defaults are: flat list presentation, overlay shown on the active
  monitor only, and MRU entry order.
- **FR-024**: The application MUST report invalid configuration values with a message identifying
  the offending setting, fall back to that setting's default, and continue running.

**Diagnostics**

- **FR-029**: The application MUST write every diagnostic message to standard error, each
  identifying its severity and the specific setting, shortcut, or condition concerned.
- **FR-030**: The application MUST additionally raise a desktop notification for conditions the
  user has to act on: an invalid configuration value, failure to register its named shortcuts with
  the compositor, and failure to reach the compositor at start-up.
- **FR-031**: Conditions the application recovers from on its own — notably reconnecting to the
  compositor — MUST be reported on standard error only, and MUST NOT raise a notification.
- **FR-032**: When the notification service is unavailable, the application MUST continue running
  with standard error reporting alone and MUST NOT fail because a notification could not be shown.

**Robustness**

- **FR-025**: The application MUST report a clear, actionable error and exit non-zero when it
  cannot reach the compositor **at start-up**. Losing an already-established connection while
  running is handled by FR-026a instead, and MUST NOT cause the application to exit.
- **FR-026**: The application MUST reflect workspace, window, and monitor changes that occur while
  it is running, so that a subsequently opened overlay shows current state.
- **FR-026a**: When the compositor connection is lost while the application is running, the
  application MUST close any open overlay without committing a selection, and MUST retry the
  connection with increasing delay between attempts, up to a capped interval, indefinitely.
- **FR-026b**: On reconnecting, the application MUST rebuild its view of workspaces, windows, and
  monitors from the compositor and re-register its named shortcuts, so that the user's existing
  compositor bindings work again with no action from the user.
- **FR-026c**: The activation history MUST be discarded on connection loss and rebuilt from
  activations observed after reconnecting.
- **FR-026d**: While disconnected, the application MUST NOT consume resources by retrying without
  delay, and MUST NOT display an overlay.
- **FR-027**: A selection whose target workspace or monitor no longer exists at commit time MUST be
  treated as a cancellation rather than producing an error or an incorrect move.
- **FR-028**: Triggering the switcher shortcut while an overlay is already open MUST NOT create a
  second overlay.

### Key Entities

- **Workspace**: An ordinary compositor workspace. Has a name, the monitor it is currently bound
  to, an ordered set of windows, and whether it is the active workspace of its monitor.
- **Monitor**: A connected display. Has a name, a geometry, exactly one active workspace, and
  whether it currently holds keyboard focus.
- **Window**: An application window belonging to exactly one workspace. Has a human-readable title
  used as its label in both presentations, and a position and size within its workspace used to
  place its rectangle in the miniature.
- **Switcher Session**: The transient state of one open overlay — the ordered entries presented,
  which entry is highlighted, the monitor the session started from, and whether it was cancelled.
- **Configuration**: User-supplied settings — presentation style (default: flat list), overlay
  placement (default: active monitor only), and entry order (default: MRU). Key combinations are
  not part of this entity; they live in the compositor's configuration and reach the application as
  named shortcuts.
- **Activation History**: The session-scoped record of the order in which workspaces were last
  active, used to produce MRU ordering. Rebuilt from scratch each time the application starts.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The overlay is visible to the user within 150 ms of pressing the switcher
  combination, so
  a fast tap-and-release never leaves the user looking at a blank screen.
- **SC-002**: The selected workspace is visible on the target monitor within 300 ms of releasing
  the modifier.
- **SC-003**: Across 100 consecutive swaps in a two-monitor setup, zero windows are lost, closed,
  or left on the wrong workspace.
- **SC-004**: A user who has never used the tool can reach a named workspace out of at least ten,
  on first attempt, without consulting documentation.
- **SC-005**: The overlay presents correctly with 1 to 4 connected monitors and with at least 20
  workspaces, with every entry rendered at its normal readable size and every workspace reachable
  through navigation by scrolling.
- **SC-006**: The application starts and operates correctly with no configuration file present, and
  its shortcuts work once the documented bind lines are present in the compositor configuration —
  no further setup is required.
- **SC-007**: Repeated use over an 8-hour session leaves no orphaned overlays on screen and no
  growth in the number of workspaces beyond those the user created.
- **SC-008**: For any workspace, the miniature shows one labelled rectangle per window in the same
  relative arrangement as the real workspace, whether or not that workspace is currently visible on
  a monitor.
- **SC-009**: After the compositor is restarted, the shortcuts work again within 10 seconds without
  the user restarting or reconfiguring anything.
- **SC-010**: Every swap leaves the user in exactly one of two states — fully swapped, or exactly
  as they were before with the failure reported. No half-swapped state is ever observable.

## Assumptions

- The target environment is the Hyprland compositor on Wayland; other compositors are out of scope.
- The application runs as a background process for the duration of the user's session so that it
  can receive its shortcuts, observe modifier release, and keep compositor state current.
- Key combinations are bound by the user in the compositor's own configuration and delivered to the
  application as named shortcuts; the application does not grab keys itself. Once the overlay is
  open the application observes the modifier release directly, which is what makes commit-on-release
  possible (FR-022a).
- A "swap" affects only the pair of workspaces involved; other workspaces and their monitor
  bindings are untouched.
- Workspace names shown in the overlay are the names the compositor reports (named workspaces show
  their name, numbered workspaces show their number).
- The overlay is keyboard-driven; mouse interaction is out of scope for this feature.
- A desktop notification service is normally present in the user's session, but the application
  treats it as optional and degrades to standard error alone when it is absent.
- Whatever supervises the application (service manager or session config) captures standard error;
  the application does not manage log files or rotation itself.
- Only ordinary workspaces participate; special/scratchpad workspaces are excluded from listing and
  swapping.
- Configuration is a single user-editable file read at start-up; live reload is out of scope.
- The MRU activation history is session-scoped and is not persisted across application restarts; on
  a fresh start the order falls back to the compositor's order until workspaces are used.
- Multi-user, remote, and nested compositor sessions are out of scope.
- Theming and appearance customisation beyond the two prescribed presentations is out of scope.
