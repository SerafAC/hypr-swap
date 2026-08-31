# Phase 0 Research: Open-Source Release Readiness

**Feature**: `003-oss-release-readiness` | **Date**: 2026-08-30

Decision numbering continues features 001 (R1–R17) and 002 (R18–R28), so a citation in a code
comment or a document is unambiguous without naming the feature. This feature holds **R29–R47**.

Findings marked **[verified]** were established by running the thing on this machine on
2026-08-30 (Hyprland 0.56.2, Rust 1.96.0, Docker 29.6.1); the commands are reproducible from
[quickstart.md](./quickstart.md). Everything else is a decision taken on documented behaviour.

---

## R29: Running the E2E tier where there is no Wayland session (FR-088, FR-089)

**The question.** The harness (`tests/e2e/harness.rs`) starts a nested Hyprland as an ordinary
Wayland client of the developer's session. Automation has no session. The spec calls this the
feature's largest technical unknown, so it was resolved by experiment rather than by reading.

**What was measured.**

1. **Hyprland cannot start on its own inside a plain container.** Both of its backends refuse,
   for different reasons — **[verified]**, from the compositor's own log:
   - the DRM backend: `libseat: failed to open a seat` → `Failed to open a session` →
     `DRM Backend failed`. There is no seat in a container and no logind session to join.
   - the Wayland backend, given a parent: `Cannot open backend: no allocator available`, followed
     by `m_pAqBackend couldn't start!`. Aquamarine wants a dmabuf allocator from the parent.
   There is no headless-only escape hatch: `strings` over the 0.56.2 binary shows **no**
   `HYPRLAND_HEADLESS_ONLY` (or any equivalent) env var, and `--help` lists no headless flag —
   **[verified]**. The `HEADLESS-n` outputs it can create are additional outputs on a backend that
   is already running, not a backend of their own.
2. **A wlroots parent is not new enough.** With `sway` (1.11 / wlroots 0.19) as the parent under
   `WLR_BACKENDS=headless`, the nested Hyprland's connection dies on
   `invalid version for global xdg_wm_base (13): expected at most 5, got 6` — aquamarine binds
   xdg-shell v6, sway offers v5 — and the allocator failure follows on the same broken
   connection — **[verified]**.
3. **The same container works perfectly against a real session.** The image below, run with the
   host's `$XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY` passed in and one render node
   (`--device /dev/dri/renderD128`), starts a nested Hyprland, reports `Monitor WAYLAND-1`,
   accepts `hyprctl output create headless` and shows `HEADLESS-1`, and maps a spawned `foot`
   client (`"class": "foot"`, `"mapped": true`) — **[verified]**. That is precisely the sequence
   every E2E test performs.

**Decision.** The test environment is **one container image, two ways of giving it a session**.

- **A contributor with any Wayland session** (the common case, and the one FR-089 is about) runs
  the published image against their own session. Verified working, and it is the same image
  automation uses, so an automation failure reproduces locally — SC-035.
- **Automation supplies a virtual GPU and a seat**, and the image starts its *own* parent Hyprland
  on the DRM backend before running the suite; the harness then nests inside that parent exactly
  as it nests inside a developer's session, unchanged. Two routes exist for the virtual GPU, and
  the first task of the CI phase is a spike that settles which one the runner actually supports:
  1. **`vkms` on the runner** — `sudo modprobe vkms` publishes a synthetic DRM device, the
     container runs with `--device /dev/dri` and its own `seatd`. Fewest moving parts, fastest.
  2. **A QEMU virtual machine with `virtio-gpu`**, KVM-accelerated when `/dev/kvm` is present.
     This is what **upstream Hyprland's own CI does** for its compositor tests — `nix/tests/default.nix`
     runs a NixOS VM with `qemu.options = [ "-vga none -device virtio-gpu-pci" ]` from an
     `ubuntu-latest` runner — so it is known to work on GitHub's hosted runners.

**Rationale.** The harness is the project's most valuable test asset and rewriting it would be a
larger change than this whole feature. Every measurement above says the harness does not need to
change at all: it needs a parent, and the difference between a developer's machine and a runner is
only *where the parent comes from*. Ruling the plain-container route out by experiment rather than
by assumption also means the fallback is chosen on evidence — including the evidence that the
compositor's own maintainers reached for a VM for the same reason.

**Alternatives considered.**

- **A headless-only Hyprland in the container** — the obvious first idea, and it does not exist in
  0.56 (finding 1). Rejected on evidence.
- **A newer wlroots parent** — would fix the xdg-shell version, but not the allocator: no GPU node
  in the container means no dmabuf, and a software parent has nothing to hand the nested
  compositor. Not worth chasing a moving compositor version for half a fix.
- **Dropping E2E from automation and running it only locally** — fails FR-088 and the
  constitution's Principle V, and it is exactly the tier that covers this project's headline
  behaviour. Rejected.
- **A self-hosted runner on the maintainer's machine** — makes outside contribution depend on one
  person's desktop being awake, and gives a contributor no way to reproduce. Rejected.
