# Quickstart: Validating Open-Source Release Readiness

**Feature**: `003-oss-release-readiness` | **Date**: 2026-08-30

How to prove this feature works, end to end. Most of it is verified by checks that run themselves
([plan.md](./plan.md) → tier table); what is here is the part a person has to run or judge — the
scenarios that need a human, and the checklists FR-092 counts as a verification tier.

## Prerequisites

- A live Hyprland session (≥ 0.55) with `foot`, for the E2E scenarios.
- Docker, for the container scenarios.
- Development tooling — none of it is compiled into the binary. The versions below are the ones
  this feature was developed and validated against (T002); anything newer is expected to work, and
  a mismatch is worth noting before blaming a check.

  | Tool | Version | Installed with | Needed for |
  |---|---|---|---|
  | `rustc` / `cargo` | 1.96.0 | rustup (the toolchain the crate already pins) | everything |
  | `cargo-deny` | 0.20.2 | `cargo install cargo-deny --locked` | advisories, licences |
  | `cargo-deb` | 3.7.0 | `cargo install cargo-deb --locked` | the Debian package |
  | `cargo-generate-rpm` | 0.21.0 | `cargo install cargo-generate-rpm --locked` | the RPM package |
  | `gitleaks` | 8.30.1 | `go install github.com/zricethezav/gitleaks/v8@latest` | FR-066a, once |
  | Node.js | 24.11.1 | nvm (**≥ 22 required**) | the site only |
  | pnpm | 11.3.0 | `corepack enable` (**≥ 11 required**) | the site only |

  The three `cargo install` targets can be installed in one invocation. `gitleaks` is needed once,
  for the FR-066a history review, and is not a Rust tool — a distribution package of the same
  version does just as well. A `go install` build does not embed its version, so `gitleaks version`
  prints `version is set by build process` rather than a number; read the real one with
  `go version -m $(command -v gitleaks)`.

  **Node and pnpm are for the documentation site and nothing else.** They are needed only to
  build the site ([research.md](./research.md) R31, R31a, R48); a contributor working on the
  program never installs either, a contributor *writing documentation* needs them only to preview
  the result, and no check outside the `docs` job uses them. **pnpm is the project's package
  manager and the only one supported** — it is pinned in `package.json`'s `packageManager` field,
  so `corepack enable` gets the exact version; npm or Yarn would write a second lockfile and ignore
  the build approvals in `pnpm-workspace.yaml`. The site's own dependencies are pinned in the root
  `pnpm-lock.yaml` and installed with `pnpm install --frozen-lockfile` — they are not global tools,
  so they are not listed here. The tree is `@docmd/core` 0.9.4 and 60 packages in total, which pnpm
  hardlinks from one store shared by every checkout on the machine, and which is what CI caches.

## 1. The program's own additions (FR-112–FR-118)

```bash
cargo test --lib                     # version parse and range, catalogue walk, advisory expiry
cargo test --test e2e_lifecycle      # the nine tests of plan.md's E2E mapping
```

Then confirm by hand what a user would see:

```bash
cargo build --release
./target/release/hypr-swap --version        # 1.0.0+v1.0.0-<n>-g<sha> on a non-tag build (FR-104)
./target/release/hypr-swap --environment    # the block the bug form asks for (FR-116)
./target/release/hypr-swap 2>&1 | head -1   # INFO daemon: hypr-swap … started (FR-112)
# …then Ctrl-C:                             # INFO daemon: stopping: SIGINT (FR-113)
```

**Expected**: the version string matches [contracts/cli.md](./contracts/cli.md); the environment
block has every line, with an explicit word where a value is unavailable and **no** configuration
file contents; the start record names the same version `--version` printed; the stopping record is
the last line written.

Then the negative cases:

```bash
XDG_RUNTIME_DIR=/nonexistent ./target/release/hypr-swap; echo "exit $?"
```

**Expected**: the existing `ERROR compositor:` record, then `INFO daemon: stopping: cannot reach
the compositor at start-up`, then exit 3 — a daemon that dies before it starts still says why.

