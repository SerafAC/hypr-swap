# Feature Specification: Open-Source Release Readiness

**Feature Branch**: `003-oss-release-readiness`

**Created**: 2026-08-30

**Status**: Draft

**Input**: User description: "Now I want to make this application \"opensource production ready\""

**Builds on**: `001-workspace-swap-overlay` and `002-overlay-visuals`. Requirement numbering
continues that sequence (FR-062 onward, SC-026 onward) so that FR references in code and documents
stay globally unique.

**Nature of this feature**: The switcher's behaviour is delivered. What is missing is everything
around it that a stranger needs in order to find, trust, install, configure, run, report on and
contribute to the project — and everything the maintainer needs to release it repeatably. This
feature adds no switching, ordering, icon or styling behaviour. The only additions to the running
program are what a user needs when something goes wrong: a record of its own lifecycle, and the
facts a bug report asks for.

## Clarifications

### Session 2026-08-30

- Q: Which distribution channels must a released version reach? → A: Tags and a changelog, plus
  prebuilt Debian and RPM packages and an AUR `PKGBUILD`. A workflow on the project's forge prepares
  the release: it raises the version number, and publishes the binaries into the forge's releases
  section. (Publishing to a language package registry is not wanted.)
- Q: Does "production ready" include operational hardening of the running daemon? → A: Logging only
  — errors for certain, plus the service starting and stopping. No verbosity control is wanted. No
  service-manager unit is wanted either: it is a Hyprland service and Hyprland starts it.
- Q: How should the documentation be organised? → A: Three tiers. The README is for the end user
  alone and answers six questions: what it is, what it is for, what it requires, how to install it,
  how to configure it, how to use it. A separate `DEVELOPMENT.md` covers development requirements,
  project setup, testing, running, and the basic architecture and project tree. A `docs/` directory
  holds the full documentation, split into an end-user section and a developer section, written in
  Markdown and built by a static documentation framework into a site hosted on the forge's pages.
  It must carry the full configuration specification and how to control themes and styles.
- Q: Must the end-to-end suite run in automated verification? → A: Yes, if at all possible —
  including a container image for testing and a workflow that runs it.
- Q: Where does the version line start for the first public release? → A: 1.0.0 — semantic
  versioning applies in full from day one, the documented contracts are declared stable, and a
  breaking change to any of them means 2.0.0.
- Q: What do the published packages target? → A: `x86_64` only. The Debian package is built
  against the oldest still-supported Debian/Ubuntu LTS and the RPM against the oldest
  still-supported Fedora, so each runs across that family's current releases; the Arch recipe
  builds from source and covers Arch on whatever the user has.
- Q: What does the public repository contain? → A: The tree as it stands — source, `specs/`,
  `.specify/`, `.claude/`, `CLAUDE.md` and the full history. The specifications stay
  authoritative for requirements and contracts; the documentation site's end-user section is
  authoritative for how to use the program.
- Q: How do changelog entries come into being? → A: Written by hand in the Keep a Changelog
  form. Every change adds to an `[Unreleased]` section; the release workflow renames it to the
  new version and date. No commit-message convention and no commit linting.
- Q: Is the documentation site versioned per release? → A: No — one version, built from the
  default branch, stating which release it documents and marking anything not yet released.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A stranger installs and runs it without building it (Priority: P1)

Someone running Hyprland hears about the project and lands on its page. Within a minute they know
what it is, what it is for, and whether it will work on their system. They install a released
package for their distribution — no toolchain, no build — add two lines to their Hyprland
configuration, and have a working overlay. When something does not work, the page they are already
reading tells them what to check.

**Why this priority**: A project nobody can install is not open source in any useful sense. Every
other journey here — contributor, packager, bug reporter — begins as someone who got it running.

**Independent Test**: On a clean system of each supported packaging family, with no development
toolchain installed, follow only the published installation instructions and reach a working
overlay.

**Acceptance Scenarios**:

1. **Given** a clean supported system with no build toolchain, **When** the user installs the
   published package for their distribution family, **Then** they obtain a working binary without
   compiling anything and without consulting the source or the maintainer.
2. **Given** the installed package, **When** the user follows the published setup, **Then** two
   bind lines and a session start produce a working overlay on the first attempt.
3. **Given** the project's front page, **When** a prospective user reads it, **Then** they can state
   what the program does, its purpose, the supported compositor version range, the required system
   libraries, the optional dependencies and what degrades without each, and the licence — without
   following a link away from that page.
4. **Given** a system missing an optional dependency, **When** the user runs anyway, **Then** the
   documentation told them in advance what degrades, and the program's behaviour matches it.
5. **Given** an unsupported compositor version, **When** the program starts, **Then** the published
   requirements state the supported range and the program's own diagnostics name the mismatch
   rather than failing obscurely.