- **Rewriting the E2E tier against a mock compositor** — would delete the only evidence the
  project has that it works against the real one. Rejected (001 R14 already refused the analogous
  trade).

## R30: What the test-environment image contains (FR-089)

**Decision.** An Arch base (`archlinux:latest`), pinned by digest, carrying `hyprland`, `foot`,
`seatd`, `mesa`, the cairo/pango development libraries and a `rustup` toolchain matching
`rust-version`; one non-root user; and an entry point that runs `cargo test --test 'e2e_*'`.

**Rationale.** Arch is the only family that ships a current Hyprland, which is what the E2E tier
needs; the distribution the *packages* target is a separate question (R33) and deliberately a
different image. Two container facts were learned the hard way and both belong in the Dockerfile
— **[verified]**:

- **Hyprland refuses to run as root** (`Hyprland was launched with superuser privileges…`), and CI
  containers run as root by default, so the image must create and drop to an unprivileged user.
  The `--i-am-really-stupid` escape hatch is not used: running the compositor as root in the same
  container as a `cargo build` is worse than adding a `useradd` line.
- **Arch's `sway` and `Hyprland` carry `cap_sys_nice=ep` file capabilities**, and executing a file
  with capabilities fails with `Operation not permitted` under Docker's default bounding set. The
  image runs `setcap -r` on them; the compositor only loses a scheduling nicety it already warns
  about (`Failed to change process scheduling strategy`).

**Alternatives considered.** A Debian/Ubuntu base — no current Hyprland, so it would mean building
the compositor from source in the image, for a tier that does not care which distribution it runs
on. A Nix flake, as upstream uses — a second package manager and a second build system for the
project's contributors to learn, against Principle I.

## R31: The documentation framework (FR-076)

**Decision.** **Fumadocs** — `fumadocs-ui` / `fumadocs-mdx` on Next.js — scaffolded from the
upstream `+next+fuma-docs-mdx+static` template, static-exported (`output: 'export'`) and published
to GitHub Pages. **`docs/` is the documentation root and holds plain Markdown**; the framework is
tucked into `docs/.fumadocs/`, which is the Next.js project. Pages live in a `user/` and a `dev/`
folder that are FR-077's two sections. The shape is R48's decision; this one is only the framework.

**Rationale.** This decision was **revised after the fact**: R31 originally chose mdBook, on the
grounds that it kept the project to a single language toolchain, and the project owner
subsequently directed the change to Fumadocs. The rationale below is what survives re-examination
rather than a retrofit — the cost the original decision was avoiding is real, is still real, and is
recorded in Complexity Tracking rather than argued away.

What the change buys:

- **Search that exists.** The original R31 conceded that real search was MkDocs Material's
  advantage over mdBook and accepted going without. Fumadocs' Orama integration builds a search
  index at build time and queries it in the browser, so a static site keeps working search with no
  server and no third-party account — the `+static` template wires this up as a supported
  configuration, not a workaround.
- **A stronger include.** FR-084's "exactly one authoritative answer" rests entirely on the site
  including the `specs/` contracts rather than restating them. Fumadocs' include extracts by
  **heading or `<section id>`**, where mdBook's `{{#include}}` extracts by line range or anchor
  comment — a line-range include silently follows the wrong lines when the included file is
  edited, and this project edits its contracts. It also registers each included file as a build
  dependency, which removes the need for mdBook's `extra-watch-dirs` bookkeeping (R32).
- **Components where prose is not enough.** The install, troubleshooting and configuration pages
  are the ones a reader arrives at in trouble; callouts, tabs and cards are worth having there, and
  in mdBook each would be a preprocessor.

What the change costs, stated plainly:

- **A second language toolchain in a Rust repository** — Node.js ≥ 22, pnpm, a `package.json` and
  a `pnpm-lock.yaml`, all confined to `docs/.fumadocs/` — which is precisely the objection the
  original R31 raised against
  Docusaurus. It is accepted deliberately, not overlooked. It is confined to `docs/.fumadocs/`: a
  contributor who touches only Rust never installs Node, and no CI job outside `docs` needs it.
  Someone editing documentation touches Markdown in `docs/` and need not open `.fumadocs/` at all.
- **A dependency tree `cargo-deny` cannot see.** FR-093's advisory watch is Cargo-only, so the
  site's npm dependencies need their own watch (R38).
- **A package manager the repository has to pin.** pnpm is the choice (R31a).
- **A heavier build** than a single static binary — a dependency install and a Next.js build,
  against mdBook's one command.

**R31a: the package manager is pnpm.** Chosen by the project owner. It is the right shape for this
repository for a reason worth recording: pnpm hardlinks packages from one content-addressed store,
so the 432-package tree costs disk **once per machine** rather than once per checkout, and the
`docs` job caches that store rather than a tarball cache. `pnpm install --frozen-lockfile` also
fails on a lockfile that disagrees with `package.json` instead of quietly resolving around it,
which is the behaviour a merge gate wants. The costs are a second binary a documentation
contributor installs (`pnpm`, not shipped with Node) and the approval precondition recorded in R46.
**npm** — one fewer thing to install, and the default the scaffolder assumes; rejected on the
owner's instruction. **Yarn**, **Bun** — the template supports both; neither was asked for.

