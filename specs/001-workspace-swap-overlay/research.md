# Phase 0 Research: Workspace Swap Overlay

**Feature**: `001-workspace-swap-overlay` | **Date**: 2026-07-28

All Technical Context unknowns are resolved here. Findings marked **[verified]** were confirmed
against the live Hyprland instance on the development machine (Hyprland 0.55.4, Wayland 1.25,
Rust 1.96); findings marked **[spike]** are decisions whose fine detail must be confirmed by a
short experiment during implementation, and each names the experiment.

## R1 — Language and runtime

**Decision**: Rust 1.96, edition 2024, a single binary crate.

**Rationale**: The application is a long-lived Wayland client that must hold a socket connection, a
Wayland connection, and an event loop with no garbage-collection pauses inside a 150 ms budget
(SC-001). Rust has first-class, maintained Wayland client bindings (`wayland-client`,
`smithay-client-toolkit`) which is the deciding factor — the alternatives' Wayland story is weaker.
The toolchain is already installed. [verified]

**Alternatives considered**: **C++** — matches Hyprland itself and would allow reusing its protocol
headers directly, but brings manual lifetime management to a program whose central risk is
mishandling state across a compositor reconnect. **Go** — pleasant concurrency, but Wayland
bindings are thin and would mean generating protocol glue by hand for layer-shell as well as the
Hyprland protocol. **Python** — fastest to write, but pulling a GUI toolkit in for the overlay and
meeting the latency budget in an interpreted process is the wrong trade for a daemon that runs all
session.

## R2 — Compositor state and command transport

**Decision**: Talk to Hyprland's two UNIX sockets directly, implemented in `hypr/ipc.rs` and
`hypr/events.rs`, with no Hyprland client crate.