6. **Given** a running program, **When** the user asks which version it is, **Then** it reports a
   version matching a published release.

---

### User Story 2 - Everything is documented, at the depth the reader needs (Priority: P2)

Three readers arrive with three different needs. A user wants to install, configure and use it, and
finds exactly that on the front page with nothing about compiling in the way. A developer wants to
build and test it, and finds a development document that gets them productive. Anyone wanting the
complete account — every configuration key, every style value, every contract, the architecture —
finds a browsable documentation site, split into a user half and a developer half, with the full
configuration and theming reference in it.

**Why this priority**: The documentation is the product for everyone who has not yet run the
program, and it is what stops the maintainer being the only route to an answer. It ranks
immediately after installation because an installed program nobody can configure is barely better
than one nobody can install.

**Independent Test**: Confirm each of the three readers is served by exactly one document and finds
their answer without reading the other two; then confirm a person can assemble a complete custom
overlay appearance using only the published site.

**Acceptance Scenarios**:

1. **Given** the README, **When** an end user reads it, **Then** it answers what it is, what it is
   for, what it requires, how to install it, how to configure it and how to use it — and contains
   no development instructions.
2. **Given** the development document, **When** a developer new to the project reads it, **Then**
   they can install the development dependencies, build, run the program, run every test tier, and
   describe the project's architecture and the purpose of every top-level directory and source
   module.
3. **Given** the documentation site, **When** a reader opens it, **Then** it is split into a user
   section and a developer section, and each is browsable without reading the other.
4. **Given** the documentation site's configuration reference, **When** a user consults it, **Then**
   every accepted setting appears with its accepted values, its valid range, its default and its
   precedence, and the theming account is sufficient to assemble a complete custom appearance
   without reading source.
5. **Given** a change to what the program accepts, **When** the documentation is not updated to
   match, **Then** an automated check fails — the published reference cannot drift from the
   program's actual behaviour.
6. **Given** any question a reader might have, **When** they look it up, **Then** exactly one
   document answers it and the others link to it — no answer appears twice and no two answers
   disagree.
7. **Given** a change lands on the default branch, **When** the documentation site is rebuilt,
   **Then** it publishes automatically without a manual step.

---

### User Story 3 - Every proposed change is automatically verified, overlay behaviour included (Priority: P3)

A contributor opens a change. Without anyone doing anything, the project builds it, runs the unit
suite, checks lint and formatting, builds it on the minimum supported toolchain, and — in a
reproducible container carrying a compositor — runs the end-to-end suite that drives the real
overlay. A plain pass or fail lands on the change. A maintainer only spends attention on changes
that already pass.

**Why this priority**: Automated verification is what makes outside contribution safe to accept,
and it enforces the constitution's testing principles mechanically rather than by reviewer memory.
Running the end-to-end tier in automation is what makes it cover the overlay's actual behaviour
rather than only its pure logic.

**Independent Test**: Open a deliberately broken change of each gated kind — a failing unit test, a
formatting violation, a lint warning, a build failure on the minimum toolchain, and a regression
that only the end-to-end suite catches — and confirm each is caught without human involvement.

**Acceptance Scenarios**:

1. **Given** a proposed change, **When** it is submitted, **Then** checks run without a maintainer
   triggering them and report one unambiguous pass/fail verdict.
2. **Given** a change that breaks a unit test, violates formatting, trips a lint rule, or fails to
   build on the declared minimum toolchain, **When** the checks run, **Then** they fail for that
   specific reason and name the local command that reproduces it.
3. **Given** a change that regresses overlay behaviour without breaking a unit test, **When** the
   checks run, **Then** the end-to-end suite runs against a compositor supplied by automation and
   catches it.
4. **Given** an end-to-end failure in automation, **When** a contributor wants to investigate,
   **Then** they can run the same suite locally in the same published container image and reproduce
   it.
5. **Given** automation is unavailable or a check is known-flaky, **When** a change is considered,
   **Then** which checks gate a merge and which merely inform is explicit, so that a check cannot
   silently stop gating or silently start.
6. **Given** the check results, **When** a maintainer reviews, **Then** they can see which
   requirements were exercised by what ran, and which were not exercised at all.

---

### User Story 4 - A maintainer cuts a release by triggering one procedure (Priority: P4)

The maintainer decides a release is due. They trigger the project's release workflow and name the
new version. It raises the version everywhere the version is recorded, closes the changelog entry,
tags the source, builds the binary and the distribution packages, and publishes them with their
integrity values into the releases section. A user or a packager can then obtain that exact version
and verify it. Nothing about the procedure lives only in the maintainer's head.

**Why this priority**: Without a repeatable release, users get told to build the default branch,
which makes bug reports untraceable and packaging impossible. It follows verification because there
must be a way to know a release is sound before publishing it.

