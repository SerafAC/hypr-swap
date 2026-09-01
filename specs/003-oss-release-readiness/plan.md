# Implementation Plan: Open-Source Release Readiness

**Branch**: `003-oss-release-readiness` | **Date**: 2026-08-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-oss-release-readiness/spec.md`

## Summary

The switcher is delivered. This feature builds everything around it that a stranger needs in order
to find, trust, install, configure, run, report on and contribute to the project, and everything
the maintainer needs to release it repeatably. It is overwhelmingly a *repository* feature: text,
workflows, packaging recipes and one container image. Only a small, sharply bounded set of changes
touches the running program.

**What changes in the program** (FR-112–FR-118, roughly 200 lines): two lifecycle records through
the existing `diag.rs` — started, with the version; stopping, with the cause — on every exit path
including the ones that never finish start-up; a `--environment` flag printing the facts the bug
report form asks for; a start-up check of the compositor's reported version against a supported
range; and a build-time version string that identifies a non-release build by its exact source.
No new dependency, no new module of decision logic beyond one pure version parse, no verbosity
setting, and no change to the diagnostic format or notification policy.

**What changes around the program**: the licence text and the third-party account; a README aimed
at the end user alone and a `DEVELOPMENT.md` aimed at the contributor; `docs/` turned into the
published documentation — plain Markdown split into a user half and a developer half, generated
by docmd from three files at the repository root — whose configuration reference *includes*
the 002 contract pages rather than restating them, so the existing catalogue unit test keeps the
published reference and the program's actual behaviour from ever diverging; a GitHub Actions
workflow whose gating checks are build, unit tests, clippy, fmt, the minimum toolchain, the
documentation build and the E2E suite; a container image that carries a compositor so the E2E tier
runs in automation and reproduces locally; a hand-written changelog; and a release workflow that
takes a version, refuses to run on a tree that is not ready, and publishes a source archive, a
binary, a `.deb`, an `.rpm` and their integrity values.

The one genuinely hard problem — running an overlay's E2E suite where there is no display — was
settled by experiment rather than assumption ([research.md](./research.md) R29): a plain container
cannot start Hyprland at all (no seat, no allocator, no headless-only mode), the same container
runs the suite perfectly against any real Wayland session, and automation therefore has to supply
a virtual GPU and a seat — which is what upstream Hyprland's own CI does for the same reason. The
harness itself does not change.

## Technical Context

**Language/Version**: Rust 1.96 (edition 2024) — unchanged, and now *enforced*: `rust-version` in
`Cargo.toml` is what the minimum-toolchain CI job builds against (FR-087).

**Primary Dependencies**: unchanged. This feature adds **no runtime dependency**. It adds
development and release tooling, none of which is compiled into the binary: `cargo-deb`,
`cargo-generate-rpm`, `cargo-deny`, `gitleaks` (once, for FR-066a), and — for the documentation
site alone — a Node.js toolchain (≥ 22) with **docmd**, whose dependencies live in the root
`package.json`, are managed with **pnpm** (R31a) and are installed by nothing but the `docs` jobs
and people building the site locally — editing the prose needs none of it (R31, R48, and one entry
in Complexity Tracking).

**Storage**: N/A.

**Testing**: `cargo test --lib` for the new pure logic (`compose_version`, the compositor version
parse and range comparison, the
`deny.toml` acceptance-expiry check, the extended settings-catalogue walk); `cargo test --test
'e2e_*'` for the lifecycle records, the environment report, the compositor-version diagnostic and
configuration compatibility. The E2E tier now also runs in automation, inside the image of
[research.md](./research.md) R29–R30.

**Target Platform**: unchanged for the program — Linux / Wayland / Hyprland ≥ 0.55. New for the
*artefacts*: `x86_64` only; the Debian package built on the oldest supported Ubuntu LTS, the RPM
on the oldest supported Fedora, the Arch recipe building from source ([contracts/packaging.md](./contracts/packaging.md)).

**Performance Goals**: unchanged; nothing here is on the overlay's paint path. Two budgets are
new and belong to automation rather than to the program: a verdict on a proposed change within
30 minutes (SC-033) and a contributor reproducing an E2E failure locally within 15 minutes
(SC-035).

**Constraints**: The diagnostic levels, format and notification policy are frozen (FR-114). No
verbosity or log-level setting. No service-manager unit. Nothing may be published to a language
package registry. The repository is published whole, history included, after the FR-066a review.
Documented behaviour cannot be restated in two places that can disagree (FR-084).

**Scale/Scope**: ~200 lines of Rust across four existing files; ~2500 lines of Markdown across the
README, `DEVELOPMENT.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `THIRD-PARTY.md`,
`CODE_OF_CONDUCT.md` and twelve site pages; five workflow files; one Dockerfile; three packaging
recipes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Gates derived from `.specify/memory/constitution.md` (v1.0.0).