- Commands/queries: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`. Write a
  request, read the response to EOF, one connection per request. `j/monitors`, `j/workspaces`,
  `j/clients` return JSON; `/dispatch …` returns `ok`.
- Events: `…/.socket2.sock`, a persistent connection streaming `EVENT>>DATA` lines.

Both socket paths and the JSON shapes are **[verified]** on 0.55.4.

**Rationale**: The protocol is a line-oriented text format that a couple of hundred lines cover
completely, and every field this feature needs is already in the JSON. A client crate would add a
dependency that tracks Hyprland releases on its own schedule — a version-skew risk on the one
interface the product cannot function without — in exchange for code we would still have to
understand. Principle I favours the direct call.

**Alternatives considered**: The **`hyprland` crate** (nice ergonomics, but an extra release
treadmill and it wraps the same two sockets). **`ext-workspace-v1`**, which Hyprland supports
**[verified]** and which would deliver workspace state over Wayland — rejected because it carries
no window geometry, so the miniatures (FR-015a) would still need the IPC path, and maintaining two
sources for one piece of knowledge violates Principle III. **Shelling out to `hyprctl`** — a
process spawn per query, and parsing a CLI's human output, for no benefit over the socket.

## R3 — Shortcut delivery

**Decision**: Register two named shortcuts through `hyprland-global-shortcuts-v1`, with app id
`hypr-swap` and shortcut ids `switcher` and `new-workspace`. The user binds them with Hyprland's
`global` dispatcher. The protocol XML is vendored under `protocols/` and pinned; `build.rs` runs
`wayland-scanner` over it.

**Rationale**: This is the mechanism the spec's clarification session settled on (FR-022), and it
is the only one on Wayland that lets the compositor own the key combination while the application
owns the action name. Hyprland's support is **[verified]** — `hyprctl globalshortcuts` exists as a
command and the protocol is implemented in 0.55.4. The protocol's `register_shortcut` takes
`(id, app_id, description, trigger_description)` and each shortcut delivers `pressed` and
`released` events with timestamps.

An important property: a global shortcut is **anonymous** — the client is not told which keys
trigger it. That single fact drives R4, because the application cannot simply "watch the Alt key";
it has to discover which modifiers are held at the moment the overlay takes focus.

**Alternatives considered**: **`org.freedesktop.portal.GlobalShortcuts`** — the portal route adds a
D-Bus dependency and an XDG portal implementation to the runtime requirements, and a known Hyprland
issue floods `Activated` signals when a portal shortcut is bound with `binde`. **A CLI that signals
a running daemon** (`hypr-swap open` bound via `exec`) — rejected because an `exec` bind gives no
release information whatsoever and would forfeit FR-022a, and it spawns a process per keypress.

## R4 — Commit-on-release mechanism (FR-002, FR-005, FR-022a)

**Decision**: When the overlay opens, its layer surface requests **exclusive** keyboard
interactivity. On `wl_keyboard.enter` the compositor sends the current `modifiers` state; the
application records the depressed-modifier mask as `initial_mods`. It then commits the highlighted
selection the moment a subsequent `modifiers` event shows that **any** modifier present in
`initial_mods` has been released.

Three trigger paths, in priority order:

1. **Primary** — a `modifiers` event where `current & initial_mods != initial_mods`. This is the
   hold-and-release interaction.
2. **Fast-tap fallback** — the shortcut's `released` event arrives before the surface has ever
   received keyboard focus. The user tapped and let go before the overlay could map. Commit the
   initial highlight immediately and never show the overlay. This is what makes the "tap Alt-Tab
   to bounce back" gesture correct rather than merely fast, and it covers the spec's
   "modifier released before the overlay has finished appearing" edge case.
3. **Sticky mode** — `initial_mods` is empty (see R15). Commit on Enter instead.

**Rationale**: Because shortcuts are anonymous (R3), the identity of the modifier must be
discovered rather than configured — and the set held at the instant the shortcut fires is exactly
the set the user is holding. Comparing against the initially-held set, rather than against zero,
also behaves correctly when the user is holding an unrelated modifier.

**[spike]** Two behaviours must be confirmed before the switcher is wired up, because the whole
interaction rests on them: (a) that Hyprland delivers `wl_keyboard.modifiers` to an exclusive-mode
layer surface both on `enter` and on each subsequent change, including the release of a modifier
that participates in an active bind; (b) that the round trip from shortcut `pressed` to first frame
stays inside SC-001's 150 ms. The experiment is a ~100-line throwaway client that maps an overlay
layer surface on `pressed` and logs every modifiers event with a timestamp. If (a) fails, the
fallback is `keyboard-shortcuts-inhibit-unstable-v1` (present in 0.55.4 **[verified]**) for the
lifetime of the overlay, which forces raw key delivery — rejected as the default because it
suppresses *all* the user's binds while open, which is a heavier hammer than the feature needs.

**Alternatives considered**: **Relying on the shortcut's own `released` event** as the primary
signal — rejected: Hyprland fires bind release on the release of the bind's *key* (Tab), not its
modifier, so `Alt+Tab, Tab, Tab, release Alt` would commit on the first Tab release. It is kept
only as the fast-tap fallback, where no keyboard focus exists to observe anything better.
**Polling the modifier state over IPC** — rejected: polling in a hot loop for a hold gesture,
against Principle I and the idle-CPU goal.

## R5 — Repeat trigger while the overlay is open (FR-003, FR-028)

**Decision**: A `pressed` event on the switcher shortcut while a session is already open
**advances the highlight by one** and does not open a second overlay.

**Rationale**: This falls out of how Hyprland dispatches binds. If the user binds `ALT, TAB`, the
compositor consumes Tab and dispatches the bind; the key never reaches our surface, even with
exclusive keyboard focus. Without this rule, the second Alt-Tab of an Alt-Tab-Tab gesture would do
nothing and the feature would feel broken with the most natural binding of all. Treating a repeat
trigger as "next" makes the classic gesture work by construction, and simultaneously satisfies
FR-028's no-second-overlay requirement — the two requirements have one implementation.

Backwards navigation still arrives as an ordinary key event: `Shift+Alt+Tab` does not match a
`ALT, TAB` bind, so the compositor forwards it, and `ui/mod.rs` sees Shift+Tab (FR-004a).

**Alternatives considered**: **Ignoring repeat triggers entirely** — literal compliance with
FR-028 and a broken Alt-Tab. **Inhibiting compositor shortcuts while open** so Tab reaches us
directly — see R4; it disables every other bind for the duration and is not needed once repeat
triggers advance the highlight.

## R6 — Overlay surface and rendering stack

**Decision**: `wlr-layer-shell-unstable-v1` on the **overlay** layer, one surface per monitor the
overlay is shown on, drawn into a `wl_shm` buffer with **cairo** and laid out with **pango**.

**Rationale**: The overlay layer is what puts the surface above fullscreen windows (FR-018) and
layer-shell is what allows exclusive keyboard interactivity (FR-002a) — no other shell gives both.
For the drawing itself, the content is rectangles, text and rounded boxes; cairo does that in a few
hundred lines against system libraries that are already installed (cairo 1.18.4, pango 1.58.0
**[verified]**). Pango is the specific reason to prefer this over a raw rasteriser: it does
international text shaping and, directly relevant to FR-015b, `ellipsize` gives truncation with a
visible ellipsis for free, plus the measurement needed to lay out entries.

**Alternatives considered**: **GTK4 + gtk4-layer-shell** (both installed) — the fastest route to
a window, but it owns the connection and the main loop, so the Hyprland shortcuts protocol would
have to be bolted onto GDK's display, and a retained widget tree plus CSS is a large amount of
machinery for a static, keyboard-only list. Principle I rules it out. **`iced`/`egui` with a
layer-shell backend** — a renderer, a GPU context and an immediate-mode framework for a page of
text. **tiny-skia + cosmic-text** — no system dependency, but re-implements the text measurement
and ellipsis that pango already provides. **GPU rendering (`wgpu`)** — nothing here is
fill-rate-bound; a shm buffer redrawn on navigation is well inside budget.

## R7 — Miniatures without screen capture (FR-015a, FR-015b, SC-008)

**Decision**: Build each miniature from the geometry Hyprland already reports. `j/clients` gives
every window's `at: [x, y]`, `size: [w, h]`, `workspace`, `floating`, `title`, and whether it is
`mapped` **[verified]** — for windows on workspaces that are not currently displayed as well as
for visible ones. Normalise those coordinates against the geometry of the monitor the workspace is
bound to, and paint one labelled rectangle per window, floating windows drawn on top in
`clients` order.

**Rationale**: This satisfies "equally accurate for workspaces that are not currently visible on
any monitor" exactly, because the compositor's layout state does not depend on whether a workspace
is being scanned out. It also keeps the product free of `hyprland-toplevel-export-v1` and the
screencopy permission surface that comes with it.

**Alternatives considered**: **`hyprland-toplevel-export-v1` / `wlr-screencopy`** — real pixels,
but FR-015a forbids capture, and they cannot produce a thumbnail for a workspace that has never
been composited, which is precisely US3-AS3. **Rendering the layout tree via a Hyprland plugin** —
a whole second deliverable in a different language and build system.

## R8 — Executing and rolling back a swap (FR-010, FR-013a, SC-010)

**Decision**: `actions.rs` turns a committed selection into a `CommandPlan` — an ordered list of
dispatchers — plus a `RollbackPlan` computed from the pre-state before anything is sent. The plan
is dispatched as **one batched request** on socket1 (`[[BATCH]]cmd;cmd;…`, **[verified]** working),
then the resulting state is read back and compared with the expected end state. On mismatch, the
rollback batch is sent and the failure is reported (FR-013b); if the read-back after rollback also
mismatches, the resulting state is reported instead (FR-013c).

The three cases, all **[verified]** as available dispatchers:

| Case | Plan |
|---|---|
| Selected workspace is on the focused monitor | `workspace <id>` |
| Selected workspace is the *active* workspace of another monitor | `swapactiveworkspaces <monA> <monB>`, then `focusmonitor <monA>` |
| Selected workspace is bound to another monitor but not shown there | `moveworkspacetomonitor <sel> <monA>`, `moveworkspacetomonitor <act> <monB>`, `focusworkspaceoncurrentmonitor <sel>` |

**Rationale**: Batching is what keeps SC-010's "no half-swapped state is ever observable" honest —
the compositor applies the whole batch within one pass rather than across several round trips, so
there is no intermediate frame for the user to see. Verify-then-undo, rather than a general
transaction abstraction, is the smallest thing that delivers all-or-nothing semantics: the
compositor is the source of truth, so comparing against it is both the check and the test oracle.
Using `swapactiveworkspaces` for the common case means Hyprland performs the exchange itself
instead of the application simulating one with two moves.

**[spike]** The third row needs empirical confirmation of two details on 0.55.4: whether
`moveworkspacetomonitor` leaves the destination monitor's active workspace unchanged, and where
keyboard focus lands after the pair of moves. The experiment is a scripted three-workspace,
two-headless-output scenario run under the E2E harness (R14) before `actions.rs` is finalised; it
becomes `e2e_swap_inactive_target`. If the observed behaviour differs, only the plan table above
changes — the surrounding verify/rollback machinery does not.

## R9 — New-workspace resolution (FR-020, FR-021)

**Decision**: Take the set of workspace ids currently known to the compositor, pick the lowest
positive integer not in it, and dispatch `focusworkspaceoncurrentmonitor <n>` — which both binds
the new workspace to the focused monitor and activates it. If the focused monitor's active
workspace currently has no windows, do nothing at all (no dispatch, no diagnostic).

**Rationale**: Hyprland creates a workspace implicitly on first focus, so "create" and "switch to"
are one dispatch, and `focusworkspaceoncurrentmonitor` is the variant that binds it to the focused
monitor rather than pulling it from wherever it lives. The emptiness guard is FR-021 verbatim and
is what stops repeat presses from accumulating workspaces (SC-007).

**Alternatives considered**: `workspace emptyn` / `emptynm` — Hyprland's own "next empty
workspace" selectors, rejected because their notion of *which* empty workspace differs from
FR-020's "lowest number not currently in use" and the requirement is specific.

## R10 — Ordering and activation history (FR-008a–d, FR-026c)

**Decision**: `state.rs` maintains a `Vec<WorkspaceId>` most-recent-first, updated from socket2
events — `workspace`, `focusedmon`, `moveworkspace`, `createworkspace`, `destroyworkspace` — never
from the application's own actions. `ordering.rs` is a pure function
`(world, config.order) -> (Vec<Entry>, highlight_index)`.

**Rationale**: Driving MRU from compositor events rather than from our own commits is what makes
FR-008c work — a switch made by the user's own `SUPER+3` bind, or by any other tool, updates the
history identically, because the application only ever learns about activations one way. It also
means a cancelled session cannot pollute the history (US1-AS5), since nothing was activated.
History is dropped on disconnect and rebuilt after reconnect (FR-026c) for the same reason: events
missed while disconnected would leave a history that is confidently wrong.

## R11 — Configuration (FR-023, FR-024)

**Decision**: TOML at `$XDG_CONFIG_HOME/hypr-swap/config.toml`, falling back to
`~/.config/hypr-swap/config.toml`. Exactly three keys, all optional. Parsing is per-setting: an
unparseable value or an unknown enum variant reports that setting and falls back to that setting's
default, leaving the others intact. A missing file is not a diagnostic. Full schema in
[contracts/config.md](./contracts/config.md).

**Rationale**: TOML is what Hyprland users already meet in the rest of their tooling, and `serde`
+ `toml` give per-field error locations, which is what FR-024's "naming the offending setting"
needs. Keeping validation per-setting rather than whole-file means one typo cannot silently reset
the user's other choices.

**Alternatives considered**: **Hyprland's own config syntax** — familiar, but there is no reusable
parser for it outside Hyprland and it would have to be hand-written. **A `~/.config/hypr-swap.conf`
key=value file** — needs a bespoke parser and error reporter for no gain over TOML.

## R12 — Diagnostics (FR-029–032)

**Decision**: One `diag.rs` with `error!`/`warn!`/`info!`-style entry points writing a fixed-shape
line to stderr, and a `notify` flag on the record. Notifications are raised by spawning
`notify-send` **detached, never waited on**; a spawn failure is swallowed after one stderr line.

**Rationale**: The set of conditions that notify is small, closed, and enumerated by FR-030, so a
policy flag on each record is enough; no logging framework earns its place here. Choosing the
`notify-send` subprocess over a D-Bus client crate keeps `zbus`/`dbus` and an async runtime out of
the dependency tree, and gives FR-032's degradation for free — if the binary or the service is
absent, the spawn fails, one line goes to stderr, and the application carries on. Not waiting on
the child is what stops a wedged notification daemon from stalling the event loop.

**Alternatives considered**: **`notify-rust`** — cleaner API, but pulls a D-Bus stack in for four
possible messages per session. **`tracing`/`log` + `env_logger`** — configurable levels nobody
asked for (Principle II); the spec wants everything on stderr, always.

## R13 — Reconnection (FR-025, FR-026a–d)

**Decision**: Failure to connect **at start-up** is fatal: report and exit non-zero. Losing an
established connection is not: close any open overlay without committing, then retry with
exponential backoff starting at 100 ms, doubling to a 5 s cap, indefinitely, with the delay reset
after a successful connection. Reconnection rebuilds world state from `j/monitors`, `j/workspaces`,
`j/clients`, re-registers both shortcuts, and clears the activation history. Reconnect attempts and
success are stderr-only, never notifications (FR-031).

**Rationale**: The split matches the spec exactly — at start-up a missing compositor means
misconfiguration the user must fix, while mid-session it almost always means Hyprland restarted and
will be back. 100 ms first retry keeps the common case (a restart) inside SC-009's 10 s budget with
room to spare; the 5 s cap satisfies FR-026d's "must not consume resources by retrying without
delay" for a compositor that is gone for good.

Re-registering the shortcuts is required rather than optional: the Wayland connection dies with the
compositor, taking the shortcut objects with it, so FR-026b's "the user's existing binds work again
with no action from the user" depends on re-registering under the same app id and ids.

## R14 — Testing strategy (Principles IV and V)

**Decision**: Two layers.

*Unit* — in-module `#[cfg(test)]` tests over the I/O-free modules: `ordering`, `actions` (plans and
their rollbacks, including the FR-013c double-failure path), `session`, `config`, `ui::layout`,
`hypr::events` line parsing, and the `model` deserialisers against captured JSON fixtures. These
need no compositor and no display.