**Independent Test**: Trigger the release workflow for a version, then confirm an independent person
can obtain that version's package, verify its integrity, install it on a clean system, and read what
changed in it.

**Acceptance Scenarios**:

1. **Given** the release workflow, **When** the maintainer triggers it with a version, **Then** the
   version is raised wherever it is recorded, the source is tagged, and the artefacts are built and
   published without further manual steps.
2. **Given** a published release, **When** anyone inspects it, **Then** the version the program
   reports at runtime, the source tag, and the changelog entry all agree.
3. **Given** a published release, **When** a user or packager obtains an artefact, **Then** they can
   verify its integrity against a published value.
4. **Given** a published release, **When** a user of a Debian-family or RPM-family distribution
   installs its package, **Then** it installs the program, its licence and its documentation to
   conventional locations, declares its runtime dependencies, and runs on a clean system of that
   family.
5. **Given** a published release, **When** an Arch user builds from the project's packaging recipe,
   **Then** it produces an installable package from the released source without the maintainer's
   help.
6. **Given** the working tree does not match the tag, the tag already exists, or the verification
   checks are not green, **When** a release is attempted, **Then** it fails rather than publishing
   something unreproducible.
7. **Given** two consecutive releases, **When** a user reads the changelog, **Then** they learn what
   they can now do, what changed, and whether any configuration key, shortcut or documented
   behaviour they rely on has changed meaning.

---

### User Story 5 - A contributor knows how to take part (Priority: P5)

Someone wants to fix a bug or add something. They find, without asking, what the project's rules
are, how the spec-driven workflow applies to their change, what a good change carries with it, and
what happens after they submit. Someone hitting a bug files it through a form that already asks for
the environment facts the maintainer would otherwise have to request.

**Why this priority**: Contribution guidance turns willingness into merged changes. It ranks below
automated verification because guidance without enforcement produces changes nobody can safely
merge, and below documentation because it builds on the development document.

**Independent Test**: A developer new to the project produces a change that passes every check and
carries its tests, documentation, changelog entry and specification updates, using only the
published contribution guidance.

**Acceptance Scenarios**:

1. **Given** the contribution guidance, **When** a newcomer reads it, **Then** it states the design
   rules the project holds to, how the spec-driven workflow applies to a behavioural change, and
   which documents such a change must update alongside the code.
2. **Given** a contributor without a live compositor, **When** they consult the guidance, **Then**
   it tells them which tier they cannot run locally, how to run it in the published container
   instead, and what automation will verify on their behalf.
3. **Given** a user hitting a bug, **When** they open a report, **Then** they are prompted for the
   program version, the compositor version, the configuration and the diagnostic output, and the
   report contains enough to act on without a follow-up round trip.
4. **Given** someone proposing a feature, **When** they open a request, **Then** they are asked what
   they are trying to achieve rather than the change they have designed, and the project's stated
   scope boundaries are shown at the point of asking.
5. **Given** a contributor submits a change, **When** it is received, **Then** the published
   expectations tell them what review will look for and that the project is maintained on a
   best-effort basis.
6. **Given** any participant, **When** they read the conduct expectations, **Then** they find them
   stated, with where to report a violation.

---

### User Story 6 - Provenance and licensing are unambiguous (Priority: P6)

A packager, a legal review, or a curious user needs to know exactly what they may do with the code
and what third-party material travels inside it. They find one authoritative licence text, the
copyright holder, and an account of every bundled or vendored third-party component with its own
licence.

**Why this priority**: Ambiguous licensing blocks packaging and adoption in exactly the audiences
most likely to carry the project further, and it is cheap to fix once. It ranks here because the
licence is already declared in package metadata; what is missing is the authoritative text and the
third-party account.

**Independent Test**: Given only the distributed source, a reviewer enumerates every third-party
component shipping inside it, names each licence, and finds the full licence text of the project.

**Acceptance Scenarios**:

1. **Given** the distributed source, **When** a reviewer looks for the licence, **Then** the full
   text is present at a conventional top-level location, names the copyright holder and year, and
   matches the licence declared in package metadata.
2. **Given** the vendored compositor protocol description and the bundled placeholder artwork,
   **When** a reviewer inspects them, **Then** each carries its upstream origin, its revision, and
   its own licence, and the project's licensing does not obscure them.
3. **Given** a redistribution — a distribution package or a fork — **When** it is assembled, **Then**
   the obligations of every bundled component are satisfiable from what the source tree contains,
   and the packages ship the licence text.

---

### User Story 7 - The daemon leaves a record of itself (Priority: P7)

The user starts the daemon from their Hyprland configuration and forgets about it. Later something
is wrong: it is not running, or it stopped, or a shortcut does nothing. They look at where their
compositor collects its output and find a record of the daemon starting, with its version; a record
of every error it reported; and, if it stopped, a record of why. They paste that into a bug report.