## 2. Nothing else about the daemon changed (FR-114)

```bash
cargo test --test 'e2e_*'
```

**Expected**: the whole 001/002 suite passes untouched. This is the check that the lifecycle
records joined the existing record rather than reshaping it.

## 3. The documentation cannot drift (FR-083, SC-030)

```bash
pnpm install --frozen-lockfile && pnpm build && pnpm validate && ./scripts/checks.sh
```

Then break it deliberately:

```bash
# add a setting to theme.rs's catalogue without documenting it
cargo test --lib theme        # expected: FAILS, naming the undocumented setting
```

**Expected**: the catalogue walk fails. This is the mechanism behind SC-030's "zero drift
possible" — the documentation is not checked by a reviewer's memory.

## 4. A reader is served by exactly one document (FR-084, SC-029)

Judged, not run — the inspection tier for the README and `DEVELOPMENT.md`:

1. Read `README.md` end to end. Does it answer what it is, what it is for, what it requires, how
   to install, how to configure, how to use — in that order, with no development instructions?
2. Pick five questions from the map in [contracts/documentation.md](./contracts/documentation.md).
   For each, does exactly one document answer it, and do the others link rather than restate?
3. Time a newcomer's path: landing page → working overlay. **SC-026**: under 15 minutes.
4. Time a developer's path: `DEVELOPMENT.md` → every test tier run. **SC-032**: under 30 minutes.
5. Give someone the site's styling page alone and ask for a complete custom appearance —
   colours, font and dimensions, no source reading. **SC-031**.

### The walk, as it went (T111–T114, 2026-09-06)

Walked once against this tree, on a live Hyprland session. Items 1 and 2 are judgements; items
3–5 were walked with the commands the documents actually give, and what follows is what happened
rather than what was hoped for.

**Item 1 — the README answers the six questions in order.** It does: what it is (the opening
paragraph), what it is for (*What it is for*), what it requires (*Requirements*), how to install
(*Install*), how to configure (*Configure it*), how to use (*Use it*), in that order and with no
development instructions — `./scripts/checks.sh` holds the last of those mechanically.

**Item 2 — five questions from the map, each with one home.** *How do I install it?* → site
`user/install.md`, README carries the short path and links. *What are the style values?* → the 002
contract, `::include[]`d into `user/styling.md`, README links. *What counts as a breaking change?*
→ `contracts/versioning.md`, linked from the changelog's header. *How do I build and test it?* →
`DEVELOPMENT.md`; the README does not, and the `docs-map` check greps to keep it that way. *What is
verified by what?* → the plan's tier tables, `::include[]`d into `dev/verification.md`, with
`CONTRIBUTING.md` pointing at the coverage table. Each has exactly one authoritative home and the
others link.

**One gap found and closed.** SC-029 asks that a reader can state *how to report a vulnerability*
within 60 seconds of arriving. `SECURITY.md` says it well, but nothing routed a reader to it: the
README's *Documentation* list named the user guide, `DEVELOPMENT.md`, `CONTRIBUTING.md` and
`CHANGELOG.md`, and stopped there. The map makes `SECURITY.md` the authoritative home for that
question and requires the others to link; the README now does. This is exactly the kind of thing
the inspection tier exists to catch — every mechanical check passed while the question had no
route.

**Item 3 — SC-026, landing page to working overlay. Passed, with the caveat that the timed path
is the from-source one.** Nothing is released yet, so the four package channels cannot be walked:
the landing page → *User guide* → *Installing* → *From source* is the only route open today, and
it is the one that was timed. Reading to the point of knowing what to run took about three
minutes; `cargo build --release` on a warm dependency cache took 36 seconds and `install -Dm755`
was instant; the *Binding the shortcuts* page gives the two `bind` lines and tells you to confirm
with `hyprctl globalshortcuts`, which listed `hypr-swap:switcher` and `hypr-swap:new-workspace` as
promised. Comfortably inside 15 minutes. **What the walk did not do** is edit the walker's live
`hyprland.conf`, so the final keypress was not made; the daemon's own registration was confirmed
instead. Re-walk this one against the published packages at the first release, when the shorter
channels exist — that is the path most newcomers will actually take, and it is the one still
unmeasured.

