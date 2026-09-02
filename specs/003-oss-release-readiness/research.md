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

**What the spike then measured, on the runner itself (2026-09-02).** The first CI run of
`e2e.yml` settled the first half of route 1 and left the second half open — **[verified]**:

- **A hosted runner does not carry `vkms`**: `modprobe: FATAL: Module vkms not found in directory
  /lib/modules/6.17.0-1022-azure`. The module is not absent from the kernel, only from the runner
  image — it ships in `linux-modules-extra` for the running kernel, which is in the archive, so the
  workflow installs it rather than assuming it.
- **Neither `HYPRLAND_*` nor `AQ_*` has grown a headless escape hatch** since finding 1 was
  measured. The image pulls Hyprland 0.56.2 and aquamarine 0.14.0, and the only backend knobs the
  two binaries carry are `AQ_DRM_DEVICES`, `AQ_FORCE_LINEAR_BLIT`, `AQ_LIBINPUT_NO_PLUGINS`,
  `AQ_MGPU_NO_EXPLICIT`, `AQ_NO_ATOMIC`, `AQ_NO_MODIFIERS` and `AQ_TRACE`. `CHeadlessBackend` is in
  the library, but nothing selects it at start-up — it is what `hyprctl output create headless`
  reaches, which is finding 1 restated with the symbol names. A DRM device is genuinely required.

**The run after that (2026-09-02), with the module installed** — **[verified]**:

- **`vkms` loads, and it is not alone.** The container saw `card0 card1`: an Azure image already
  carries a Hyper-V framebuffer (`pci id 1414:0006, driver (null)`, repeated across every fd the
  loader probed) and `vkms` arrives beside it. Neither publishes a `renderD*`. Left to scan,
  aquamarine has a framebuffer to pick that cannot allocate, so the entry point now resolves the
  vkms node through `/sys/class/drm/*/device/driver` and names it in `AQ_DRM_DEVICES`.
- **`seatd` is not the problem.** `Created VT-bound seat seat0`, the compositor connected
  (`Added client 1 to seat0`, `Opened client 1 on seat0`) and then disconnected on its own death.
  Route 1's seat half works.
- **The compositor crashed with its reason in a file.** Hyprland disables stdout logging a few
  lines into start-up and continues into `$XDG_RUNTIME_DIR/hypr/<signature>/hyprland.log`, so the
  captured output ended at `failed to mkdir() crash report directory / Permission denied` with no
  cause attached. Two fixes, both of them defects rather than discoveries: the parent's config now
  sets `debug:enable_stdout_logs`, and the entry point dumps the log file as well as the pipe.
- **`HOME` survived the privilege drop as root's.** `setpriv` changes credentials and nothing else,
  and the crash-report `mkdir` was the symptom. Every path derived from `HOME` was wrong; it is now
  read from `getent passwd` and passed across with `USER` and `LOGNAME`.

**The run that finally said why (2026-09-02)** — **[verified]**, and the cause is *not* the
allocator that finding 1 met:

```
drm: Found 2 GPUs
drm: Starting backend for /dev/dri/card1, with driver hyperv_drm
drm: gpu /dev/dri/card1 becomes primary drm
drm: Starting backend for /dev/dri/card0, with driver vkms with primary /dev/dri/card1
Created a GBM allocator with drm fd 21          ← GBM works, on both devices
Created a GBM allocator with drm fd 24
...
DRM dev /dev/dri/card1 has no render node, falling back to primary
openRenderNode got drm device /dev/dri/card1
ERR: openRenderNode failed to open drm device /dev/dri/card1
CRIT: ASSERTION FAILED! Couldn't open a gbm fd  at line 348 in OpenGL.cpp
```

Aquamarine created GBM allocators on **both** devices, which is the thing finding 1 could not do.
What failed afterwards is an `open(2)`: libseat hands the compositor an fd for the card, but
Hyprland opens a render node *itself* for its GL context, and that open bypasses libseat. The
container's unprivileged user was in `seat` and nothing else, so it had no permission to open a DRM
node directly. The image now puts it in `video`, `render` and `input` as well.