**Why this priority**: A daemon that dies without saying why is unsupportable, and every bug report
depends on this record existing. It ranks last among the delivery stories because it is the smallest
change and the one an existing user is least likely to notice.

**Independent Test**: Start the daemon through the compositor, exercise it, terminate it, and
confirm the session's collected output contains a start record with the version, every error
reported in between, and a shutdown record naming the cause.

**Acceptance Scenarios**:

1. **Given** the daemon starts successfully, **When** it becomes ready, **Then** it records that it
   started and which version it is.
2. **Given** the daemon is asked to stop, **When** it shuts down, **Then** it records that it is
   stopping and what caused it — a signal, or a fatal start-up condition.
3. **Given** any condition the program already reports, **When** it occurs, **Then** it still
   appears in that same record, at its existing level and in the existing format.
4. **Given** the daemon is started from the compositor's configuration, **When** the user wants its
   output, **Then** the documentation tells them exactly where the compositor collects it and how to
   retrieve it.
5. **Given** a user filing a bug, **When** the form asks for environment facts, **Then** the program
   can produce them on demand rather than the user assembling them by hand.
6. **Given** a configuration file written for an earlier released version, **When** a newer version
   starts, **Then** it either behaves identically or reports which setting changed meaning; a
   setting the user wrote is never silently reinterpreted.

---

### User Story 8 - Vulnerabilities and dependency rot have a path (Priority: P8)

Someone finds a security-relevant defect and wants to report it responsibly. They find a private
channel and know which versions are supported. Separately, the maintainer learns about a vulnerable
dependency from the project's own checks rather than from a user.

**Why this priority**: The attack surface is genuinely small — a session-local daemon with no
network access, no elevated privileges and no secrets — so this is a reporting channel and a
dependency watch, not a threat-modelling exercise. It ranks last for that reason, but it is table
stakes for asking strangers to run the program.

**Independent Test**: Confirm the reporting channel and supported-version statement are published
and reachable, and that the dependency check surfaces a deliberately introduced vulnerable
dependency.

**Acceptance Scenarios**:

1. **Given** a person with a security-relevant finding, **When** they look for how to report it,
   **Then** they find a private channel distinct from the public tracker, and an expected
   acknowledgement time.
2. **Given** the published policy, **When** a user asks whether their version still receives fixes,
   **Then** the supported-version statement answers it.
3. **Given** a dependency with a published advisory, **When** the checks run, **Then** the advisory
   is surfaced to the maintainer, and one with no available fix can be accepted in a recorded,
   time-bounded way rather than leaving the project permanently failing.
4. **Given** a privacy-conscious reader, **When** they read the front page, **Then** the project
   states plainly that it performs no network access, collects no telemetry, and reads nothing
   beyond the compositor's state, the user's configuration and the desktop's icon files.

---

### Edge Cases

- **A contributor has no Hyprland session** (a container, another desktop): which tiers can they run,
  what must they delegate to automation, and does the published image let them run the rest?
- **The end-to-end suite cannot start a compositor in automation** — the image drifts, the
  compositor version moves, the runner lacks what it needs: is that distinguishable from a genuine
  test failure, and does it block every merge until fixed?
- **A change touches only documentation or specifications**: do the full checks still apply, and is
  a changelog entry still required?
- **The documentation site fails to build** after a merge: is the published site left stale, or
  broken, and is anyone told?
- **The published configuration reference and the program's accepted values diverge**: which one is
  authoritative, and what catches the divergence?
- **A release is attempted from a tree that does not match the tag**, the tag already exists, or the
  gating checks are not green: the procedure must refuse rather than publish.
- **A release workflow fails halfway** — the tag pushed, the packages not published: can it be
  re-run without producing two different artefacts for one version?
- **A user builds from the default branch** rather than a release: the version they report must still
  identify the exact source it came from.
- **A distribution package's declared dependencies do not match a target release** of that
  distribution: what does the project promise, and to which distribution versions?
- **The AUR recipe falls behind the released version**: what keeps them in step?
- **A dependency advisory appears with no fix available**: a reasoned, time-bounded acceptance must
  be possible without the project sitting permanently red.
- **A configuration setting is renamed or removed** between releases: the user's file must not be
  silently reinterpreted.
- **A bug report arrives with no environment information**: the form must have made that hard.
- **The daemon dies at start-up before it can log** (no compositor, no environment): the cause must
  still reach the record, since exit code alone does not identify it.

## Requirements *(mandatory)*

### Functional Requirements

Numbering continues features 001 and 002 (FR-062 onward).

#### Licensing and provenance

- **FR-062**: The distributed source MUST contain the project's full licence text at a conventional
  top-level location, naming the copyright holder and year, and it MUST match the licence declared
  in the project's package metadata.