**Item 4 — SC-032, `DEVELOPMENT.md` to every test tier run. Passed.** Its *Running the tests*
block is a copyable list, and every command in it was run from it: `cargo test --lib` (407 passed),
`cargo test --test 'e2e_*'`, `./scripts/checks.sh`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check`, and the site's `pnpm install --frozen-lockfile && pnpm build && pnpm validate`.
Elapsed wall-clock for the whole set was around six minutes on a warm tree, well inside 30. The one
command that could not be run as written is `cargo deny check licenses`, because `cargo-deny` was
not installed on the machine — but the page says so on the same line (`needs cargo install
cargo-deny --locked`), so the document is right and only the machine was short. *Which tier needs
what* answers the question a newcomer actually has — why the end-to-end tier fails on a headless
box — before they hit it.

**Item 5 — SC-031, a complete custom appearance from the styling page alone. Passed.** The page
carries all twenty-three settings with form, default and range: eleven colours, two font values and
ten dimensions. A configuration naming every one of them was assembled from the rendered page
without opening a source file, and the daemon accepted it in silence — no `InvalidConfigValue`,
no clamp. That silence was then given meaning with a negative control: the same file with
`highlight = "not-a-colour"` and `text_size = 9.9` produced exactly the two records the page
promises —

```text
WARN  config.style.highlight: expected #rgb, #rrggbb or #rrggbbaa, got "not-a-colour"; using #2e70cc
WARN  config.style.text_size: 9.9 is above the maximum 1; using 1
```

— a per-setting fallback for the colour and a clamp to the nearer bound for the dimension, which is
what FR-024 and FR-054 describe. The page is sufficient on its own.

## 5. The tier runs against pinned versions (FR-089)

`docker/e2e/` is a **local compatibility-testing tool**: the compositor, the toolchain and the test
dependencies at fixed versions, so "does this still work against the compositor the project
supports?" is answerable without changing what is installed on your machine. It is not a CI
environment — there is no E2E job to reproduce, because nothing can supply automation a compositor
that can host a nested one ([research.md](./research.md) R29, **failed**).

```bash
docker build -t hypr-swap-e2e docker/e2e
docker run --rm \
  --device /dev/dri/renderD128 \
  -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
  -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
  -v "$PWD:/work" -w /work \
  hypr-swap-e2e
```

**Expected**: a nested Hyprland starts inside the container against the session you handed it, and
the whole tier passes — **[verified], 86 passed, 0 failed**.

It cannot make a session; with none of your own there is no way to run this tier at all, which is
the measured limit R29 records and [docker/e2e/README.md](../../docker/e2e/README.md) states.

## 6. Every gated failure is actually caught (SC-034)

Open four deliberately broken changes, one of each kind, and confirm automation catches each
without a human:

| Break | Expected failing job |
|---|---|
| Invert an assertion in a unit test | `unit` |
| Reformat a file by hand | `fmt` |
| Add a `clippy::pedantic` violation | `clippy` |
| Use an API newer than `rust-version` | `msrv` |

**Expected**: each fails for its own reason and names the local command that reproduces it
(FR-090).

**A fifth row used to be here** — "change entry ordering so only the overlay shows it", expected to
fail `e2e` — and it was described as the one that mattered, because it is the reason FR-088 exists.
It is struck because there is no `e2e` job: no automation can supply a compositor able to host the
nested one this tier needs ([research.md](./research.md) R29, **failed**). That break is now caught
only by running the tier on a machine with a Wayland session, which is exactly the exposure the
deviation recorded at FR-088 describes.

### The walk, as it went (T048, 2026-09-07)

Four branches off `master`, one break each, opened as draft pull requests #6–#9 and closed as soon
as the verdict was in.

| Break | Red job | Elapsed |
|---|---|---|
| `ordering.rs`'s MRU highlight assertion inverted | `unit` | 43s |
| `Outcome`'s variants re-indented by hand | `fmt` | 14s |
| an uncalled function with a truncating `u64 as u32` | `clippy` | 36s |
| `u32::bit_width`, stable since Rust 1.97.0 | `msrv`, and `clippy` too | 36s / 39s |

Each failing job printed the "reproduce locally" command of
[contracts/ci.md](./contracts/ci.md) — the `msrv` one interpolating the real minimum,
`rustup run 1.96 cargo build` — and every gating job unrelated to the break stayed green.
`ci-required` refused all four. **Submitted 12:03:33Z, every verdict in by 12:05:34Z**: under two
minutes against SC-034's thirty.

The one surprise: `cargo clippy` reads `rust-version` from `Cargo.toml` and denies
`clippy::incompatible_msrv`, so the minimum-toolchain break is caught on stable as well as on 1.96.
Two jobs, one cause, and each still names its own command.

## 7. A release, end to end (FR-105–FR-111, SC-037, SC-038)

Trigger the release workflow with a version, then check as an outsider would:

```bash
gh release download v<version>
sha256sum -c SHA256SUMS
./hypr-swap-<version>-x86_64 --version      # equals the tag and the changelog heading
```

Then install each package in a clean container of its family's oldest and current release:

```bash
docker run --rm -v "$PWD:/pkg" ubuntu:22.04 bash -c \
  'apt-get update -qq && apt-get install -y /pkg/hypr-swap_*_amd64.deb && hypr-swap --version'