**Initial evaluation (pre-Phase 0)**: PASS with one item flagged for research — whether the E2E
tier can run in automation at all (FR-088). The spec names it the feature's largest unknown, and
Principle V's standing depends on the answer: if the tier could not run in automation, either the
requirement or the constitution's E2E principle would have had to give. It is resolved in
[research.md](./research.md) R29 and leaves one entry in Complexity Tracking. A second entry
was added after the fact, when the documentation framework brought a Node toolchain with it
(R31).

**Post-design re-evaluation (post-Phase 1)**:

- [x] **I. KISS**: PASS. The feature adds no runtime dependency and no new module. The program's
      four changes are a new `Condition` pair on an existing policy table, a flag on an existing
      argument parser, one pure function, and a `build.rs` addition beside the one already there.
      Around the program, every tool chosen is the smallest thing that answers a requirement:
      `::include[]` instead of a generator (R32), `cargo-deb`/`cargo-generate-rpm` because they read `Cargo.toml` instead of adding a
      second metadata file (R33), and one `cargo-deny` answering both the advisory watch and the
      packager's licence question (R38). The container image is in Complexity Tracking, because it
      is genuinely new machinery.
- [x] **II. YAGNI**: PASS. Every artefact traces to a requirement; the trace is the table in
      [contracts/README.md](./contracts/README.md) and the tier column below. Four things were
      deliberately *not* built: a configuration migration framework (R43 — the versioning policy
      plus the existing unknown-key diagnostic is the whole requirement), a logging crate and a
      verbosity setting (R40, explicitly out of scope), artefact signing (R36 — FR-108 asks for
      integrity, not provenance), and versioned documentation snapshots (FR-078a forbids them).
- [x] **III. DRY**: PASS, and the feature removes duplication rather than adding it. The
      configuration and style reference is *included* from the 002 contracts into the site, so one
      file is simultaneously the contract, the tested catalogue and the published page (R32). The
      version exists once, in `Cargo.toml`, and is read by the binary, both packaging tools and
      the release workflow. The bind lines still come from `ui/shortcuts.rs`. The supported
      compositor range becomes a single constant that the diagnostic, the README and the site's
      requirements page all derive from. FR-084's "exactly one authoritative answer" is enforced
      by the document map in [contracts/documentation.md](./contracts/documentation.md).
- [x] **IV. Unit tests**: PASS, with the same documented shell exemption 001 and 002 carry. The
      new pure logic — the compositor version parse and range comparison, the extended settings
      catalogue walk, the `deny.toml` acceptance-expiry check — is unit-tested in-module. The
      lifecycle records sit in `main.rs`, which stays in the shell exemption and is covered E2E.
- [x] **V. E2E coverage**: PASS. Every requirement in this feature has a named verification tier
      and none is "unknown" — which is itself FR-092 and SC-036. The program's four changes are
      E2E-covered through the real interfaces (stderr, stdout, exit codes). The requirements that
      are documents or workflows are verified by the checks that run them or, where only a human
      can judge, by a named checklist item — the table below is the published statement.

### Verification tier for every requirement

