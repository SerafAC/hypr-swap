# Contract: Versioning and the changelog

The project follows **semantic versioning** and its public history begins at **1.0.0** (FR-101).
There is no 0.x line: the contracts below are declared stable from the first published release.
The `0.1.0` recorded in `Cargo.toml` today is pre-publication and is raised by the first run of the
release workflow.

## The stable surface

A **breaking change** is a change to any of the following (FR-101a). Each has exactly one
authoritative definition, named here so that "is this breaking?" is answerable without judgement:

| Surface | Authority |
|---|---|
| Configuration keys and their accepted values | [002 contracts/config.md](../../002-overlay-visuals/contracts/config.md), [001 contracts/config.md](../../001-workspace-swap-overlay/contracts/config.md) |
| Style values: names, forms, ranges, defaults | [002 contracts/style-values.md](../../002-overlay-visuals/contracts/style-values.md) |
| Global shortcut names | [001 contracts/shortcuts.md](../../001-workspace-swap-overlay/contracts/shortcuts.md) |
| In-overlay keys | [001 contracts/shortcuts.md](../../001-workspace-swap-overlay/contracts/shortcuts.md) |
| Command-line flags and their output shape | [001 contracts/cli.md](../../001-workspace-swap-overlay/contracts/cli.md), [cli.md](./cli.md) |
| Exit codes | as above |
| Diagnostic subjects | [001 contracts/diagnostics.md](../../001-workspace-swap-overlay/contracts/diagnostics.md), [diagnostics.md](./diagnostics.md) |
| The install map — where a package puts each file | [packaging.md](./packaging.md) |

### What each level means

- **MAJOR** — any change to the surface above that an existing user's configuration, bind lines,
  scripts or expectations could notice: a key renamed or removed, an accepted value withdrawn, a
  default changed, a shortcut name changed, a flag removed or its output reshaped, an exit code
  reassigned, a diagnostic subject renamed, a file moved to a different path inside a package.
  Raising a supported-compositor **minimum** is also major: it withdraws support a user had.
- **MINOR** — new capability that leaves the surface intact: a new setting with a default that
  reproduces today's behaviour, a new flag, a new diagnostic condition, a new accepted value for an
  existing setting.
- **PATCH** — fixes and internal changes that alter no documented behaviour. Documentation-only and
  specification-only changes do not require a release at all.

Diagnostic *messages* are not part of the surface — only their subjects and levels are — so
rewording a message is a patch. Adding a condition is minor.

### Deprecation

A key that is renamed or removed keeps its old name **recognised** in the release that changes it,
reporting what replaced it, so that a configuration file written for an earlier release is never
silently reinterpreted (FR-117). That release is a major one regardless.

No compatibility layer is built for this ([research.md](../research.md) R43): the existing
`UnknownConfigKey` diagnostic, FR-024's per-setting fallback and the major-version rule above are
the whole mechanism. It is held by two release-checklist items — keep the old name recognised, and
refresh `tests/fixtures/config-previous-release.toml` — and by the E2E test that reads that
fixture.

## The changelog

`CHANGELOG.md`, in [Keep a Changelog](https://keepachangelog.com/) form, written **by hand** for
users; never derived from commit messages, and there is no commit-message convention (FR-102).

- Every change that alters what a user can do adds to the `[Unreleased]` section **as it lands**;
  a pull request that changes `src/` without touching the changelog fails the `changelog` check
  (FR-102a). A documentation- or specification-only change does not need an entry.
- Sections are `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
- Entries answer what a user can now do, what changed, and what broke — not which functions moved.
- Anything breaking is stated in the entry in the user's own vocabulary: the key, the shortcut or
  the flag by name, and what to write instead.
- The release workflow renames `[Unreleased]` to `## [<version>] - <YYYY-MM-DD>` and opens a fresh
  empty `[Unreleased]` above it. Nothing else edits the file.

## The version at runtime

One definition — `Cargo.toml`'s `version` — reaches the binary through `hypr_swap::version()`,
which composes it with a git-describe suffix for builds that are not exactly a release tag through
the pure `compose_version` (FR-104); the forms are
in [cli.md](./cli.md). The release workflow asserts that the runtime version, the tag and the
changelog heading agree before it publishes (FR-103).
