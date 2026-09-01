---
title: Architecture
description: The one seam the whole codebase is organised around, and every module's place on one side of it or the other.
---

The codebase is organised around a single seam: **pure decision logic on one side, a thin I/O shell
on the other**. Almost every question about where something belongs is answered by it, so it is
worth stating precisely before the module list.

## The seam

A **pure** module decides. It is a function of already-read data — no sockets, no filesystem, no
Wayland, no clock — and it is unit-tested in its own file. Given the same inputs it produces the
same outputs, and its tests are ordinary `cargo test --lib` tests that need no compositor and no
display.

The **shell** carries data across the boundary and does what it is told. It opens sockets, paints
pixels, dispatches Wayland events. It is deliberately logic-free: nothing in it decides anything a
pure module could have decided instead. It is covered end-to-end, not by unit tests, because there
is nothing there worth unit-testing.

**Any new decision rule belongs on the pure side.** That is the rule the seam exists to enforce. If
a change needs a new condition, a new ordering, a new fallback, a new bit of arithmetic — it goes in
a pure module and gets a unit test, and the shell calls it. The pull towards writing "just this one
`if`" in the Wayland handler is the pressure this design is built to resist, because the code on the
shell side can only be exercised by launching a compositor.

## Pure, I/O-free, unit-tested in-module

| Module | Decides |
|---|---|
| `config.rs` | TOML load and per-setting fallback: which settings the file supplies, and what each invalid one falls back to |
| `model.rs` | `Workspace` / `Monitor` / `Window`, and their deserialisation from the compositor's IPC replies |
| `state.rs` | `World` — the cached compositor view plus MRU activation history, updated by applying events |
| `ordering.rs` | The order entries appear in, and which one opens highlighted |
| `actions.rs` | Selection → a command plan and its rollback plan; the new-workspace plan |
| `session.rs` | The switcher state machine: what an open overlay does with each key and each release |
| `ui/layout.rs` | Entry metrics, the scroll viewport, miniature rect mapping, the icon slot |
| `theme.rs` | Colour parsing, geometry ranges and clamping, the built-in palettes, and `resolve` — the override → theme → default chain |

## I/O, but still unit-tested

These do real I/O, and each still keeps its **decision rule as a pure function over already-read
data**, with the reading and the writing around it. That is what makes them testable without a
compositor even though they are not pure modules.

| Module | The I/O | The rule that is tested without it |
|---|---|---|
| `hypr/ipc.rs` | Request/response and batch dispatch on socket1 | Request framing and reply parsing. Carries an env-gated fault-injection hook used only by the E2E rollback tests |
| `hypr/events.rs` | The socket2 event stream | Line parsing and the reconnection backoff schedule |
| `icons/entries.rs` | The desktop-entry scan | The class-to-entry matching ladder — filesystem-free, tested as such |
| `icons/iconset.rs` | Reading `index.theme` | `index.theme` parsing, `Inherits` resolution, directory scoring — also filesystem-free |
| `icons/decode.rs` | PNG via cairo, SVG via `resvg` | Decoding, over fixture bytes |
| `icons/mod.rs` | `IconStore` | The resolve-once-per-program cache |

## Shell: E2E-covered only, deliberately logic-free

| Module | Does |
|---|---|
| `main.rs` | Start-up, the calloop event loop, reconnection |
| `ui/mod.rs` | Wayland registry, seat, layer surfaces, shm |
| `ui/shortcuts.rs` | Global-shortcut registration |
| `ui/render.rs` | cairo painting |

## Five facts worth knowing before you edit

**One event loop.** calloop in `main.rs`, over three sources: the Wayland fd, the Hyprland event
socket, and signals. There is no second loop and no background thread doing IPC.

**The Wayland shell never does IPC directly.** It records a `Request` in `app.outbox`, and
`main.rs` acts on it after dispatch, in `handle_request`. This is what keeps `ui/` free of
compositor commands, and it is the seam in its most concrete form.

**Reconnection is teardown.** Losing the compositor drops the whole client — surfaces, world,
history — and `run()` rebuilds everything after a backoff. There is no partial-reconnect state, and
adding some would be a significant change in kind rather than an optimisation.

**All diagnostics go through `diag.rs`**, via `diag::report(Condition, subject, message)`, which
owns both the stderr record format and the notification policy. Never `eprintln!` outside it — the
policy of which conditions raise a desktop notification lives in one `match`, and a direct write
bypasses it silently.

**Shortcuts have one definition.** Bind lines, shortcut names and the usage text all derive from
`ui/shortcuts.rs::Shortcut`. Change a shortcut there and nowhere else; a unit test asserts the
published bind page still quotes every line the code generates, so the documentation cannot drift
away from the binary.

**`theme.rs` owns every visual default** — the eleven colours, the font, and the ten geometry values
with their ranges — as `const` catalogues. `config.rs` carries only what the user wrote; `ui/` only
ever sees a resolved `Style`. A unit test walks the published style catalogue against those
`const`s, so a new setting that is not documented fails `cargo test --lib`.

## Where the protocol bindings come from

`build.rs` and the vendored `protocols/hyprland-global-shortcuts-v1.xml` generate the
global-shortcuts bindings through `wayland-scanner`. This is a proc-macro expansion rather than a
generated file on disk, so there is nothing to regenerate and nothing to commit — but it does mean
the XML is a build input, and editing it changes the compiled interface.

## Why it is built this way

The reasoning behind each of these choices, with the alternatives that were rejected, is recorded
as numbered decisions in the specifications rather than restated here — see
[`specs/001-workspace-swap-overlay/research.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/001-workspace-swap-overlay/research.md)
(R1–R17) and
[`specs/002-overlay-visuals/research.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/002-overlay-visuals/research.md)
(R18–R28). The numbering is continuous across features, so a citation such as `research.md R22` in a
code comment is unambiguous without naming which feature it belongs to. Cite those decisions rather
than re-litigating them.