Tiers: **Unit** (`cargo test --lib`) · **E2E** (`cargo test --test 'e2e_*'`) · **CI** (a check in
the pull-request workflow) · **Release** (a precondition or step of the release workflow) ·
**Inspection** (a named item on the release checklist in [quickstart.md](./quickstart.md)).

| Requirement | Tier | Verified by |
|---|---|---|
| FR-062 licence text present, matches metadata | CI | `licence-files` check: `LICENSE` exists, holder/year present, `Cargo.toml` `license` agrees |
| FR-063 third-party components attributable | CI | same check: every path in `protocols/`, `assets/` appears in `THIRD-PARTY.md` |
| FR-064 dependency licence position | CI | the gating `licenses` job: `cargo deny check licenses` |
| FR-065 package metadata complete | CI | `licence-files` check asserts description, licence, repository, documentation, keywords |
| FR-066 packages ship the licence | Release | `.deb`/`.rpm` contents asserted after build |
| FR-066a history reviewed before publication | Inspection | `history-review.md` recorded (R44) |
| FR-067 README answers six questions in order | Inspection | README review item |
| FR-068 no development instructions in README | CI | `docs-map` check: README carries no `cargo test`/`cargo clippy`/architecture headings |
| FR-069 requirements section complete | CI | `docs-map` check: compositor range matches `SUPPORTED_HYPRLAND`, toolchain matches `rust-version` |
| FR-070 overlay shown in both presentations | Inspection | both images present and current (R47) |
| FR-071 scope and privacy statement | Inspection | README review item |
| FR-072 development document covers setup/tests/run/architecture | Inspection | `DEVELOPMENT.md` review item |
| FR-073 architecture conveys the pure/shell seam | Inspection | same |
| FR-074 every top-level directory and module described | CI | `docs-map` check: every `src/**/*.rs` and top-level directory named in `DEVELOPMENT.md` |
| FR-075 per-tier requirements, container route stated | Inspection | `DEVELOPMENT.md` review item |
| FR-076 site builds | CI | `docs` job: `pnpm install --frozen-lockfile && pnpm build && pnpm validate` |
| FR-077 two navigable sections | CI | the `navigation` in `docmd.config.mjs`, asserted by the `docs-map` check |
| FR-078 auto-published, failure reported | CI | the `docs` workflow itself (R46) |
| FR-078a one version, states its release | CI | `docs-map` check: front page names a released version |
| FR-079 complete configuration reference | Unit + CI | catalogue walk (extended, R32); include resolves at build |
| FR-080 theming account sufficient | Unit | catalogue walk covers every colour, font and geometry value |
| FR-081 end-user section covers install/binds/keys/icons/diagnostics/troubleshooting | CI | `docs-map` check: required page set and troubleshooting entries present, each naming a real `Condition` |
| FR-082 developer section complete | CI | `docs-map` check: required page set |
| FR-083 reference verified against behaviour | Unit | the catalogue walk — the check that cannot be bypassed |
| FR-084 one authoritative answer per question | CI | `docs-map` check against [contracts/documentation.md](./contracts/documentation.md) |
| FR-084a specs stay authoritative | CI | same check: developer pages link to `specs/`, do not restate |
| FR-084b development record retained | Inspection | published tree contains `specs/`, `.specify/`, `.claude/`, `CLAUDE.md` |
| FR-085 checks run unprompted, one verdict | CI | the `ci-required` aggregate job (R39) |
| FR-086 build, unit, lint, format | CI | those four jobs |
| FR-087 minimum toolchain built | CI | `msrv` job pinned from `rust-version` |
| FR-088 E2E runs against a supplied compositor | CI | the `e2e` job in the image of R29/R30, called from `ci.yml` so `ci-required` aggregates it |
| FR-089 image defined in-repo and usable locally | Inspection + E2E | `docker/e2e/Dockerfile`; the local run is quickstart scenario 5 |
| FR-090 failures name the reproducing command | CI | each job's failure step; asserted by the `ci-required` job's summary |
| FR-091 gating distinguishable from informational | Inspection | branch protection requires only `ci-required`; [contracts/ci.md](./contracts/ci.md) lists both sets |
| FR-092 published coverage of every requirement | Inspection | this table, `::include[]`d into the developer section, beside the derived rows for 001 and 002 |
| FR-093 advisories surfaced, bounded acceptance | CI + Unit | `cargo deny check advisories` (informational); the expiry test is gating; the site's npm tree watched by Dependabot (R38) |
| FR-094 contribution guidance | Inspection | `CONTRIBUTING.md` review item |
| FR-095 spec-driven expectations stated | Inspection | same |
| FR-096 conduct expectations | Inspection | `CODE_OF_CONDUCT.md` present with a reporting address |
| FR-097 bug form requires environment facts | Inspection | issue form fields marked required |
| FR-098 feature form asks for the goal, shows scope | Inspection | issue form review item |
| FR-099 change checklist | Inspection | pull-request template |
| FR-100 expected response stated | Inspection | `CONTRIBUTING.md` review item |
| FR-101 semver from 1.0.0 | Release | version input validated; first release is 1.0.0 |
| FR-101a breaking change defined over the contract surface | Inspection | [contracts/versioning.md](./contracts/versioning.md) |
| FR-102 hand-written user-facing changelog | Inspection | changelog review item |
| FR-102a `[Unreleased]` section maintained | CI | `changelog` check: the section exists and is non-empty when the change touches `src/` |
| FR-103 tag, runtime version and entry agree | Release + E2E | workflow asserts all three; `e2e_version_matches_metadata` asserts the runtime half |
| FR-104 non-release build identifies its source | Unit + E2E | `compose_version` unit-tested over its inputs; `e2e_version_reports_build` |
| FR-105 one triggered workflow, no manual steps | Release | the workflow (R36) |
| FR-106 artefact set published | Release | upload step asserts the five files |
| FR-107 Arch recipe in step | Release | recipe regenerated from the published artefacts |
| FR-108 integrity values published | Release | `SHA256SUMS` generated and verified after upload |
| FR-109 conventional locations, declared dependencies, clean install | Release | install-and-run smoke test in a clean container of each family |
| FR-109a built on oldest supported, matrix published | Release | build containers pinned; smoke test on oldest and current |
| FR-110 refuses on a dirty tree, existing tag, or red checks; re-runnable | Release | preconditions at the top of the workflow |
| FR-111 packager has what they need | Inspection | release notes carry dependencies, build steps and install map |
| FR-112 start record with version | E2E | `e2e_records_start_with_version` |
| FR-113 stop record with cause | E2E | `e2e_records_stop_on_signal`, `e2e_records_stop_on_fatal_startup` |
| FR-114 existing diagnostics unchanged | Unit + E2E | `Condition` policy tests; existing E2E suite unchanged and still passing |
| FR-115 where the compositor collects output | Inspection | troubleshooting page review item |
| FR-116 environment report on demand | E2E | `e2e_environment_report` |
| FR-117 old configuration not reinterpreted | E2E | `e2e_config_from_previous_release` |
| FR-118 compositor version mismatch named | Unit + E2E | version parse tests; `e2e_unsupported_compositor_version` |
| FR-119 private security channel | Inspection | `SECURITY.md` review item |
| FR-120 supported versions published | Inspection | same |
| FR-121 dependency policy stated | Inspection | `CONTRIBUTING.md` review item |