The same log corrected the device story. `vkms` registers on the **faux** bus
(`/sys/devices/faux/vkms/drm/card0`), where `device/driver` resolves to nothing — so the first
attempt at naming the vkms node reported "no vkms node found" on a runner that had one, and the
backend scanned and made the Hyper-V framebuffer primary. `hyperv_drm` has no render node and one
plane format against vkms's twenty-two. The driver is now read from `device/uevent` with the sysfs
path as the fallback the faux bus needs, and `AQ_DRM_DEVICES` lists vkms first with the others
behind it rather than hidden.

**What is still open** is narrower than it was: whether Hyprland's GL context comes up on a
`vkms` primary once the node can be opened at all. The allocator question finding 1 raised is
answered — GBM works here — so the remaining risk is the renderer, not the buffer. If it does not
come up, route 2 is the answer: a QEMU `virtio-gpu` is a PCI device with a real render node, which
removes both of this run's failure modes rather than working around them.

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

**Decision.** **docmd** (`@docmd/core`), a zero-configuration Markdown site generator, run from
the repository root against `docs/` and published to GitHub Pages. **`docs/` is the documentation
and holds nothing but plain Markdown**; the whole of the framework is three files beside
`Cargo.toml` — `docmd.config.mjs`, `package.json`, `pnpm-lock.yaml` and `pnpm-workspace.yaml` —
plus the one plugin in `scripts/docmd-include.mjs` that R32 needs. Pages live in a `user/` and a `dev/` folder that are
FR-077's two sections. The shape is R48's decision; this one is only the framework.

**Rationale.** This decision has been revised twice. It first chose mdBook, on the grounds that it
kept the project to a single language toolchain; it was then directed to Fumadocs, for search and
for a heading-scoped include; and it was directed to docmd after review, on the grounds that a
Next.js application is disproportionate machinery for twelve Markdown pages. Both earlier choices
are discarded and nothing in the repository refers to them. What follows is the case for what is
here now, not a defence of the route taken to it.

What docmd gives, against the requirements as written:

- **Search that exists, with nothing to configure.** The index is built at build time and queried
  in the browser. Nothing is sent anywhere and no account is needed, which keeps FR-071's privacy
  statement true of the site as well as of the program. Under the previous framework this needed
  a hand-written index builder, because plain Markdown pages carried no structured data; here
  plain Markdown *is* the input format and the index covers included text without being asked.
- **A site that does not care where it is served from.** Every generated URL is relative, so the
  same output works at a domain root, at `/hypr-swap/` on GitHub Pages, and on a dev server at
  `http://localhost:3000`. There is no base path to set and no way to get it wrong — which is the
  one thing the previous framework did get wrong, silently, in development.
- **`.nojekyll`, `sitemap.xml` and `robots.txt` written for us**, rather than being a publishing
  step to remember (R46).
- **A link checker.** `docmd validate` walks every internal link and reference. A documentation
  site's characteristic defect is a link that stopped resolving, and this catches it in the same
  run that builds the site.
- **Proportion.** 60 packages, a 1.6-second build, and one command. The tree it replaced was 432
  packages and a React application whose configuration had to be understood before a page could be
  added. Constitution Principle I is the reason this matters: the simplest thing that satisfies
  the requirement is the thing to build.

What it costs, stated plainly:

- **A second language toolchain in a Rust repository** — Node.js ≥ 22, a `package.json` and a
  lockfile. This cost is inherent to any documentation site richer than raw Markdown, and it is
  accepted deliberately rather than overlooked. A contributor who touches only Rust never installs
  Node, and no CI job outside `docs` needs it.
- **A dependency tree `cargo-deny` cannot see.** FR-093's advisory watch is Cargo-only, so the
  site's npm dependencies need their own watch (R38).