*E2E* — integration tests that launch a **nested Hyprland instance** (its own
`HYPRLAND_INSTANCE_SIGNATURE`, its own config, `WAYLAND_DISPLAY` pointing at the host session),
add headless outputs with `hyprctl output create headless` for multi-monitor scenarios
**[verified as a supported command]**, spawn `foot` toplevels with known titles for content
assertions (`foot` is present **[verified]**), inject real key events through
`virtual-keyboard-unstable-v1` (present in 0.55.4 **[verified]**), and assert by reading the nested
instance's own IPC state.

**Rationale**: This is what Principle V means by the real external interface — the test presses
keys against a compositor that is running the user's documented bind lines, and asks that
compositor what happened. Nothing is asserted through an internal API. A nested instance rather
than the developer's session is what makes the suite safe to run repeatedly and safe in CI, and
`hyprctl output create headless` is what makes cross-monitor swapping testable without a second
physical display.

Per the constitution's Testing Standards, the substitutions are documented here: the E2E suite
substitutes **headless outputs for physical monitors** and **`foot` for arbitrary user
applications**; everything else — compositor, binds, key events, Wayland protocols, IPC — is real.
The two rollback tests additionally substitute a **fault-injecting IPC layer** enabled by an
environment variable, because a genuine dispatcher failure cannot be provoked from outside; this is
the one place an E2E test reaches past the real interface, and it is confined to those tests.