### E2E coverage mapping

| E2E test | Drives | Covers |
|---|---|---|
| `e2e_records_start_with_version` | daemon started under the harness, stderr read | FR-112, US7-AS1, SC-042 |
| `e2e_records_stop_on_signal` | `SIGTERM` to a running daemon | FR-113, US7-AS2 |
| `e2e_records_stop_on_fatal_startup` | daemon started with no compositor reachable | FR-113 (pre-start-up exit), spec edge case |
| `e2e_existing_diagnostics_unchanged` | an invalid configuration value, as 001/002 already exercise | FR-114, US7-AS3 |
| `e2e_environment_report` | `--environment` against the nested instance | FR-116, US7-AS5 |
| `e2e_version_reports_build` | `--version` from a non-tag build | FR-104, US1-AS6 |
| `e2e_version_matches_metadata` | `--version` compared with `Cargo.toml` | FR-103 |
| `e2e_config_from_previous_release` | `tests/fixtures/config-previous-release.toml`, refreshed each release | FR-117, US7-AS6, SC-043 |
| `e2e_unsupported_compositor_version` | env-gated version override below the minimum | FR-118, US1-AS5 |

Requirements deliberately **not** E2E-covered are every requirement whose subject is a document, a
workflow or a package rather than the running program; each has a tier in the table above, which
is what FR-092 and SC-036 ask for. Three of the spec's success criteria are human measurements
rather than checks — SC-026 and SC-032 (time-to-first-overlay and time-to-first-test for a
newcomer) and SC-031 (a user assembles an appearance from the documentation alone) — and are
measured the way 001 measures SC-004: by walking the published path once, recorded in
[quickstart.md](./quickstart.md).

