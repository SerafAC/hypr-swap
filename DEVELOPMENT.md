# Developing hypr-swap

Everything you need to get from a fresh clone to a running program and a passing test suite. If you
are installing hypr-swap rather than working on it, the [README](README.md) is the shorter path; if
you are about to propose a change, [CONTRIBUTING.md](CONTRIBUTING.md) is what review looks for.

## What you need

| | |
|---|---|
| Rust | `1.96` or newer — the edition is 2024 |
| System libraries | cairo, pango and pangocairo, **development** packages |
| To run it | Hyprland `>= 0.55`, on Wayland |
| To run the end-to-end tests | the same session, plus `foot` |
| To build the documentation site | Node.js `>= 22` and **pnpm** `>= 11` — and nothing else in the repository needs them |

```bash
# Debian, Ubuntu and derivatives
sudo apt install build-essential pkg-config libcairo2-dev libpango1.0-dev

# Fedora, RHEL and derivatives
sudo dnf install gcc pkgconf-pkg-config cairo-devel pango-devel

# Arch
sudo pacman -S --needed base-devel cairo pango
```

The Rust toolchain comes from [rustup](https://rustup.rs/); `rust-version` in `Cargo.toml` is the
minimum the project supports and is what the MSRV check builds against.

## Setting up

```bash
git clone https://github.com/SerafAC/hypr-swap.git
cd hypr-swap
cargo build
```

There is no code generation step to run and nothing to vendor. `build.rs` turns
`protocols/hyprland-global-shortcuts-v1.xml` into the global-shortcuts bindings through
`wayland-scanner` as a proc-macro expansion — it produces no file on disk, so there is nothing to
regenerate and nothing to commit.

## Running it

hypr-swap is a daemon. Start it in a terminal inside your Hyprland session and its diagnostics
arrive in front of you, which is much the easiest way to work on it:

```bash
cargo run
```

It needs two key combinations bound before it does anything visible. Add these to `hyprland.conf`
(and **do not** add `exec-once` while you are developing — you want to control when it starts):

```ini
bind = ALT, TAB, global, hypr-swap:switcher
bind = SUPER, N, global, hypr-swap:new-workspace
```

Then `hyprctl globalshortcuts` confirms the running daemon registered them. Only one instance can
hold the names at a time, so stop the one your session started before running your own.

Useful while developing:

```bash
cargo run -- --config ./my-test-config.toml   # an alternative configuration file
cargo run -- --environment                     # what the daemon can actually see
cargo run -- --version
```

## Running the tests

Three tiers. The practical difference between them is what each needs from the machine.

```bash
cargo test --lib                       # unit — nothing required
cargo test --test 'e2e_*'              # end-to-end — needs a live Hyprland session
./scripts/checks.sh                    # the document checks — nothing required
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Narrower runs, when you are working on one thing:

```bash
cargo test --lib ordering              # one module's unit tests
cargo test --lib theme                 # style values, colour parsing, ranges, precedence
cargo test --lib icons                 # the matching ladder, set lookup, cache, decoding
cargo test --test e2e_harness -- nested_instance_starts    # a single E2E test
```

### Which tier needs what

**The unit tier needs nothing.** No compositor, no display, no network. In a headless environment —
a container, an automation runner, a machine with no Wayland session — this is the tier that runs,
and it is where most of the project's behaviour is actually verified.

**The end-to-end tier needs a live Wayland session with Hyprland ≥ 0.55 and `foot` installed.**
`tests/e2e/harness.rs` starts a *nested* Hyprland as an ordinary Wayland client of your own session,
adds headless outputs to it, spawns `foot` windows into them, and injects real key events through
`virtual-keyboard-unstable-v1`. Every assertion goes through the nested instance's own IPC socket.
Nothing is mocked — the daemon under test talks to a real compositor over the real protocols.

Only one nested compositor can exist at a time, so these tests serialise on an internal lock.
**Do not fight the harness with `--test-threads`**: it will not make them faster and it will make
failures confusing.

**Without a Wayland session there is no way to run this tier** — not in a container, and not in
automation. The harness nests a compositor as an ordinary Wayland client, so it needs a parent
session that can hand it a dmabuf allocator, and that needs a real GPU underneath. Both ways of
faking one were built and measured, and neither works; that is why the project has no end-to-end
CI job, and why the requirement asking for one is recorded as unmet rather than quietly dropped.
The measurements are in `specs/003-oss-release-readiness/research.md` R29.

**With a session, `docker/e2e/` gives you the same tier against pinned versions.** It is a local
compatibility-testing tool rather than a CI environment: a fixed Hyprland, a fixed toolchain and
the test dependencies, so you can ask "does this still work against the compositor the project
supports?" without changing what is installed on your machine, and tell "my compositor updated"
apart from "my change broke it". It is also the way to exercise the tier from a distribution that
does not ship a current Hyprland.

```bash
docker build -t hypr-swap-e2e docker/e2e
docker run --rm \
  --device /dev/dri/renderD128 \
  -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
  -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
  -v "$PWD:/work" -w /work \
  hypr-swap-e2e
```

[docker/e2e/README.md](docker/e2e/README.md) covers the rest, including what to do when your
`/dev/dri` nodes are not world-accessible.

**The documentation site** builds with its own toolchain, and only documentation work needs it:

```bash
pnpm install --frozen-lockfile
pnpm build         # the site, in site/
pnpm dev           # a live server at http://localhost:3000 while writing
pnpm validate      # every internal link and reference
```

**pnpm is the package manager this project uses — not npm or Yarn.** `package.json` pins it in
`packageManager`, the committed lockfile is `pnpm-lock.yaml`, and `pnpm-workspace.yaml` carries the
dependency build approvals without which an install fails outright. Installing with another manager
produces a second lockfile and an unapproved dependency tree; if `pnpm` is missing, `corepack enable`
gets you the pinned version.

What you edit is the plain Markdown under `docs/`; `docmd.config.mjs` holds the navigation order
and little else. The one directive that is not plain Markdown is `::include[]`, which pulls a
`specs/` contract — or one heading's section of it — into a page at build time, so that the page
and the contract cannot diverge. It is `scripts/docmd-include.mjs`, and it fails the build naming
the page and the target when a path or a heading slug is wrong.

## Architecture

The codebase is organised around **one seam: pure decision logic on one side, a thin I/O shell on
the other.** Almost every question about where a change belongs is answered by it.

A **pure** module decides. It is a function of already-read data — no sockets, no filesystem, no
Wayland, no clock — and it is unit-tested in its own file, next to the code it covers. The **shell**
carries data across the boundary and does what it is told: it opens sockets, paints pixels,
dispatches Wayland events, and decides nothing.

**A new decision rule belongs on the pure side.** That is the rule this seam exists to enforce. If
your change needs a new condition, a new ordering, a new fallback, a new piece of arithmetic, it
goes in a pure module with a unit test, and the shell calls it. The temptation is always to write
"just this one `if`" in the Wayland handler; resist it, because code on the shell side can only be
exercised by launching a whole compositor, and that is why the shell is kept as small as it is.

Four facts worth knowing before you edit anything:

- **One event loop** — calloop in `main.rs`, over three sources: the Wayland fd, the Hyprland event
  socket, and signals.
- **The Wayland shell never does IPC directly.** It records a `Request` in `app.outbox`, and
  `main.rs` acts on it after dispatch. This is the seam in its most concrete form.
- **Reconnection is teardown.** Losing the compositor drops the whole client — surfaces, world,
  history — and `run()` rebuilds everything after a backoff. There is no partial-reconnect state.
- **All diagnostics go through `diag.rs`.** Never `eprintln!` outside it: the policy of which
  conditions raise a desktop notification lives in one `match`, and a direct write bypasses it.

The full account, module by module, is
[the architecture page](https://serafac.github.io/hypr-swap/dev/architecture/).

## The tree

### Top-level directories

| Directory | Holds |
|---|---|
| `src/` | The program — every Rust module, listed below |
| `tests/` | The end-to-end tier: `e2e_*.rs` test binaries and the `e2e/` harness they share |
| `specs/` | The specifications, plans, contracts and research decisions, one directory per feature — the authority on what was promised |
| `docs/` | The documentation, as plain Markdown — the pages of the published site, and nothing else |
| `scripts/` | `checks.sh`, the document checks, runnable exactly as the `checks` job runs them; and `docmd-include.mjs`, the site's `::include[]` |
| `protocols/` | The vendored `hyprland-global-shortcuts-v1.xml`, a build input |
| `assets/` | `placeholder.svg`, the icon drawn where no program icon resolves — compiled into the binary |
| `target/` | Cargo's build output. Not in the repository |
| `site/` | The built documentation site, and `node_modules/` what builds it. Neither is in the repository |

At the root, beside `Cargo.toml`: `docmd.config.mjs`, `package.json`, `pnpm-lock.yaml` and
`pnpm-workspace.yaml` are the documentation site's whole configuration and its pinned generator.

`.specify/` holds spec-kit's own templates and the project constitution; `.claude/` holds the
agent configuration. Both stay in the published tree, along with `CLAUDE.md`, because the
development record is part of what is published rather than something tidied away before release.

### Modules under `src/`

Pure — a decision, unit-tested in its own file:

| Module | Decides |
|---|---|
| `config.rs` | TOML load and per-setting fallback |
| `model.rs` | `Workspace` / `Monitor` / `Window`, and their deserialisation from IPC replies |
| `state.rs` | `World` — the cached compositor view and MRU activation history, updated by applying events |
| `ordering.rs` | The order entries appear in, and which one opens highlighted |
| `actions.rs` | Selection → a command plan and its rollback plan; the new-workspace plan |
| `session.rs` | The switcher state machine |
| `theme.rs` | Colour parsing, geometry ranges and clamping, the built-in palettes, and the override → theme → default chain |
| `ui/layout.rs` | Entry metrics, the scroll viewport, miniature rect mapping, the icon slot |
| `lib.rs` | The crate root, and the supported-compositor range |

I/O, but with the decision still pure inside it and unit-tested:

| Module | The I/O | The rule tested without it |
|---|---|---|
| `hypr/mod.rs` | The Hyprland client's shared types | — |
| `hypr/ipc.rs` | Request/response and batch dispatch on socket1 | Request framing and reply parsing |
| `hypr/events.rs` | The socket2 event stream | Line parsing and the reconnection backoff |
| `icons/mod.rs` | `IconStore` | The resolve-once-per-program cache |
| `icons/entries.rs` | The desktop-entry scan | The class-to-entry matching ladder |
| `icons/iconset.rs` | Reading `index.theme` | Parsing, `Inherits`, directory scoring |
| `icons/decode.rs` | PNG via cairo, SVG via `resvg` | Decoding, over fixture bytes |

Shell — end-to-end covered only, and deliberately logic-free:

| Module | Does |
|---|---|
| `main.rs` | Start-up, the calloop event loop, reconnection |
| `ui/mod.rs` | Wayland registry, seat, layer surfaces, shm |
| `ui/shortcuts.rs` | Global-shortcut registration — and the single definition of the shortcut names, bind lines and usage text |
| `ui/render.rs` | cairo painting |
| `diag.rs` | The one diagnostic path: the stderr record format and the notification policy |

## Two tests that read documents

These are worth knowing about before they surprise you, because both fail as ordinary unit-test
failures when documentation and code disagree — which is the point:

- `theme.rs` walks the published style catalogue in
  `specs/002-overlay-visuals/contracts/style-values.md` against its own `const` tables. **Add a
  setting without documenting it and `cargo test --lib` fails.**
- `ui/shortcuts.rs` `include_str!`s `docs/user/binds.md` and asserts it quotes every bind line the
  program generates. The include is compile-time, so moving or renaming that page fails
  `cargo build` rather than going unnoticed.

## How this project is developed

Spec-first, with [spec-kit](https://github.com/github/spec-kit). A feature is specified, planned and
broken into tasks before it is written, and all of that stays in `specs/` afterwards as the record
of what was decided and why. Code comments cite requirement numbers (`FR-024`) and research
decisions (`research.md R22`) rather than restating them; the research numbering runs continuously
across features, so a citation never needs to name which one it belongs to.

`.specify/memory/constitution.md` is binding rather than aspirational: KISS, YAGNI and DRY, unit
tests for all code, end-to-end coverage of major requirements. A new abstraction or dependency is
justified in the feature's `plan.md` Complexity Tracking table.

[CONTRIBUTING.md](CONTRIBUTING.md) covers proposing a change and what review looks for.