- **FR-063**: Every third-party component shipping inside the source tree — at minimum the vendored
  compositor protocol description and the bundled placeholder artwork — MUST be attributable from
  the source tree alone: its upstream origin, its version or revision, and its own licence.
- **FR-064**: The project MUST state its position on the licences of its build-time and runtime
  dependencies, such that a packager can judge redistributability without auditing the dependency
  graph themselves.
- **FR-065**: Package metadata MUST carry what a packager and a source index expect: description,
  licence, repository location, documentation location, and the topics under which the project
  should be found.
- **FR-066**: Every distributed package MUST ship the project's licence text alongside the binary.
- **FR-066a**: Before the repository is first made public, its entire history MUST be reviewed for
  credentials, personal data, and material the project has no right to publish, and the review's
  outcome MUST be recorded.

#### The README — the end user's document

- **FR-067**: The README MUST answer exactly six questions, in this order: what the program is; what
  its purpose is; what it requires; how to install it; how to configure it; how to use it.
- **FR-068**: The README MUST NOT carry development instructions — building from source for
  development, test invocation, architecture, or contribution mechanics belong to FR-072 and
  FR-094 and the developer documentation, and the README MUST link to them rather than restate
  them.
- **FR-069**: The README's requirements section MUST state the supported compositor version range,
  the minimum language toolchain version needed to build, the required system libraries, and each
  optional dependency together with what degrades without it.
- **FR-070**: The README MUST show the overlay visually in both of its presentations, so a
  prospective user sees what they are installing before installing it.
- **FR-071**: The README MUST state the project's scope boundaries — Hyprland on Wayland only, and
  what the project deliberately does not do — and MUST state plainly that the program performs no
  network access, collects no telemetry, and reads nothing beyond the compositor's state, the user's
  configuration file and the desktop's icon files.

#### The development document

- **FR-072**: The project MUST publish a development document covering: the development
  requirements; how to set the project up and install its dependencies; how to run every test tier;
  how to run the program; and the project's basic architecture together with a description of its
  tree.
- **FR-073**: The development document's architecture section MUST convey the project's organising
  seam — pure decision logic separated from the thin input/output shell — and state where a new
  decision rule belongs, so that a contributor's first change lands in the right module.
- **FR-074**: The development document's tree description MUST name every top-level directory and
  every source module with its responsibility.
- **FR-075**: The development document MUST state, for each test tier, what it requires — in
  particular which tier needs a live compositor, and how a contributor without one runs it in the
  project's published container image instead.

#### The full documentation site

- **FR-076**: The project MUST publish its full documentation as a browsable site, authored as
  Markdown inside the repository, built by a static documentation framework, and hosted on the
  forge's pages.
- **FR-077**: The site MUST be divided into an end-user section and a developer section, each
  navigable without reading the other.
- **FR-078**: The site MUST be rebuilt and republished automatically when a change lands on the
  default branch, with no manual publication step, and a build failure MUST be reported rather than
  silently leaving the site stale.
- **FR-078a**: The site MUST document one version only — the default branch — and MUST state which
  release it corresponds to and mark any documented behaviour that is not yet in a release, so a
  reader on the current release is never misled. Per-release snapshots MUST NOT be published, since
  FR-083's check can only verify documentation against the code it ships with.
- **FR-079**: The end-user section MUST contain the complete configuration specification: every
  accepted setting, its accepted values, its valid range, its default, and the precedence between
  overrides, themes and defaults.
- **FR-080**: The end-user section MUST contain a dedicated account of controlling themes and
  styles, sufficient for a reader to assemble a complete custom appearance without reading source.
- **FR-081**: The end-user section MUST additionally cover: installation for each published channel,
  binding the shortcuts, the fixed in-overlay keys, the presentations and ordering modes, program
  icons and icon sets, retrieving the program's diagnostics, and a troubleshooting account covering
  at minimum shortcuts not firing, the overlay not appearing, the daemon exiting at start-up,
  missing or wrong icons, and a second instance already running — each tied to the diagnostic the
  program actually emits.
- **FR-082**: The developer section MUST cover the architecture in full, the spec-driven workflow,
  the project's external contracts, every test tier including the end-to-end harness, and the
  release procedure.
- **FR-083**: The published configuration and style reference MUST be verified against the values
  the program actually accepts by an automated check, so that documentation cannot drift from
  behaviour.
- **FR-084**: Each question a reader may have MUST be answered authoritatively in exactly one
  document, with the others linking to it; no answer may appear in two places where the two can
  diverge.
- **FR-084a**: The feature specifications and contracts under `specs/` remain authoritative for the
  project's requirements and for its external contracts; the site's end-user section is authoritative
  for how to use the program, and the developer section MUST link to the specifications rather than
  restate them.