## Project Structure

### Documentation (this feature)

```text
specs/003-oss-release-readiness/
├── plan.md                  # This file
├── research.md              # Phase 0 output — R29–R47
├── data-model.md            # Phase 1 output
├── quickstart.md            # Phase 1 output — validation scenarios and the release checklist
├── contracts/               # Phase 1 output
│   ├── README.md            # Contract index + requirement trace
│   ├── cli.md               # `--environment`, the version string (delta to 001's cli.md)
│   ├── diagnostics.md       # The three new conditions (delta to 001's diagnostics.md)
│   ├── versioning.md        # Semver policy: the contract surface, what makes a major
│   ├── release.md           # The release workflow: inputs, preconditions, steps, artefacts
│   ├── packaging.md         # Package layout, dependencies, the verified distribution matrix
│   ├── ci.md                # Checks, gating vs informational, local reproduction, the image
│   └── documentation.md     # The document map: which document answers which question
├── checklists/
│   └── requirements.md      # Pre-existing
├── history-review.md        # FR-066a outcome, recorded before publication
└── tasks.md                 # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

New and changed files only; everything else in the 001/002 tree is untouched.

```text
LICENSE                      # NEW: MIT text, holder and year (FR-062)
THIRD-PARTY.md               # NEW: vendored protocol + placeholder artwork (FR-063)
CHANGELOG.md                 # NEW: Keep a Changelog, [Unreleased] section (FR-102)
CONTRIBUTING.md              # NEW: rules, spec-driven workflow, what review looks for (FR-094)
CODE_OF_CONDUCT.md           # NEW (FR-096)
SECURITY.md                  # NEW: private channel, supported versions (FR-119, FR-120)
DEVELOPMENT.md               # NEW: setup, tests, running, architecture, tree (FR-072–FR-075)
README.md                    # REWRITTEN to the six end-user questions only (FR-067, FR-068)
deny.toml                    # NEW: advisory and licence policy (R38)
build.rs                     # CHANGED: + the raw git-describe fact, no decision (FR-104, R37)
Cargo.toml                   # CHANGED: + metadata.deb, metadata.generate-rpm, keywords,
                             #   repository, documentation (FR-065, R33)

src/
├── lib.rs                   # CHANGED: + pure compose_version() and version(); SUPPORTED_HYPRLAND
├── diag.rs                  # CHANGED: + Started, Stopping, CompositorVersionUnsupported
├── model.rs                 # CHANGED: + the j/version response and the pure version comparison
└── main.rs                  # CHANGED: lifecycle records on every exit path; --environment;
                             #   the start-up version check

tests/
└── e2e_lifecycle.rs         # NEW: the nine tests in the mapping above