**[spike]** The nested instance must be confirmed to start, accept `output create headless`, and
accept virtual-keyboard input under the developer's session before the E2E suite is built out;
this is the first task of the E2E work and gates the rest.

**Alternatives considered**: **Testing against the developer's live session** — destructive, since
tests move the user's real workspaces between monitors. **A mock compositor** — would test the
application against our own beliefs about Hyprland, which is exactly the class of bug E2E exists to
catch. **Screenshot comparison for the overlay** — brittle across fonts and scaling; the
presentation tests instead assert on the layout module's computed geometry (unit) plus the
overlay's presence and monitor placement via `hyprctl layers` (E2E).

## R15 — Shortcut bound without a modifier

**Decision**: If no modifier is held when the overlay takes keyboard focus, the session enters
**sticky mode**: the overlay stays open, navigation works as usual, **Enter** commits and **Escape**
cancels.

**Rationale**: A shortcut bound to a bare key (`bind = , F13, global, hypr-swap:switcher`) has no
modifier to release, so FR-002's close condition can never occur. The spec does not cover this
configuration. Committing immediately would make the overlay useless; leaving it open forever with
no way to commit would strand the user's keyboard. Sticky mode is the smallest behaviour that keeps
every documented requirement true and the application usable. Recorded here explicitly rather than
implemented quietly, and worth raising as a spec amendment if the reviewer disagrees.