- **No transclusion of its own.** FR-084 rests on including the `specs/` contracts rather than
  restating them, and docmd has no directive for it. This is the one real gap, and R32 closes it
  with a local plugin.
- **A young project.** docmd is at 0.9.x and moves quickly. The version is pinned exactly in
  `package.json` and installed with `--frozen-lockfile`, so a release upstream cannot change what
  this repository builds; upgrading is a deliberate act with the site build as its test.

**R31a: the package manager is pnpm, and it is the only one this project supports.** Chosen by
the project owner. It is pinned in `package.json`'s `packageManager` field, so `corepack` resolves
the exact version rather than whatever a contributor happens to have, and both the documentation
and the workflow name it explicitly. `pnpm install --frozen-lockfile` installs exactly the lockfile
and fails on one that disagrees with `package.json` rather than quietly resolving around it, which
is the behaviour a merge gate wants. pnpm also hardlinks packages from one content-addressed store,
so the tree costs disk once per machine rather than once per checkout, and the `docs` jobs cache
that store rather than a tarball cache.

Using another manager is not a neutral choice here, which is why both documents that name the
command say so: npm or Yarn would write a second lockfile that nothing in the repository
describes, and would ignore `pnpm-workspace.yaml` — which is not decoration but the file that
decides which dependencies may run a build script (below). The costs are a second binary a
documentation contributor installs, `pnpm/action-setup` as a second CI step, and that approval
file. **npm** — one fewer thing to install, and the default a scaffolder assumes; rejected on the
owner's instruction. **Yarn**, **Bun** — neither was asked for.

**Alternatives considered.** **mdBook** — one static Rust binary and no second toolchain, which is
genuinely the cheapest thing here; it loses on search, and its `{{#include}}` extracts by line
range, which silently follows the wrong lines when the included file is edited, and this project
edits its contracts. **MkDocs Material** — the same second-toolchain cost in Python, plus a plugin
set to maintain. **Fumadocs, Docusaurus** and the other React documentation frameworks — a web
application, its build system and its component model, to publish twelve Markdown pages.
**GitHub's own wiki, or bare Markdown in the repository** — no navigation, no search, and no build
to fail, so FR-078's "a build failure MUST be reported" would have nothing to report on.

## R32: Making documentation drift impossible (FR-083, FR-084)

**Decision.** The configuration and style reference is **included, not restated**. The site's
`docs/user/configuration.md` and `docs/user/styling.md` pull
`specs/002-overlay-visuals/contracts/config.md` and `contracts/style-values.md` in at build time,
and the existing unit test that walks the style-value catalogue against `theme.rs`'s
`COLOURS`/`GEOMETRY`/`TEXT_SIZE` catalogues is extended to cover every setting `config.rs` accepts,
by walking `specs/001-workspace-swap-overlay/contracts/config.md` and
`specs/002-overlay-visuals/contracts/config.md` as well.

docmd ships no transclusion (R31), so the directive is a local plugin —
`scripts/docmd-include.mjs`, about a hundred lines, named from `docmd.config.mjs` — hooked into
`onBeforeParse`, which docmd calls with each page's raw Markdown and its path before parsing it:

```md
::include[../../specs/002-overlay-visuals/contracts/config.md]
::include[../../specs/002-overlay-visuals/contracts/style-values.md#colours]
```

Because the expansion happens before the parser, everything downstream — the table of contents,
the search index, `docmd validate` — sees the included text as ordinary page content, with no
second code path to keep in step.

**Rationale.** This is the cheapest possible answer to FR-083 and it already half exists — feature
002 built the catalogue test precisely so that a new setting cannot be added without documenting
it. Including rather than copying means the published page and the tested page are the same bytes,
so "the documentation drifted" is not a state the repository can reach: the check that fails is
the existing unit test, in `cargo test --lib`, with no new tooling and no new format.