docs/                        # NOW the published documentation (FR-076, R48) — plain Markdown,
├── index.md                 #   and nothing but. What it is; which release this documents (FR-078a)
├── user/                    # FR-077's first section
│   ├── install.md           # Every published channel (FR-081)
│   ├── binds.md             # moved here; `shortcuts.rs` include_str!s it (FR-022b)
│   ├── configuration.md     # ::include of 002 contracts/config.md (FR-079, R32)
│   ├── styling.md           # ::include of 002 contracts/style-values.md (FR-080)
│   ├── icons.md             # Program icons and icon sets (FR-081)
│   └── troubleshooting.md   # Diagnostics, and the five named failures (FR-081, FR-115)
├── dev/                     # FR-077's second section
│   ├── architecture.md      # The pure/shell seam, in full (FR-082)
│   ├── workflow.md          # Spec-driven flow; links to specs/ (FR-082, FR-084a)
│   ├── testing.md           # Every tier, the harness, the image (FR-082)
│   ├── verification.md      # The tier table above (FR-092)
│   └── releasing.md         # The release procedure (FR-082)
└── assets/                  # NEW: the two overlay screenshots (FR-070, R47)

docmd.config.mjs             # NEW: the site — url, navigation, theme, the include plugin (R31, R48)
package.json                 # NEW: @docmd/core, pinned exactly; dev/build/validate scripts
pnpm-lock.yaml               # NEW: committed; the `docs` jobs run `pnpm install --frozen-lockfile`
pnpm-workspace.yaml          # NEW: allowBuilds — without it every install fails (R46)

docker/e2e/Dockerfile        # NEW: the test environment image (FR-089, R29, R30)
scripts/checks.sh            # NEW: licence-files, docs-map, changelog — runnable locally
scripts/docmd-include.mjs    # NEW: `::include[]`, the whole of FR-084's mechanism (R32)
packaging/aur/PKGBUILD       # NEW: the Arch recipe (FR-107)

