# Phase 1 Data Model: Open-Source Release Readiness

**Feature**: `003-oss-release-readiness` | **Date**: 2026-08-30

This feature's entities are mostly *repository* entities — a release, an artefact, a changelog
entry — which have no in-memory representation at all: their schema is the shape of a file, a tag
or a workflow input. They are modelled here because the requirements constrain them and because
the release workflow validates them. The three entities that do exist in the program are listed
first.

## In the program

### `Condition` (extended — `src/diag.rs`)

Three variants join the existing enum. The enum remains the single home of the notification policy
(FR-114); nothing else about it changes.

| Variant | Level | Notifies | Subject | Requirement |
|---|---|---|---|---|
| `Started` | `Info` | no | `daemon` | FR-112 |
| `Stopping` | `Info` | no | `daemon` | FR-113 |
| `CompositorVersionUnsupported` | `Warn` | no | `compositor` | FR-118 |

None notifies: a daemon starting or stopping is not something the user must act on (FR-030's
test), and a version warning is a record for a bug report rather than an interruption. Exact
message forms are in [contracts/diagnostics.md](./contracts/diagnostics.md).

### `CompositorVersion` (new, pure — `src/model.rs`)

Deserialised from Hyprland's `j/version` response, of which exactly two fields are read.

| Field | Type | Source | Notes |
|---|---|---|---|
| `version` | `String` | `version` | e.g. `0.56.2` — **[verified]** shape against the running compositor |
| `tag` | `Option<String>` | `tag` | e.g. `v0.56.2`; carried for the environment report only |

Derived, by pure functions unit-tested in-module:

- `parse(&str) -> Option<(u32, u32, u32)>` — accepts `MAJOR.MINOR[.PATCH]` with an optional `v`
  prefix and ignores any trailing suffix; `None` for anything else.
- `supported(&self) -> Support` — `Supported`, `TooOld { found, minimum }`, or `Unknown { found }`.

`SUPPORTED_HYPRLAND` in `src/lib.rs` is the single definition of the range (minimum `0.55`,
no maximum), and is what the README, the site's requirements page and the diagnostic all state.

### `EnvironmentReport` (new — assembled in `src/main.rs`, printed by `--environment`)

Not a stored type; a block of lines written to stdout. Its field list is a contract because the
bug report form asks for it verbatim ([contracts/cli.md](./contracts/cli.md)).

| Line | Value | Absent when |
|---|---|---|
| `hypr-swap` | version, including the build suffix of R37 | never |
| `hyprland` | `version` (and `tag`) from `j/version` | not reachable → `unavailable` |
| `config` | resolved path, and whether it exists | never |
| `settings` | only those differing from their defaults, `key = value` | none differ → `defaults` |
| `icon-set` | the set actually resolved, or `none` | never |
| `notify-send` | `present` / `absent` | never |

The report never prints the configuration file's contents, window titles, or any path outside the
configuration and icon-set locations (FR-071).

## In the repository

### Release

A named, tagged, immutable point in history.

| Field | Where it lives | Rule |
|---|---|---|
| version | `Cargo.toml` `version` | semver; first public release is `1.0.0` (FR-101) |
| tag | git tag `v<version>` | must not already exist when the workflow runs (FR-110) |
| changelog entry | `CHANGELOG.md` section | renamed from `[Unreleased]` with the date (FR-102a) |
| artefacts | GitHub release assets | the five of FR-106 |
| support status | `SECURITY.md` | which versions receive fixes (FR-120) |

**Invariant** (FR-103): the version the binary prints, the tag, and the changelog heading agree.
The release workflow asserts all three before publishing.

### Artefact

One published file. Kinds: source archive, `x86_64` binary, `.deb`, `.rpm`, `SHA256SUMS`.

| Field | Rule |
|---|---|
| name | carries the version, e.g. `hypr-swap_1.0.0_amd64.deb` |
| integrity | one SHA-256 line in `SHA256SUMS` (FR-108) |
| built on | for packages, the oldest supported release of the family (FR-109a) |

**Invariant** (FR-110): re-running a release for the same version either publishes the identical
file set or fails; it never produces a second, different artefact for one version.

### Packaging recipe

`packaging/aur/PKGBUILD`. Fields the release workflow rewrites: `pkgver`, `sha256sums`. Its source
is the published release archive, never the default branch (FR-107).

### Changelog entry

Keep a Changelog form: a version heading with a date, and any of `Added`, `Changed`, `Deprecated`,
`Removed`, `Fixed`, `Security`. Written by hand, for users, never derived from commit messages
(FR-102). A change that alters what a user can do carries its entry in `[Unreleased]` before it
merges (FR-102a).

### Documentation section

One half of the site, each with its own audience and authoritative scope.

| Section | Audience | Authoritative for |
|---|---|---|
| `user/` | someone running the program | how to install, configure and use it (FR-084a) |
| `dev/` | someone changing the program | architecture, tiers, release procedure; links to `specs/` rather than restating them |

### Automated check run

The verdict for one proposed change: a set of checks, each **gating** or **informational**. The
gate is the single `ci-required` job; the membership of both sets is stated in
[contracts/ci.md](./contracts/ci.md) (FR-091).

### Test environment image

`docker/e2e/Dockerfile`, published to the registry and used identically by automation and by a
contributor. Contract: given a Wayland session (a developer's own, or one automation supplies on a
virtual GPU), running it in the repository root runs `cargo test --test 'e2e_*'` and exits with
the suite's status (FR-089).

### Contribution

A proposed change carrying code, tests, documentation, a changelog entry and specification
updates, asserted by the pull-request checklist (FR-099).

### Bug report

A structured issue whose required fields are exactly the `EnvironmentReport` lines plus expected
versus observed behaviour (FR-097).

### Third-party component

Something in the tree that originates elsewhere: `protocols/hyprland-global-shortcuts-v1.xml` and
`assets/placeholder.svg` today. Each carries origin, revision and licence, in `THIRD-PARTY.md` and
in the file itself (FR-063).

### Supported version range

Three ranges the project publishes, each with exactly one definition:

| Range | Defined in | Stated in |
|---|---|---|
| supported Hyprland | `SUPPORTED_HYPRLAND` (`src/lib.rs`) | README requirements, site, FR-118 diagnostic |
| minimum toolchain | `rust-version` (`Cargo.toml`) | README, `DEVELOPMENT.md`, the MSRV CI job |
| supported releases | `SECURITY.md` | the security policy |