**Writing the directive was preferred to doing without it.** A plugin is code this project now
maintains, which Principle I says to justify rather than assume. The justification is that the
alternative is not "a simpler include" but *no* include — and without one, FR-079 (the end-user
section contains the complete configuration specification) and FR-084 (exactly one authoritative
answer) cannot both hold: the page either restates the contract or sends the reader away to it.
The plugin is a pure Markdown-to-Markdown transformation with no framework knowledge in it, which
is also why the two earlier documentation frameworks left no trace in it and a third would not
either.

**Four behaviours, each chosen because the obvious version is wrong:**

1. **Paths resolve against the including page, and reach outside `docs/`.** `../../specs/…` from
   `docs/user/` is the whole point; there is no containment check because the tree being reached
   into is the same repository.
2. **`#slug` takes one section**, from the heading whose GitHub-style slug matches to the next
   heading at the same or a shallower level, and **without that heading itself** — the page has
   already given the section a heading of its own. A whole-file include drops the file's leading
   level-1 title for the same reason. Line-range includes are what mdBook offered and what this
   deliberately is not: a slug survives an edit to the file, a line number does not.
3. **Headings are demoted to fit.** Included headings are shifted so the shallowest sits one level
   below the page heading the directive appears under. Without this a contract's `## Schema`
   lands beside the page's own `##` headings and the page has two interleaved hierarchies; with
   it, one.
4. **Relative links inside an included file are re-pointed at the repository.** A contract links
   to its siblings under `specs/`, which are not pages on this site. They are resolved against the
   included file and rewritten to `repoBlobUrl`, so the link still reaches the document it names —
   which is FR-084a working as intended: `specs/` stays authoritative, and stays where it is.

Two smaller properties matter for the same reason: fenced code blocks are tracked, so a `#`
comment inside a TOML fence is not mistaken for a heading (the contracts are full of them), and a
missing file or an unknown slug **throws**, failing the build with both the page and the target
named, rather than publishing a page with a hole in it.

**Not a constraint any more, and worth recording as removed.** The previous framework failed the
build, with an error naming no file, on a bare `<tag>` in an included contract and on a code fence
naming a language its highlighter did not know — and `scripts/checks.sh` carried a rule for each.
docmd does neither: unknown fence languages degrade to plain highlighting, and raw HTML is a
configuration setting. Both rules were deleted. The site sets `security: { html: 'escape' }` so
that a stray `<path>` in a contract renders as the text it is rather than disappearing silently
into the markup, which is the failure a reader would never notice.

**Alternatives considered.** **Generating the reference from the code** (a `--dump-settings` flag
feeding a build step) — a new external surface nobody asked for, plus a generator to maintain, to
replace a table a human should be writing prose around anyway. **Copying the catalogue into `docs/`
at build time** — preserves single authorship, but adds a generated tree to gitignore and a copy
step to explain, and leaves the copy in the reader's editor looking like a source of truth.
**Copying the catalogue into `docs/` and testing both copies** — two files holding one truth, which
is the duplication Principle III exists to prevent. **Moving the catalogue out of `specs/` into the
site** — would break FR-084a: the contracts stay authoritative and the site presents them.

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
so nothing it does says anything about the site's `pnpm-lock.yaml` (R31). Leaving it there would
make FR-093's advisory watch quietly partial, so the npm tree is watched by **Dependabot** with an
`npm` ecosystem entry beside the `cargo` one, both pointed at `/`, in `.github/dependabot.yml` —
Dependabot's ecosystem is named `npm` and is the one that reads a `pnpm-lock.yaml`. The
asymmetry is deliberate and worth stating: a Cargo advisory reaches a maintainer through a check,
an npm advisory through a pull request. It is proportionate — nothing in the site's dependency tree
ships to a user or runs on their machine; its blast radius is a documentation site built by CI.

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
on pull requests and never deploys from one. The site's front page states which release it
documents and marks anything already on the default branch but not yet released. No versioned
snapshots.