```

**Expected**: installs with no manual dependency work, `--version` agrees, `LICENSE` is on disk at
the path in [contracts/packaging.md](./contracts/packaging.md). **SC-039**.

Then the refusals — a release must fail rather than publish something unreproducible:

| Attempt | Expected |
|---|---|
| Tag already exists and is published | refused before any commit |
| Working tree dirty | refused |
| Gating checks not green on the commit | refused |
| Re-run after a half-finished draft | resumes from the tag, replaces the draft's assets, produces the same files |

## Release checklist (the inspection tier)

Every requirement whose tier is **Inspection** in [plan.md](./plan.md), in one list, to be walked
before the first public release and re-walked at each subsequent one:

- [ ] FR-066a — history scanned (`gitleaks detect --log-opts=--all`), outcome recorded in
      `history-review.md`, before the repository is made public
- [ ] FR-067, FR-071 — README answers the six questions in order; scope boundaries and the
      no-network / no-telemetry statement are present
- [ ] FR-070 — both overlay screenshots are current
- [ ] FR-072, FR-073, FR-075 — `DEVELOPMENT.md` gets a newcomer to a running test suite, conveys
      the pure/shell seam, and states which tier needs a compositor
- [ ] FR-084b — the published tree still carries `specs/`, `.specify/`, `.claude/`, `CLAUDE.md`
- [ ] FR-091 — branch protection requires `ci-required` and nothing else
- [ ] FR-092 — the tier table is published and covers every requirement of 001, 002 and 003
- [ ] FR-101a — [contracts/versioning.md](./contracts/versioning.md)'s breaking-change definition
      still names the whole contract surface
- [ ] FR-117 — any key renamed or removed this release keeps its old name recognised, and
      `tests/fixtures/config-previous-release.toml` has been refreshed to this release's contract
- [ ] FR-094, FR-095, FR-100, FR-121 — `CONTRIBUTING.md` states the rules, the spec-driven flow,
      what review looks for, the best-effort expectation and the dependency bar
- [ ] FR-096 — `CODE_OF_CONDUCT.md` with a reporting address
- [ ] FR-097, FR-098 — the issue forms require the environment block and ask for the goal
- [ ] FR-099 — the pull-request template's checklist covers tests, docs, changelog, specs
- [ ] FR-102 — the changelog entry is written for users, and names anything that broke
- [ ] FR-109a — the distribution matrix in [contracts/packaging.md](./contracts/packaging.md) still
      names releases that are actually supported
- [ ] FR-111 — the release notes carry the packager block
- [ ] FR-115 — the troubleshooting page still names where the compositor collects the output
- [ ] FR-119, FR-120 — `SECURITY.md`'s channel works and its supported-version list is current
