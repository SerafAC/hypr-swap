# Contract: The release procedure

One workflow, `workflow_dispatch`, one input. Nothing about releasing lives only in the
maintainer's head (FR-105), and nothing is published from a tree that is not ready (FR-110).

## Input

| Input | Form | Validation |
|---|---|---|
| `version` | `MAJOR.MINOR.PATCH` | semver; strictly greater than the current `Cargo.toml` version; the first release must be exactly `1.0.0` |

## Preconditions — all checked before anything is written (FR-110)

1. The workflow was triggered on the default branch, and the working tree is clean.
2. The tag `v<version>` **does not exist**, unless a *draft* release already exists for it — the
   resume case below.
3. The gating checks of [ci.md](./ci.md) are green on the commit being released. Steps 1–4 then
   create a *new* commit, so the gate is re-run against the tag before step 6 builds anything: no
   artefact is ever built from a commit no check has seen.
4. `CHANGELOG.md` has a non-empty `[Unreleased]` section.

Any failure stops the workflow before the first commit, so a refused release leaves no trace.

## Steps

| # | Step | Requirement |
|---|---|---|
| 1 | Raise `version` in `Cargo.toml`; refresh `Cargo.lock` (`cargo update -w`) | FR-105 |
| 2 | Rename `[Unreleased]` to `## [<version>] - <date>`; open a fresh empty `[Unreleased]` | FR-102a |
| 3 | Assert the runtime version, the new tag and the changelog heading agree | FR-103 |
| 4 | Commit (`release: <version>`) and tag `v<version>` | FR-103 |
| 5 | Re-run the gating checks of [ci.md](./ci.md) against the new tag; stop if they are not green | FR-110 |
| 6 | Build the `x86_64` binary (release profile) | FR-106 |
| 7 | Build the `.deb` in the oldest supported Ubuntu LTS container; build the `.rpm` in the oldest supported Fedora container | FR-106, FR-109a |
| 8 | Install each package in a clean container of that family's oldest and current release; run `--version` and `--environment` | FR-109, SC-039 |
| 9 | Compute `SHA256SUMS` over every artefact | FR-108 |
| 10 | Publish the release with its notes: the changelog entry, plus the packager block of FR-111 | FR-106, FR-111 |
| 11 | Verify every published asset against `SHA256SUMS` by re-downloading | FR-108 |
| 12 | Regenerate **both** Arch recipes (`pkgver`, `pkgrel`, `sha256sums`) from the published artefacts — the archive for `hypr-swap`, the binary *and* the archive for `hypr-swap-bin`; commit; push each to its own AUR repository | FR-107, FR-107a |

Step 12 is its own workflow, called by the release workflow as its last job. It is the same step in
the same order; what it gains by being separate is that the one step whose failures come from
outside the repository can be disabled, re-enabled and re-run alone, against a release published
long before. It takes only the version: the digests it rewrites the recipes with are read out of
the release's own published `SHA256SUMS`.

## Artefacts (FR-106)

| Artefact | Name |
|---|---|
| Source archive | `hypr-swap-<version>.tar.gz` (GitHub's tag archive) |
| Binary | `hypr-swap-<version>-x86_64` |
| Debian family | `hypr-swap_<version>_amd64.deb` |
| RPM family | `hypr-swap-<version>-1.x86_64.rpm` |
| Integrity | `SHA256SUMS` |

## Release notes (FR-111)

Beyond the changelog entry, every release carries what a distribution packager needs without
contacting the maintainer:

- build dependencies with minimum versions — Rust `rust-version`, cairo, pango, pangocairo;
- runtime dependencies — cairo, pango, pangocairo; optional: an icon set, `notify-send`;
- the build steps (`cargo build --release`);
- the install map from [packaging.md](./packaging.md);
- the verified distribution matrix.

## Re-running after a partial failure (FR-110)

The release is created as a **draft** and published only by step 11. Re-running the workflow for a
version whose tag exists:

- **fails** if a published (non-draft) release exists for that tag — a published version is
  immutable;
- **resumes** if the release is still a draft: it checks out the existing tag rather than creating
  one, rebuilds the artefacts from that exact commit, and replaces the draft's assets.

Because every artefact is built from the tag rather than from the branch head, a resumed run
cannot produce a different file for the same version.

The AUR push is **step 12, after** the release is published and verified, so a run that fails late
never leaves a recipe pointing at a release that does not exist. A failure on either of the two
recipes fails the job — they describe the same release, and one of them quietly lagging is the
same defect.

## The AUR push, while the AUR is closed

The AUR has paused new account registration, so neither package can be created there yet. Step 12
therefore separates the two things it was doing:

- **regenerating both recipes and committing them** happens on every release, unconditionally.
  This is what FR-107's "in step with the released version" asks for, and it is satisfied in this
  repository whatever the AUR is doing;
- **pushing them to the AUR** is gated on the repository variable `AUR_PUBLISH` being `true`, and
  it is currently unset. When it does run and the key is absent it still fails loudly rather than
  skipping: a push that silently skips itself is how a recipe falls behind, and the gate exists so
  that a *deliberate* pause is visible in the workflow rather than disguised as one.

Turning it back on is setting that variable. Recipes for releases published while it was off are
caught up by running the AUR workflow by hand, once per version, with the push box ticked.

## Supported versions

`SECURITY.md` states which released versions receive fixes (FR-120); the release workflow does not
change it, and updating it is a checklist item in [quickstart.md](../quickstart.md).