- **FR-084b**: The published repository MUST retain the project's development record — the
  constitution, the feature specifications with their plans, research and contracts, and the agent
  instructions — so that the spec-driven workflow FR-095 asks contributors to follow is inspectable
  rather than merely described.

#### Automated verification

- **FR-085**: Every proposed change and every push to the default branch MUST trigger automated
  checks without a maintainer acting, and MUST report one unambiguous pass/fail verdict.
- **FR-086**: Automated checks MUST include, at minimum: a release-profile build, the full
  compositor-free unit suite, the project's lint rules at the strictness the project already
  enforces, and the project's formatting rules.
- **FR-087**: Automated checks MUST verify the declared minimum language toolchain version by
  building against it, so the declared minimum cannot silently drift.
- **FR-088**: Automated checks MUST run the end-to-end suite against a compositor supplied by
  automation, so that overlay behaviour — not only pure logic — is verified before merge.
- **FR-089**: The container image that supplies that compositor MUST be defined in the repository
  and MUST be usable by a contributor locally, so that an end-to-end failure seen in automation can
  be reproduced without automation.
- **FR-090**: A failing check MUST name what failed in terms a contributor can act on, and MUST name
  the local command that reproduces it.
- **FR-091**: The checks that gate a merge MUST be distinguishable from those that merely inform, so
  that an unavailable provider or a known-flaky check cannot silently become a gate or silently stop
  being one.
- **FR-092**: The project MUST publish which requirements are verified by which tier, and that
  statement MUST cover every requirement — including those verified only by inspection.
- **FR-093**: Automated checks MUST surface published advisories affecting the project's
  dependencies to the maintainer, and MUST permit a reasoned, recorded, time-bounded acceptance of an
  advisory with no available fix without leaving the project permanently failing.

#### Contribution process

- **FR-094**: The project MUST publish contribution guidance covering the design rules it holds to,
  how the spec-driven workflow applies to a behavioural change, and what review will examine —
  referring to the development document for setup rather than restating it.
- **FR-095**: The contribution guidance MUST state the convention that code cites requirement
  numbers, and that a behavioural change updates the feature specification, the plan's coverage
  table and the task list alongside the code.
- **FR-096**: The project MUST publish its conduct expectations and where to report a violation.
- **FR-097**: Bug reports MUST be collected through a form requiring the program version, the
  compositor version, the relevant configuration, the diagnostic output, and the expected versus
  observed behaviour.
- **FR-098**: Feature requests MUST be collected through a form that asks what the user is trying to
  achieve rather than the change they have designed, and that shows the project's scope boundaries
  at the point of asking.
- **FR-099**: Proposed changes MUST be accompanied by a stated checklist covering tests,
  documentation, changelog and specification updates.
- **FR-100**: The project MUST state what response a contributor should expect, including that it is
  maintained on a best-effort basis.

#### Versioning and releases

- **FR-101**: The project MUST follow semantic versioning and MUST begin its public history at
  **1.0.0**, declaring its documented contracts stable rather than releasing under a 0.x line.
- **FR-101a**: The versioning policy MUST define a breaking change in terms of the project's own
  user-facing contracts — configuration keys and accepted values, shortcut names, in-overlay keys,
  command-line flags, exit codes, and diagnostic subjects — and a change to any of them MUST raise
  the major version.
- **FR-102**: The project MUST maintain a changelog written for users — what they can now do, what
  changed, what broke — with an entry for every released version. Entries MUST be written by hand in
  the Keep a Changelog form and MUST NOT be derived from commit messages.
- **FR-102a**: The changelog MUST carry an `[Unreleased]` section that every change adds to as it
  lands, which the release workflow renames to the released version and date; a change that alters
  what a user can do MUST NOT merge without its entry.
- **FR-103**: Every release MUST be identified by a tag in the source history, and the version the
  program reports at runtime MUST match that tag and its changelog entry.
- **FR-104**: A build made from a source snapshot that is not a released version MUST still report a
  version identifying the exact source it was built from.
- **FR-105**: Releasing MUST be performed by a workflow the maintainer triggers with the intended
  version, which raises the version wherever it is recorded, closes the changelog entry, tags the
  source, builds the artefacts and publishes them — with no manual step beyond choosing the version.
- **FR-106**: A release MUST publish, into the forge's releases section: the source archive, a
  prebuilt `x86_64` binary, a Debian-family package and an RPM-family package, both for `x86_64`.
- **FR-107**: The project MUST maintain an Arch packaging recipe that builds an installable package
  from a published release, and it MUST be updated in step with the released version.
- **FR-108**: Every published artefact MUST be verifiable against a published integrity value.
- **FR-109**: Each distribution package MUST install the binary, its licence and its documentation to
  that family's conventional locations, MUST declare its runtime dependencies, and MUST run on a
  clean system of that family with no further manual dependency work.