.github/
├── workflows/
│   ├── ci.yml               # build, unit, clippy, fmt, msrv, docs, checks → ci-required
│   ├── e2e.yml              # the E2E tier in the image, on: workflow_call (FR-088)
│   ├── docs.yml             # build and deploy the site (FR-078)
│   ├── advisories.yml       # cargo-deny, informational (FR-093)
│   └── release.yml          # workflow_dispatch, version input (FR-105)
├── dependabot.yml           # NEW: cargo + npm ecosystems (FR-093, R38)
├── ISSUE_TEMPLATE/
│   ├── bug.yml              # required environment fields (FR-097)
│   ├── feature.yml          # asks for the goal, shows the scope (FR-098)
│   └── config.yml           # points security reports at SECURITY.md
└── pull_request_template.md # tests, docs, changelog, specs (FR-099)
```

**Structure Decision**: The single-binary layout is unchanged, and no source module is added — a
feature about releasing the program should not restructure it. Two structural choices are worth
naming. First, **`docs/` becomes the published documentation itself**, plain Markdown organised
into `user/` and `dev/`, with nothing else in it: the generator reads that tree from four files at
the repository root and owns no directory inside it (R48). The tree is meant to
be read either way — open the folder in an editor, or open the site — and relative links of the
form `[binds](./user/binds.md)` work in both, which is the property that makes the two equivalent.

One page keeps a compile-time tie to the code, deliberately. `ui/shortcuts.rs` does an
`include_str!("../../docs/user/binds.md")` and asserts the page contains every bind line
`Shortcut::suggested_bind` generates, so changing a combination in the code fails the build until
the page agrees (FR-022b, FR-033, Principle III). The tie was **narrowed rather than cut**: it once
also asserted three exact sentences, which froze editorial wording, and those are gone. What
remains checks agreement between the documentation and the code — the only thing a test here can
usefully hold — and leaves the prose around it free. The cost is that this one page cannot be
renamed without a one-line change in `shortcuts.rs`, which `cargo build` reports immediately.
The Node artefacts are `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml` and
`docmd.config.mjs`, which a Rust contributor never opens. Second, the
checks that verify documents (`docs-map`,
`licence-files`, `changelog`) are **shell steps in the workflow**, not Rust code, except where a
check needs to compare a document against the program's own values — the settings catalogue and
the advisory expiry — which are unit tests, because that is where the values live and where a
contributor will see the failure first.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| A container image carrying a compositor, plus a virtual GPU in automation, to run the E2E tier | FR-088 requires the E2E suite to run against a compositor supplied by automation, and Principle V makes that tier the evidence for this project's headline behaviour. Measurement ([research.md](./research.md) R29) shows there is no cheaper way: Hyprland 0.56 has no headless-only mode, refuses to start without a seat, and its Wayland backend needs a dmabuf allocator no GPU-less parent can supply. Upstream Hyprland reached the same conclusion and runs its own compositor tests in a QEMU VM with `virtio-gpu`. | **A plain container** — measured, does not work, with the exact failures recorded in R29. **A newer wlroots parent** — fixes the xdg-shell version mismatch but not the allocator, and buys a dependency on someone else's release cadence for half a fix. **A mock compositor** — would replace the only evidence the project has that it works against a real one, which is 001 R14's rejected trade in a new costume. **Marking E2E informational** — fails FR-088 outright and makes the merge gate blind to the behaviour users actually see. The cost is bounded: one Dockerfile, one workflow job, and a harness that does not change at all. |
| A second document tree (`docs/`) beside `specs/` | FR-076–FR-082 require a published site for users and developers, and FR-084a keeps `specs/` authoritative for requirements and contracts. Two trees with two audiences and one rule about which answers what ([contracts/documentation.md](./contracts/documentation.md)). | **Publishing `specs/` as the site** — the specification is a record of decisions, not a user manual; a user looking for "how do I set a colour" should not land in an FR list. **Keeping only the README** — cannot hold the complete configuration and theming reference FR-079/FR-080 demand without becoming the thing FR-067 forbids. The duplication risk is the real cost, and it is answered structurally: the site *includes* the contract pages rather than restating them (R32), so the two trees share bytes rather than agreeing by discipline. |
| A Node.js toolchain and a pnpm dependency tree, for the documentation site only (R31, R31a, R48) | FR-076 requires a published site, and no Rust tool answers it well. docmd is the smallest thing that does: 60 packages, one command, plain Markdown in and static HTML out. It buys working search — which the site otherwise has none of — and a link checker, and it leaves the include that FR-084's "one authoritative answer" is built on as a hundred-line local plugin rather than a framework feature (R32). | **mdBook**, which costs no second toolchain — rejected in R31: no search, and only line-range includes, which silently follow the wrong lines when a contract is edited. **MkDocs Material** — the same second-toolchain cost in Python, plus plugins. **A React documentation framework** — a web application to publish twelve Markdown pages. The cost is contained rather than argued away: four files at the root, no Rust job installs it, a contributor who never builds the site never sees it, and the one thing it genuinely breaks — `cargo-deny` cannot see npm advisories — is answered by a separate Dependabot ecosystem (R38, FR-093). |
| Shell modules unit-test-exempt (Principle IV) | The lifecycle records, the `--environment` flag and the start-up version check live in `main.rs`, the deliberately logic-free shell described in `CLAUDE.md`. Their behaviour is process lifecycle and stderr, which a unit test can only assert by re-implementing the process. The one decision rule this feature adds — parsing a compositor version and comparing it to a range — is pure, in `model.rs`, and **is** unit-tested. | **Unit-testing the shell** would mean mocking a compositor and a signal disposition to assert against the mock. **Extracting a lifecycle module** to make three `diag::report` calls testable would add an abstraction to test a call site, against Principle I. This is the same deviation 001 and 002 recorded, restated here because the constitution requires it in *this* feature's table. |
| An env-gated compositor-version override for the FR-118 E2E test | The nested compositor is whatever version the machine has, so the "below the minimum" path cannot be reached by driving the real interface. Without the hook, FR-118 would have unit coverage only. | **Pinning an old Hyprland in the image** — a second compositor build to maintain, and it would drift out of support exactly when the test mattered. **Leaving FR-118 unit-only** — acceptable, and rejected because the requirement is about what the daemon *reports at start-up*, which is a stderr behaviour. The hook follows the existing precedent: `hypr/ipc.rs`'s fault injection (001) and `diag.rs`'s paint records (002), both env-gated and inert in normal operation. |