**What "build" means (R31).** `pnpm install --frozen-lockfile && pnpm build` at the repository
root, producing static
HTML in `site/`, which is what gets uploaded — there is no Node server in the deployment. What
made this a section of its own under the previous framework was the four settings needed to land
an export correctly on a Pages subpath; docmd needs none of them:

- **The base path takes care of itself.** Every generated URL is relative, so `site/` works served
  from `/hypr-swap/` on Pages and from `/` on the dev server, unchanged. `url` in
  `docmd.config.mjs` is stated for canonical links and the sitemap, not to make the site load.
- **`.nojekyll` is written by the build.** Pages runs Jekyll by default and Jekyll skips
  directories beginning with an underscore, which would drop the site's search index; docmd emits
  the file without being asked.
- **Search is part of the output**, not a service: the index is built at build time and queried in
  the browser, as `site/_docmd-search/search-index.json`. Nothing is sent anywhere and no account
  is needed, which keeps FR-071's privacy statement true of the site as well as of the program.
  **Two of docmd's default plugins are turned off to keep that true**: `ai`, which puts a chat
  client carrying a cloud-relay endpoint on every page, and `okf`, which writes a knowledge bundle
  for AI agents beside the site. Neither is wanted here, and a site whose subject is a program that
  makes no network access should not itself ship 99 KB of third-party client. With both off, the
  shipped JavaScript contains no external fetch target at all — verified against the output.

The workflow still asserts what it uploads rather than trusting the build: one page from each of
FR-077's two sections, the search index, and `.nojekyll`. It then runs `pnpm validate`, which
walks every internal link in what was just built — a broken cross-reference is the defect a
documentation site actually has, and it costs a second to catch.

**Caching.** `pnpm/action-setup` followed by `actions/setup-node` with `cache: pnpm`, which caches
the pnpm store keyed on `pnpm-lock.yaml` — the standard pairing, and the order matters: setup-node
asks the pnpm binary for the store path, so pnpm has to exist first. The tree is 60 packages, so
the install is not the job's cost centre.

**One pnpm-specific precondition, or every `docs` job fails.** pnpm 11 does not run a dependency's
build script unless the project has approved it, and it treats an unapproved script as an
**error**, not a warning:

```text
[ERR_PNPM_IGNORED_BUILDS] Ignored build scripts: @docmd/engine-rust@0.9.4, esbuild@0.28.2
```

This stops the build outright, and it stops it identically in CI where there is no one to run the
interactive `pnpm approve-builds`. The approvals are therefore committed, in `pnpm-workspace.yaml`,
and both are decisions rather than boilerplate:

```yaml
allowBuilds:
  esbuild: true            # its postinstall places the platform binary docmd loads config with
  '@docmd/engine-rust': false
```

`@docmd/engine-rust` is docmd's optional native accelerator, and it is **denied**: building it
would put a Rust compile inside a job that installs Node and nothing else, to speed up a build that
already takes under two seconds on the JavaScript engine. A denial silences the error as
effectively as an approval, which is what makes an explicit `false` the right answer rather than an
approval nobody wanted.

Two details cost time if not written down: the file is `pnpm-workspace.yaml` even though this is a
single package and not a workspace, and the key is `allowBuilds` — pnpm 11 renamed it from pnpm
10's `onlyBuiltDependencies`, and it **no longer reads the `pnpm` field in `package.json` at all**,
which is what most existing guidance still tells you to edit. `pnpm install` writes the file as a
template with each package listed and unanswered; fill in the answers rather than hand-writing it.

**Rationale.** Building on pull requests is what stops the "the site is stale and nobody was told"
edge case: a broken page fails the change that broke it, not the deployment afterwards. One
version is the clarified requirement and it is also what keeps FR-083's drift check meaningful —
the check can only compare documentation against the code sitting beside it.

## R47: Showing the overlay on the front page (FR-070)

**Decision.** Two PNG screenshots, one per presentation, captured with `grim` from the E2E
harness's own nested instance so they show a controlled workspace set rather than the maintainer's
desktop, committed under `docs/assets/` and referenced from both the README and the site.