**Alternatives considered.** **mdBook** — the displaced choice, and still the cheapest: one static
Rust binary, no second toolchain, no lockfile from another language. It loses on search and on the
include mechanism FR-084 depends on. **MkDocs Material** — the same second-toolchain cost as
Fumadocs (Python instead of Node) with a plugin set to maintain, and no advantage over it.
**Docusaurus** — the same Node cost for a heavier, less documentation-focused framework.
**GitHub's own wiki or bare Markdown in the repository** — no navigation, no build to fail, and
FR-078's "a build failure MUST be reported" then has nothing to report on.

## R32: Making documentation drift impossible (FR-083, FR-084)

**Decision.** The configuration and style reference is **included, not restated**. The site's
`docs/user/configuration.md` and `docs/user/styling.md` pull
`specs/002-overlay-visuals/contracts/config.md` and `contracts/style-values.md` in with Fumadocs'
include directive, and the existing unit test that walks the style-value catalogue against
`theme.rs`'s `COLOURS`/`GEOMETRY`/`TEXT_SIZE` catalogues is extended to cover every setting
`config.rs` accepts, by walking `specs/001-workspace-swap-overlay/contracts/config.md` and
`specs/002-overlay-visuals/contracts/config.md` as well — which means extending `catalogue()`'s
"first column is `Key`" table rule to those two pages' shape, or normalising their tables to it.

Because the pages are Markdown rather than MDX (R48), the syntax is the directive form:

```md
::include[../../specs/002-overlay-visuals/contracts/config.md]
::include[../../specs/002-overlay-visuals/contracts/style-values.md#colours]
```

**Rationale.** This is the cheapest possible answer to FR-083 and it already half exists — feature
002 built the catalogue test precisely so that a new setting cannot be added without documenting
it. Including rather than copying means the published page and the tested page are the same bytes,
so "the documentation drifted" is not a state the repository can reach: the check that fails is
the existing unit test, in `cargo test --lib`, with no new tooling and no new format.

**Verified against a real build**, because the whole guarantee rests on it and most of what follows
is not stated in Fumadocs' documentation (`+next+fuma-docs-mdx+static` scaffold, Fumadocs 16.15.4 /
`fumadocs-mdx` 15.4.0 / Next 16.3.3, against this repository's actual 002 contracts):

1. **A path out of the documentation tree resolves.** The include plugin calls `path.resolve` on
   the including file's directory with no containment check, so a page in `docs/user/` reaches
   `../../specs/…`. The included file is registered with the bundler as a build dependency, so
   editing a contract rebuilds the page — mdBook's `extra-watch-dirs` has no counterpart here and
   is not needed.
2. **But only if Turbopack's root is widened** — see the constraint below. This is the single
   configuration line the whole mechanism depends on.
3. **A `.md` include is parsed as Markdown, not MDX**, so a brace in a contract — `{like_this}` —
   is literal text rather than a JSX expression. Confirmed by building one in. This is what makes
   it safe to point the site at files written for a different audience by people not thinking
   about MDX.
4. **Heading extraction is real and scoped.** `style-values.md#colours` emitted the `Colours`
   section and demonstrably *not* the `Built-in themes`, `Geometry` or `Values that are
   deliberately absent` sections that follow it. So a page may take one section of a contract
   without taking the whole file, which mdBook could only do by line range.

**Constraint: `turbopack.root` must name the repository root.** Left at its default, Next resolves
the module graph against the Next project directory and refuses the include outright — the build
fails with `FileSystemPath("").join("../specs/…") leaves the filesystem root`, which names neither
the page nor the include. `docs/.fumadocs/next.config.mjs` therefore sets:

```js
turbopack: { root: path.join(import.meta.dirname, '..', '..') },
```

This is load-bearing, not tidiness: without it FR-084's mechanism does not build at all. It is
recorded here because the failure is opaque enough to cost an afternoon otherwise.

**Constraint: plugins are configured as an array, never as a function.** The include directive
needs `remark-directive`, and the only form that survives `defineDocs`' macro expansion is the
array:

```ts
mdxOptions: { remarkPlugins: [remarkDirective] }
```

The function form the type signature also advertises — `(v) => [remarkDirective, ...v]` — fails at
build time with `TypeError: (mdxOptions.remarkPlugins ?? []) is not iterable`, because the macro
serialises the configuration and the callback never receives the default list. The array form is
**not** a replacement: Fumadocs splices the project's plugins into the middle of its own chain
(`[gfm, heading, include, image, codeTab, npm, ...yours, structure]`, from `preset-bundler.js`), so
GitHub-flavoured tables, heading anchors, the include plugin itself and the search-structure pass
all survive. Verified by building tables and search out of the result.