## R16 — Overlay sizing and scrolling (FR-019, SC-005)

**Decision**: Entries have a fixed intrinsic size that does not vary with workspace count — list
rows are one text line tall with fixed padding; grid miniatures have a fixed cell size with the
monitor's aspect ratio. The overlay is capped at **80 % of the monitor's width and 80 % of its
height** (the documented fraction FR-019 requires), and when entries exceed that, the viewport
scrolls to keep the highlighted entry in view, with the highlight never landing closer than one
entry to a scrolled edge. Concrete pixel values live in
[contracts/config.md](./contracts/config.md) as documented constants, not as settings.

**Rationale**: FR-019 forbids scaling entries down, so the only remaining variable is how much of
the monitor the overlay may claim; 80 % keeps the surrounding desktop visible as context, which is
what makes the overlay read as an overlay. Keeping the values as constants rather than settings
respects Principle II — the spec explicitly puts theming out of scope — while still keeping them in
one place (Principle III) so they can become settings later if a requirement ever asks.

The viewport arithmetic sits in `ui/layout.rs` as a pure function of (entry count, entry size,
monitor size, highlight index), which is what lets SC-005's 20-workspace case be unit-tested at
every monitor size without a compositor, with the E2E test confirming the same on real outputs.

## Resolved unknowns

| Unknown from Technical Context | Resolution |
|---|---|
| Language and runtime | R1 — Rust 1.96 |
| How compositor state is read and changed | R2 — Hyprland IPC sockets, direct |
| How shortcuts reach the application | R3 — `hyprland-global-shortcuts-v1` |
| How modifier release is observed | R4 — exclusive layer-shell keyboard focus + `modifiers` |
| How the overlay is drawn and stacked | R6 — `wlr-layer-shell` overlay layer, shm + cairo/pango |
| How miniatures are produced without capture | R7 — window geometry from `j/clients` |
| How swaps are made atomic | R8 — batched dispatch, verify, inverse batch |
| Configuration format and location | R11 — TOML under `$XDG_CONFIG_HOME/hypr-swap/` |
| How E2E drives the real interface | R14 — nested Hyprland + headless outputs + virtual keyboard |