- **FR-109a**: Each distribution package MUST be built against the oldest still-supported release of
  its family — a Debian/Ubuntu LTS for the Debian package, the oldest supported Fedora for the RPM —
  so that one package runs across that family's currently supported releases, and the project MUST
  publish which releases of each family the package is verified against.
- **FR-110**: A release MUST fail rather than publish when the working tree does not match the tag,
  when the tag already exists, or when the gating checks are not green; and re-running it after a
  partial failure MUST NOT produce a second, different set of artefacts for the same version.
- **FR-111**: Each release MUST carry what a distribution packager needs to build it without
  contacting the maintainer: the source archive, the declared dependencies with minimum versions, the
  build steps, and the files to install and where.

#### The daemon's own record

- **FR-112**: The daemon MUST record that it has started, and the version it is, once start-up
  completes.
- **FR-113**: The daemon MUST record that it is stopping, and what caused it — a termination signal,
  or a fatal start-up condition — including when it exits before it is fully started.
- **FR-114**: The existing diagnostic levels, format and notification policy MUST be preserved
  unchanged; the lifecycle records of FR-112 and FR-113 join the existing record rather than
  replacing it, and no verbosity or log-level setting is introduced.
- **FR-115**: The documentation MUST state where the compositor collects the daemon's output when it
  is started from the compositor's configuration, and how a user retrieves it.
- **FR-116**: The program MUST be able to report, on demand, the version and environment facts the
  bug report form asks for, so a reporter need not assemble them by hand.
- **FR-117**: A configuration file written for an earlier released version MUST either behave
  identically or produce a diagnostic naming the setting whose meaning changed; a setting the user
  wrote MUST never be silently reinterpreted.
- **FR-118**: The program MUST report a diagnostic naming the mismatch when it runs against a
  compositor version outside the supported range, rather than failing obscurely.

#### Security and maintenance

- **FR-119**: The project MUST publish a private security reporting channel, distinct from the
  public tracker, with an expected acknowledgement time.
- **FR-120**: The project MUST publish which released versions receive fixes.
- **FR-121**: The project MUST state its dependency policy — that a new dependency requires written
  justification, consistent with the constitution's simplicity principle — so a contributor
  proposing one knows the bar in advance.

### Key Entities

- **Release**: A named, tagged, immutable point in the project's history, carrying a version number,
  a changelog entry, a set of verifiable artefacts, and a support status.
- **Artefact**: One published file belonging to a release — source archive, binary, Debian package,
  RPM package — with its integrity value.
- **Packaging recipe**: A build description maintained in step with the released version, from which
  a distribution's users build an installable package themselves.
- **Changelog entry**: The user-facing account of one release: what was added, changed, fixed, and
  what broke.
- **Documentation section**: One half of the published site — end user or developer — each with its
  own audience, navigation and authoritative scope.
- **Automated check run**: The verdict for one proposed change, composed of individual checks, each
  either gating or informational.
- **Test environment image**: The reproducible container carrying a compositor, used identically by
  automation and by a contributor locally.
- **Contribution**: A proposed change carrying its code, tests, documentation, changelog entry and
  specification updates, with the checklist asserting each is present.
- **Bug report**: A structured account of an observed failure, carrying the environment facts needed
  to act on it.
- **Third-party component**: Something shipping inside the source tree that originates elsewhere,
  carrying its origin, revision and licence.
- **Supported version range**: Which released versions receive fixes, and which compositor and
  toolchain versions the project is verified against.

## Success Criteria *(mandatory)*

Numbering continues features 001 and 002 (SC-026 onward).

### Measurable Outcomes

- **SC-026**: A person who has never seen the project goes from landing on its page to a working
  overlay in under 15 minutes, using only what is published and asking the maintainer nothing.
- **SC-027**: On a clean system with no development toolchain, installing a published package and
  reaching a working overlay takes under 5 minutes and requires no compilation.
- **SC-028**: A reader can state the project's licence, its supported compositor range, its
  requirements, and how to report a vulnerability within 60 seconds of arriving.
- **SC-029**: The README answers all six end-user questions and contains zero development-only
  instructions.
- **SC-030**: 100% of the settings the program accepts appear in the published configuration
  reference with their accepted values, range and default — verified automatically, with zero drift
  possible between the documentation and the program.
- **SC-031**: A user assembles a complete custom overlay appearance — colours, font and dimensions —
  using only the published theming documentation, without reading source.
- **SC-032**: A developer new to the project completes setup and runs every test tier in under 30
  minutes using only the development document.
- **SC-033**: 100% of proposed changes receive an automated pass/fail verdict without maintainer
  action, and that verdict arrives within 30 minutes of submission.
- **SC-034**: A deliberately introduced failure of each gated kind — broken unit test, formatting
  violation, lint warning, minimum-toolchain build failure, and an overlay behaviour regression
  caught only end to end — is caught by automation in 100% of cases.