**Constraint: no bare raw HTML in an included contract.** An included `.md` is Markdown, so
`<NotAComponent>` in it is a raw-HTML node, and the pipeline fails with `Cannot handle unknown
node 'raw'` — again naming no file. Backticked generics (`` `Vec<String>` ``) are code nodes and
are safe, which is what the contracts already write. `rehype-raw`, the remedy Fumadocs suggests,
was tried and set aside: it needs a `passThrough` list for every MDX node type before it compiles
at all, and it rewrites the tree in a way that cost the search index in testing. Since the
contracts have no reason to contain raw HTML, `scripts/checks.sh` grows a `docs-html` rule that
greps the included files for a bare tag outside backticks and fails with the file, the line and the
fix — a better message than either tool produces, for a shell one-liner and no dependency.

**Constraint: code fences must name a language Shiki knows.** ` ```conf ` — the obvious fence for a
`hyprland.conf` snippet, and the bind page is full of them — fails the build with `ShikiError:
Language 'conf' not found`. Use ` ```ini `. The `docs-html` rule checks this too, since it is the
same class of mistake: a documentation edit that breaks a build with an error naming no file.

**Alternatives considered.** **Generating the reference from the code** (a `--dump-settings` flag
feeding a build step) — a new external surface nobody asked for, plus a generator to maintain, to
replace a table that a human should be writing prose around anyway. **Copying the catalogue into
the site tree at build time** — the honest fallback if the cross-tree include had not worked, and
it would still have preserved single-authorship, but it adds a generated tree to gitignore and a
copy step to explain; the include makes it unnecessary. **Copying the catalogue into `docs/` and
testing both copies** — two files holding one truth, which is the duplication Principle III exists
to prevent. **Moving the catalogue out of `specs/` into the site** — would break FR-084a: the
contracts stay authoritative and the site presents them.

## R33: Building the distribution packages (FR-106, FR-109, FR-109a)

**Decision.** `cargo-deb` and `cargo-generate-rpm`, each **run inside a container of the oldest
still-supported release of its family**, with the metadata for both living in `Cargo.toml` under
`[package.metadata.deb]` and `[package.metadata.generate-rpm]`.

**Rationale.** The binary links cairo, pango and glib dynamically, so what a package can run on is
decided by the glibc and library versions it was *built* against, not by the packaging tool.
Building on the oldest supported release and running everywhere newer is the standard answer, and
it is comfortably within reach — the crates' own declared minimums are **cairo 1.14, pango 1.40
and glib 2.56** (**[verified]** from the `system-deps` metadata of `cairo-sys-rs` 0.22,
`pango-sys` 0.22 and `glib-sys` 0.22.8), while Ubuntu 22.04 already carries cairo 1.16, pango 1.50
and glib 2.72. The two tools generate their package straight from `Cargo.toml`, need no `rpmbuild`
and no `debhelper`, and keep the packaging metadata in the file that already holds the version, so
FR-105's "raise the version wherever it is recorded" stays a one-line change.