The images live in `docs/assets/` beside the pages that use them, so a reader browsing the folder
and the README both reach them by an ordinary relative path. The site build copies the directory
through to `site/assets/` and rewrites each reference to the page's own depth, so one committed
copy of each image serves the folder, the README and the published site alike.

**Rationale.** The one requirement in this feature that cannot be met with text. Capturing from
the nested instance means the images can be regenerated deterministically when the appearance
changes, and it keeps the maintainer's real windows and window titles out of the repository.

## R48: The documentation tree's shape (FR-076, FR-077, FR-084)

**Decision.** `docs/` **is** the documentation — plain Markdown, organised for a person reading
files, and containing nothing else:

```text
docs/
├── index.md            # front page
├── user/               # FR-077's first section
│   └── install.md  binds.md  configuration.md  styling.md  icons.md  troubleshooting.md
├── dev/                # FR-077's second section
│   └── architecture.md  workflow.md  testing.md  verification.md  releasing.md
└── assets/             # screenshots, referenced by the README too
```

The generator reads that tree from the repository root — `src: 'docs'` in `docmd.config.mjs` — and
owns no directory inside it. There is no framework to hide beneath the prose, because there is no
framework directory: the whole of it is `docmd.config.mjs`, `package.json`, `pnpm-lock.yaml` and
`pnpm-workspace.yaml` at the root, beside `Cargo.toml`, plus `scripts/docmd-include.mjs`.

**Rationale.** The requirement is that the tree read as well in an editor as on the published site,
and three properties deliver that, each verified by building them:

- **Plain `.md`.** Every page is Markdown an IDE previews and a forge renders. Nothing in this
  documentation needs a component model, and the one thing that might have forced one — the
  include — is a directive on its own line (R32).
- **Relative links between files resolve both ways.** A page written `[binds](./user/binds.md)`
  opens the neighbouring file in an editor, and the build rewrites it to `../binds/` in the
  published site. Verified in the output. This is the single property that makes "browse the
  folder" and "browse the site" the same act, so links between pages MUST be written in the
  relative-file form. Images behave the same way: `./assets/overlay-list.png` reaches the file
  from the folder and is rewritten to the page's own depth in the site.
- **Directory names are the sections.** `user/` and `dev/` are FR-077's two sections, and there
  are **no non-Markdown files among the documents at all** — the section titles and page order are
  the `navigation` list in `docmd.config.mjs`, which is one place rather than one per directory,
  and which `scripts/checks.sh` checks reaches every page that exists.

One page is pinned by the code: `user/binds.md` is `include_str!`d by `ui/shortcuts.rs`, which
asserts it quotes every bind line the program generates (FR-022b). That is a deliberate exception
to the freedom the rest of the tree has — it is the one page whose *content* must agree with the
binary, so it is the one page that cannot be renamed without a compile error saying so. The
assertion covers the bind lines only; the prose around them is as free as any other page's.

**Search costs nothing here**, which is worth recording because under the previous framework it
was this decision's one real price: plain Markdown pages carried no structured data, so the index
had to be built by hand. docmd takes plain Markdown as its input format, indexes it per section,
and picks up the text that `::include[]` pulls in without being told to — verified by finding
`gtk-icon-theme-name` and `grid_cell_width`, which appear only inside included contracts, in the
built index.

**Alternatives considered.** **A separate top-level `website/`** — the shape this plan carried
until the documentation tree was made the root; it split the documentation across two directories
and left `docs/` a stub. **A framework directory beneath `docs/`** — necessary when the framework
was an application with its own source tree, and pointless now that it is three files; hiding
three files under a dot-directory would only make them harder to find. **A `meta.json` per
section**, giving each its title and page order — the previous shape, and the only non-Markdown
files that were ever in `docs/`; one navigation list in the configuration says the same thing in
one place and can be checked against the tree.