- **SC-035**: A contributor reproduces an end-to-end failure seen in automation on their own machine,
  using the published image, in under 15 minutes.
- **SC-036**: 100% of requirements have an explicitly published verification tier; none has an
  unknown coverage status.
- **SC-037**: A release is produced by triggering the workflow, with the maintainer performing zero
  manual steps beyond naming the version.
- **SC-038**: For 100% of releases, the runtime version, the source tag and the changelog entry
  agree, and every published artefact verifies against its integrity value.
- **SC-039**: Each published distribution package installs and runs with no additional manual
  dependency work on a clean system of both the oldest and the current supported release of its
  family — verified for 100% of published packages.
- **SC-040**: 100% of bug reports arriving through the published form contain the program version,
  the compositor version and the diagnostic output, requiring zero follow-up requests for basic
  environment facts.
- **SC-041**: 100% of third-party components shipping inside the source tree are attributable to
  their origin and licence from the source tree alone; zero unattributed files.
- **SC-042**: For 100% of daemon runs, the session's collected output contains a start record naming
  the version, every error reported during the run, and a shutdown record naming its cause; zero
  runs end without a record of why.
- **SC-043**: A configuration file written for the previous release produces either identical
  behaviour or an explicit diagnostic on the current one; zero cases of a written setting silently
  changing meaning.

## Assumptions

- **The application's behaviour is complete and is not revisited here.** Features 001 and 002 stand
  as delivered. This feature adds no switching, ordering, icon or styling behaviour; the only
  additions to the running program are the lifecycle records, the environment report, and the two
  diagnostics of FR-117 and FR-118.
- **The project will be hosted publicly on GitHub**, whose issue tracking, pull requests, workflow
  automation, releases and pages hosting are what the requirements above assume. There is no remote
  configured today; establishing it is part of this work.
- **The repository is published whole**, including its development scaffolding and its full history.
  Nothing is rewritten or split out; the only pre-publication action on the history is the review of
  FR-066a. This also preserves the existing unit test that walks
  `specs/002-overlay-visuals/contracts/style-values.md` as the single source of truth for style
  values, which FR-083's documentation check builds on.
- **Publishing to a language package registry is out of scope**, per the clarification: the release
  channels are the forge's releases section, the Debian and RPM packages, and the Arch recipe.
- **The licence is MIT**, as already declared in package metadata; this feature adds the
  authoritative text and the third-party account rather than choosing a licence.
- **The daemon is started by Hyprland**, from the user's compositor configuration. No service manager
  unit is provided, and the daemon's output is collected wherever the compositor collects it.
- **No verbosity control is introduced.** The existing three diagnostic levels and their fixed
  notification policy stand; a log-level setting would be speculative configuration and is
  explicitly not wanted.
- **The end-to-end suite can be made to run in automation.** The harness today nests inside a live
  session; running it in a container means supplying a compositor there instead. This is treated as
  achievable, and the plan must establish how — it is the largest technical unknown in this feature.
- **The project is maintained by a single maintainer on a best-effort basis.** Governance stays
  lightweight: no elected roles, no service-level guarantees — but the expectation is stated rather
  than left to be discovered.
- **The target audience is Linux desktop users running Hyprland.** No other compositor, platform or
  architecture is promised, and the documentation says so rather than leaving it implied.
- **The existing published contracts are the versioning surface, and 1.0.0 declares them stable.**
  The configuration schema, shortcut names, CLI, exit codes and diagnostic subjects already
  documented under `specs/*/contracts/` are what FR-101a's policy makes promises about, and the
  documentation site presents them to users rather than replacing them. The version recorded in
  package metadata today (`0.1.0`) is pre-publication and is raised to `1.0.0` by the first run of
  the release workflow.
- **The security surface is genuinely small** — a session-local daemon with no network access, no
  elevated privileges and no secrets — so the security work is a reporting channel, a supported
  version statement and a dependency watch, not a threat-modelling exercise.

## Out of Scope

- Any new switcher, ordering, icon or theming behaviour.
- Publishing to a language package registry.
- A service-manager unit, or supervision outside the compositor's own start of the daemon.
- A log-level or verbosity setting.
- A commit-message convention, commit linting, or any changelog or version derived from the commit
  trail.
- Versioned documentation: per-release snapshots of the site, or a version picker.
- Support for compositors other than Hyprland, or for non-Wayland sessions.
- Architectures other than `x86_64` for the prebuilt binary and packages; other architectures are
  served by the Arch recipe or by building from source.
- Distributions outside the Debian, RPM and Arch families.
- Translation or localisation of the documentation or the program's diagnostics.
- A graphical configuration tool.
- Multi-maintainer governance, funding, or trademark arrangements.
- Rewriting the delivered feature specifications; they remain the authority on the behaviour they
  describe.