**Alternatives considered.** **`nfpm`** — one tool for both formats, but a second configuration
file duplicating what `Cargo.toml` already says. **Native `debuild`/`rpmbuild` recipes** — the
fully-correct route for getting *into* a distribution's archives, which is not what this feature
promises (the packages are published on the project's own releases page); a `debian/` directory
and a spec file are a lot of surface for that. **A static build against musl** — cairo, pango and
their font stack make that a research project of its own, and it would change what the program
actually links against on a user's machine.

## R34: What the packages are verified against (FR-109a, SC-039)

**Decision.** The published matrix, as of 2026-08-30: the Debian-family package is built on
**Ubuntu 22.04 LTS** and verified to install and run on 22.04 and on the current LTS; the
RPM-family package is built on the **oldest Fedora still receiving updates** (Fedora 43 as of
2026-08-30, pinned in [contracts/packaging.md](./contracts/packaging.md)) and verified on that
and on current. The *rule* — "built on the oldest still-supported release of the family, verified
on oldest and current" — is what the documentation states; the concrete version numbers live in
one table in `contracts/packaging.md` and are revisited at each release.

**Rationale.** Distribution support windows move faster than the project will. Publishing the rule
and one dated table means a reader always knows what the promise is, and the release checklist has
one place to correct. The verification runs in containers of those exact releases as part of the
release workflow, which is what turns SC-039 from a claim into a check.

## R35: The Arch recipe, kept in step (FR-107)

**Decision.** A `PKGBUILD` at `packaging/aur/PKGBUILD` building from the release's source archive.
The release workflow rewrites its `pkgver` and `sha256sums` from the artefacts it just published,
commits that, and attaches the file to the release; pushing it to the AUR is a documented
maintainer step, automated by the same job when an AUR SSH key is configured.

**Rationale.** FR-107's requirement is that the recipe cannot fall behind the release, which is
satisfied by generating it from the release rather than by hand. Keeping the push conditional on a
secret being present means a fork or a first release without AUR credentials still produces a
correct recipe rather than a failing workflow.

**Alternatives considered.** A `-git` PKGBUILD tracking the default branch — no integrity value to
verify, and it contradicts FR-103's "a release is a tag". Maintaining the recipe only in the AUR
repository — then it is not in the source tree and FR-111's "a packager needs nothing from the
maintainer" is untrue.

## R36: The release workflow (FR-105, FR-108, FR-110)

**Decision.** One `workflow_dispatch` workflow taking the version as its only input. In order: it
refuses unless the gating checks are green on the commit, the tag does not exist, and the tree is
clean; raises the version in `Cargo.toml` (and `Cargo.lock` via `cargo update -w`); renames the
changelog's `[Unreleased]` section to the version and the date; commits; tags; builds the binary,
the `.deb` and the `.rpm` in their respective containers; computes a `SHA256SUMS` file over every
artefact; and publishes the source archive, the three binaries and `SHA256SUMS` to the GitHub
release. Re-running it for a version whose tag already exists **fails** unless the release is still
a draft, in which case it replaces the draft's artefacts from the same tag — so a half-finished
release is resumable and can never produce two different files for one version.

**Rationale.** Every precondition in FR-110 is a cheap check at the top of the workflow, and doing
them there is what makes "nothing about the procedure lives in the maintainer's head" true.
`SHA256SUMS` is what every distribution's tooling already expects, and it is what the `PKGBUILD`
of R35 reads.

**Alternatives considered.** **`cargo-dist`** — generates exactly this kind of workflow and is very
good at it, but it owns the workflow file, prefers its own installer story, and does not cover the
`.deb`/`.rpm`-on-oldest-LTS requirement without configuration that is longer than the workflow it
replaces. **Signing artefacts (GPG or sigstore)** — beyond FR-108, which asks for verifiable
integrity rather than verified provenance; adding a signing key to a single-maintainer project is
a key-management burden no requirement asks for.

## R37: What version a non-release build reports (FR-103, FR-104)

**Decision.** `build.rs` asks git for `describe --tags --always --dirty`; `VERSION` becomes
`CARGO_PKG_VERSION` alone when the build is exactly a release tag, and
`CARGO_PKG_VERSION+<describe>` otherwise. With no git available — a source archive, a distribution
build — the plain `CARGO_PKG_VERSION` stands. `build.rs` emits `cargo::rerun-if-changed` for
`.git/HEAD` and the packed refs so the string cannot go stale.

**Rationale.** FR-103 wants the release build to say exactly the tag, and FR-104 wants any other
build to identify its source; one string with an optional suffix does both, and a bug report
carrying `1.0.0+v1.0.0-14-gabc1234-dirty` is immediately actionable. `build.rs` already exists for
the protocol scanner, so this adds a function, not a mechanism.

## R38: Advisories and dependency licences (FR-064, FR-093, FR-121)

**Decision.** **`cargo-deny`**, one `deny.toml`, run as its own CI job: `advisories` for FR-093 and
`licenses` for FR-064. An advisory with no available fix is accepted by an `ignore` entry whose
`reason` **must** begin `until YYYY-MM-DD:`; a unit test parses `deny.toml` and fails once that
date has passed, or if the form is wrong.

**Rationale.** One tool answers two requirements — the advisory watch and the packager's
"what licences am I redistributing" question — from one file, and its licence check is the only
honest way to make FR-064's statement true rather than aspirational. cargo-deny has **no built-in
expiry** for ignores, so the time bound is carried in the `reason` string it does support and
enforced by a test in the repository; the project already uses a unit test to hold a document and
the code together (002's style-value catalogue), so this is an existing idiom rather than a new
one, and it costs no new dependency — `toml` is already in the tree.

**The site's npm tree is the one thing this does not cover.** `cargo-deny` reads Cargo metadata,
so nothing it does says anything about `docs/.fumadocs/pnpm-lock.yaml` (R31). Leaving it there would
make FR-093's advisory watch quietly partial, so the npm tree is watched by **Dependabot** with an
`npm` ecosystem entry beside the `cargo` one, pointed at `/docs/.fumadocs`, in
`.github/dependabot.yml`. The asymmetry is
deliberate and worth stating: a Cargo advisory reaches a maintainer through a check, an npm
advisory through a pull request. It is proportionate — nothing in `docs/.fumadocs/` ships to a user or
runs on their machine; its blast radius is a documentation site built by CI.

**Alternatives considered.** **`cargo-audit`** alone — advisories only, leaving FR-064 unanswered.
**Dependabot** — raises pull requests but does not fail a check, so an advisory could sit
unnoticed; it is complementary and enabled, not a substitute, for the Cargo tree. **`pnpm audit` as
a gating job** — would fail the merge gate on a transitive advisory in a build-time-only
documentation dependency, which is the "permanently red" outcome FR-093 legislates against. **Letting an unfixable advisory fail
the build indefinitely** — the outcome FR-093 explicitly legislates against, because a permanently
red project teaches everyone to ignore red.

## R39: Which checks gate a merge (FR-085, FR-090, FR-091)

**Decision.** One workflow, one job per check, and a single aggregating `ci-required` job that
depends on the gating ones — that job is the only branch-protection requirement. **Gating**: the
release-profile build, `cargo test --lib`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check`, the minimum-toolchain build (`rust-version` from `Cargo.toml`, so the declared
minimum cannot drift — FR-087), the documentation build, and the E2E suite. **Informational**:
`cargo-deny advisories` (an advisory is news about the world, not a defect in the change) and any
check the spike of R29 leaves unstable. Every job's failure step prints the exact local command
that reproduces it (FR-090).

**Rationale.** An aggregate job makes the gate a property of the repository's configuration rather
than of each job's name, so a renamed or newly-added job cannot silently stop gating or silently
start; that is exactly what FR-091 asks for, and it is one file to read.

## R40: The daemon's lifecycle record (FR-112, FR-113, FR-114)

**Decision.** Two new `diag::Condition` variants, `Started` and `Stopping`, both `Level::Info` and
both `summary() == None` (no notification). `Started` is reported once, from `serve`, when the
client, the world and the event stream are all up: `hypr-swap 1.0.0 started`. `Stopping` is
reported on every exit path — a signal (`stopping: SIGTERM`), a fatal start-up condition
(`stopping: cannot reach the compositor at start-up`), and normal end — including exits that
happen before start-up completes, which is the case the spec's last edge case is about.

**Rationale.** FR-114 forbids touching the levels, the format or the notification policy, and this
adds nothing to any of them: the two records are ordinary `diag::report` calls at an existing
level with an existing shape, and the policy table in `Condition` keeps owning the answer. `Info`
rather than `Warn` because a daemon starting is not a problem; no notification for the same reason.
The reconnection loop already reports its own transitions (`CompositorConnection`), so the
lifecycle records bracket a session rather than narrating it.

**Alternatives considered.** A logging crate (`log` + `env_logger`, `tracing`) — a dependency and a
verbosity model the clarification explicitly rejected, to produce the two lines `diag.rs` already
knows how to produce. Recording start-up before the client is built — would claim a working daemon
in exactly the case where it is about to exit 3.

## R41: The environment report (FR-116)

**Decision.** A new `--environment` flag that prints, to stdout, and exits 0: the program version
including the build suffix of R37; the compositor version from `hyprctl`'s `j/version` (`version`
and `tag`); the resolved configuration file path and whether it exists; the settings that differ
from their defaults; the icon set in effect; and whether `notify-send` is on `PATH`. It is the
same block the bug-report form asks the reporter to paste.

**Rationale.** FR-097 asks the reporter for facts they mostly cannot assemble by hand — "which
icon set is actually in effect" is a resolution result, not a setting. Printing *differences* from
the defaults rather than the file's contents keeps a user's `config.toml` out of a public issue
unless they chose to paste it, and keeps the block short enough that people will actually include
it. It reads nothing the daemon does not already read (FR-071's promise), and it is stdout, not a
diagnostic, because it is an answer to a question rather than a report of a condition.

**Alternatives considered.** `--version --verbose` — overloads a flag whose output other tools may
parse. A `--debug-dump` of the whole internal state — a new external surface with no requirement
behind it (Principle II), and a much better way to leak a user's window titles into an issue.

## R42: Refusing to guess against an unsupported compositor (FR-118)

**Decision.** At start-up, after the IPC socket is found, request `j/version`, take its `version`
field, compare against a single `SUPPORTED_HYPRLAND` range constant, and on a version below the
minimum report a new `Condition::CompositorVersionUnsupported` at `Level::Warn` naming both the
found version and the supported range — then carry on. An unparseable or absent version is
reported the same way and is likewise not fatal. The parse and the comparison are a pure function
in `model.rs`, unit-tested; the E2E test drives it through an env-gated override of the version
string, the same hook idiom `hypr/ipc.rs` already carries for the 001 rollback tests.

**Rationale.** `j/version` reports a clean `"version": "0.56.2"` alongside its tag and commit
(**[verified]** against the running compositor), so this costs one request at start-up. A warning
rather than a refusal because the daemon may well work, and taking someone's switcher away over a
version comparison is a worse failure than the obscure one FR-118 is trying to replace; the record
is what turns "nothing happens when I press the key" into a report the maintainer can act on.

## R43: A configuration file written for an earlier release (FR-117)

**Decision.** No new machinery. Three things already in place satisfy it, and the plan makes them
explicit rather than adding a compatibility layer: every key the program does not recognise is
already reported (`Condition::UnknownConfigKey`), every invalid value is already reported and
falls back alone (FR-024), and FR-101a makes changing the meaning of a key a **major** version
change. The addition is an E2E test that runs the current build against
`tests/fixtures/config-previous-release.toml` — a committed copy of the configuration contract as
of the last release, which at 1.0.0 is the 1.0.0 contract itself, since there is no earlier one to
read — asserting identical behaviour and no diagnostics, plus two release-checklist items: refresh
that fixture, and keep a renamed or removed key's old name recognised, reporting what replaced it.

**Rationale.** A migration framework for a program with one flat configuration file and no
released history would be the definition of speculative generality (Principle II). The
requirement is that a user's setting is never silently reinterpreted; the mechanism that
guarantees that is the versioning policy plus a test that would notice, not code.

## R44: Publishing the history (FR-066a)

**Decision.** Before the repository is made public, scan the full history with `gitleaks detect
--log-opts=--all`, review by hand what it reports, and record the outcome — tool, version, date,
commit range, findings — in `specs/003-oss-release-readiness/history-review.md`, referenced from
the security policy.

**Rationale.** The tree is 19 commits by one author with no deployment credentials anywhere in its
design, so the expected finding count is zero; the requirement is that the review *happened and is
recorded*, and an unrecorded clean scan is indistinguishable from no scan. The recorded outcome
also settles the question for anyone auditing the project later.

## R45: Licence text and third-party attribution (FR-062, FR-063, FR-066)

**Decision.** `LICENSE` at the top level (MIT, naming the copyright holder and 2026), matching the
`license = "MIT"` already in `Cargo.toml`. A `THIRD-PARTY.md` at the top level accounting for
everything that ships inside the tree that originates elsewhere: `protocols/hyprland-global-shortcuts-v1.xml`
(upstream repository, revision, its own licence) and `assets/placeholder.svg` (origin and licence),
each also carrying its provenance in a comment in the file itself. Both packaging tools are
configured to install `LICENSE` and `THIRD-PARTY.md` under the family's documentation directory.

**Rationale.** FR-063's test is that a reviewer with only the source tree can attribute every
file; a top-level account plus an in-file header means neither a file moved out of the tree nor a
tree read without the index loses the attribution. The dependency graph's licences are a different
question, answered by cargo-deny's licence check (R38) rather than by a vendored list that would
be stale within a month.

## R46: Publishing the site (FR-078, FR-078a)

**Decision.** A `docs` workflow builds the site on every push to the default branch and deploys it
with GitHub Pages' own `actions/deploy-pages`. The pull-request build is a separate, gating `docs`
job inside `ci.yml`, so that it reaches the `ci-required` verdict; `docs.yml` itself does not run
on pull requests and never deploys from one. The site's front page states which release it documents and marks
anything already on the default branch but not yet released. No versioned snapshots.

**What "build" means under Fumadocs (R31).** `pnpm install --frozen-lockfile && pnpm build` in
`docs/.fumadocs/`, producing a static export in `docs/.fumadocs/out/` which is what gets uploaded —
there is no Node server in the deployment. Four settings make that export land correctly on Pages, and all four are project
configuration rather than anything a page author sees:

- `output: 'export'` — already set by the `+static` template.
- `basePath: '/hypr-swap'` and a matching `assetPrefix`, because the site is served from a
  repository subpath rather than a domain root. Wrong here means a site that loads and is entirely
  unstyled.
- `images: { unoptimized: true }`, since the optimiser is a server route that an export cannot
  carry.
- A `.nojekyll` file in the uploaded artefact: Pages runs Jekyll by default and Jekyll skips
  directories beginning with an underscore, which would drop `_next/` — the whole bundle.

Search is part of the export rather than a service: the Orama index is built at build time and
queried in the browser, and it appears in the artefact as `out/api/search` (~100 KB for a
nine-page site, measured). Nothing is sent anywhere and no account is needed, which keeps FR-071's
privacy statement true of the site as well as the program.

**Caching.** The `docs` jobs cache the **pnpm store** (`pnpm store path`) keyed on
`docs/.fumadocs/pnpm-lock.yaml` — `pnpm/action-setup` followed by `actions/setup-node` with
`cache: 'pnpm'` is the standard pairing. An uncached install of this dependency tree resolves 432
packages and dominates the job; the built site is a few megabytes and the lockfile is 136 KB.

**One pnpm-specific precondition, or every `docs` job fails.** pnpm 11 does not run dependency
build scripts unless the project has approved them, and it treats an unapproved script as an
**error**, not a warning:

```text
[ERR_PNPM_IGNORED_BUILDS] Ignored build scripts: esbuild@0.28.2
```

`esbuild` needs its `postinstall` to place its platform binary, so this stops the build outright —
and it stops it identically in CI, where there is no one to run the interactive `pnpm
approve-builds`. The approval is therefore committed, in `docs/.fumadocs/pnpm-workspace.yaml`:

```yaml
allowBuilds:
  esbuild: true
```

Two details cost time if not written down: the file is `pnpm-workspace.yaml` even though this is a
single package and not a workspace, and the key is `allowBuilds` — pnpm 11 renamed it from pnpm
10's `onlyBuiltDependencies`, and it **no longer reads the `pnpm` field in `package.json` at all**,
which is what most existing guidance still tells you to edit. Let `pnpm approve-builds --all`
write the file rather than hand-writing it, then commit the result.

**Rationale.** Building on pull requests is what stops the "the site is stale and nobody was told"
edge case: a broken book fails the change that broke it, not the deployment afterwards. One
version is the clarified requirement and it is also what keeps FR-083's drift check meaningful —
the check can only compare documentation against the code sitting beside it.

## R47: Showing the overlay on the front page (FR-070)

**Decision.** Two PNG screenshots, one per presentation, captured with `grim` from the E2E
harness's own nested instance so they show a controlled workspace set rather than the maintainer's
desktop, committed under `docs/assets/` and referenced from both the README and the site.

The images live in `docs/assets/` beside the pages that use them, so a reader browsing the folder
and the README both reach them by an ordinary relative path. Next.js only serves what is under its
own `public/`, so the site's `prebuild` script copies `docs/assets/` to
`docs/.fumadocs/public/assets/`, and that destination is gitignored — one authoritative copy of
each image, in the directory the prose needs it in.

**Rationale.** The one requirement in this feature that cannot be met with text. Capturing from
the nested instance means the images can be regenerated deterministically when the appearance
changes, and it keeps the maintainer's real windows and window titles out of the repository.

## R48: The documentation tree's shape (FR-076, FR-077, FR-084)

**Decision.** `docs/` **is** the documentation — plain Markdown, organised for a person reading
files, with the framework hidden beneath it in `docs/.fumadocs/`:

```text
docs/
├── index.md            # front page
├── meta.json           # section order
├── user/               # FR-077's first section
│   ├── meta.json       # "User guide"
│   └── binds.md  install.md  configuration.md  styling.md  icons.md  troubleshooting.md
├── dev/                # FR-077's second section
│   ├── meta.json       # "Developer guide"
│   └── architecture.md  workflow.md  testing.md  verification.md  releasing.md
├── assets/             # screenshots, referenced by the README too
└── .fumadocs/          # the Next.js project: package.json, next.config.mjs, src/, lockfile
```

The Next project reads the tree above it — `defineDocs({ dir: '..' })` — rather than owning a
`content/` directory of its own. `.fumadocs/` is skipped by the content glob because it is
dot-prefixed, so the framework does not have to be excluded by hand.

**Rationale.** The requirement is that the tree read as well in an editor as on the published site,
and three properties deliver that, each verified by building them:

- **Plain `.md`, not `.mdx`.** Every page is Markdown an IDE previews and a forge renders. Nothing
  in this documentation needs JSX, and the one thing that would have forced MDX — the include — has
  a Markdown directive form (R32).
- **Relative links between files resolve both ways.** A page written `[binds](./user/binds.md)`
  opens the neighbouring file in an editor, and Fumadocs rewrites it to `/docs/user/binds` in the
  built site. Verified in the export: `./user/binds.md` → `/docs/user/binds`, `../index.md` →
  `/docs`. This is the single property that makes "browse the folder" and "browse the site" the
  same act, so links between pages MUST be written in the relative-file form.
- **Directory names are the navigation.** `user/` and `dev/` are FR-077's two sections, and the
  only non-Markdown files among the documents are two-line `meta.json` files giving each section
  its title and page order.

One page is pinned by the code: `user/binds.md` is `include_str!`d by `ui/shortcuts.rs`, which
asserts it quotes every bind line the program generates (FR-022b). That is a deliberate exception
to the freedom the rest of the tree has — it is the one page whose *content* must agree with the
binary, so it is the one page that cannot be renamed without a compile error saying so. The
assertion covers the bind lines only; the prose around them is as free as any other page's.

**Cost, and it is a real one: search needs an explicit index builder.** Fumadocs derives its search
index from a `structuredData` export that only compiled MDX carries, so a tree of plain Markdown
fails the static export with `Cannot find structured data from page, please define the page to
index function`. The remedy is the one the error names — `createFromSource(source, { buildIndex })`
in `docs/.fumadocs/src/app/api/search/route.ts`, building each record from the page's processed
text with `structure()` from `fumadocs-core/mdx-plugins/remark-structure`:

```ts
async buildIndex(page) {
  return {
    id: page.url, url: page.url,
    title: page.data.title, description: page.data.description,
    structuredData: structure(await page.data.getText('processed')),
  };
}
```

Twelve lines, written once, and it indexes the *processed* text — so the contracts pulled in by
`::include` become searchable on the site, which the default MDX path would also have done. This is
the price of Markdown pages and it is worth paying for them.

**Alternatives considered.** **A separate top-level `website/`** — the shape this plan carried
until the documentation tree was made the root; it kept the framework further from the prose but
split the documentation across two directories and left `docs/` a stub. **`docs/` as the Next
project root directly**, with `package.json`, `next.config.mjs`, `tsconfig.json` and `src/` beside
the prose — no hidden directory, but the first thing a reader opening `docs/` sees is
configuration, which is the opposite of the requirement. **`.mdx` pages** — first-class components
and search with no `buildIndex`, at the cost of files an IDE and a forge show as source rather than
as documents. **A `content/` directory inside `.fumadocs/`** — the template's default, and the
arrangement that buries the prose inside the framework.

