# Contract: Automated checks

Every proposed change and every push to the default branch runs these, with no maintainer action,
and produces one unambiguous verdict (FR-085). The verdict is the **`ci-required`** job: it is the
only check branch protection requires, and it depends on exactly the gating jobs below. A job that
is renamed, added or removed therefore cannot silently start or stop gating — the change is visible
in one file (FR-091).

## Gating checks

| Job | Command | Reproduce locally with | Requirement |
|---|---|---|---|
| `build` | `cargo build --release` | the same | FR-086 |
| `unit` | `cargo test --lib` | the same | FR-086 |
| `clippy` | `cargo clippy --all-targets -- -D warnings` | the same | FR-086 |
| `fmt` | `cargo fmt --check` | the same | FR-086 |
| `msrv` | `cargo build` on the toolchain named by `rust-version` | `rustup run <version> cargo build` | FR-087 |
| `docs` | `pnpm install --frozen-lockfile && pnpm build && pnpm validate` | the same | FR-076, FR-078 |
| `checks` | `licence-files`, `docs-map`, `changelog` (see below) | `./scripts/checks.sh` | FR-062–FR-065, FR-074, FR-077–FR-084, FR-102a |
| `licenses` | `cargo deny check licenses` | the same | FR-064 |

Every job's failure step prints the command in its "reproduce locally" column, so a contributor is
never left to guess what to run (FR-090).

**The `needs:` list of `ci-required` and the table above are the same statement.** A job added to
one is added to the other in the same change; that is what makes the gating set visible in one file
rather than spread across job names (FR-091).

**Where `e2e` went.** It was in this table, aggregated into `ci.yml` via `uses:` so that
`ci-required` could `needs:` it. **There is no `e2e` job any more.** Both ways of supplying
automation with a compositor were built and measured, and neither can host the nested compositor
the harness needs ([research.md](../research.md) R29, marked failed). The tier is verified on a
developer's machine instead, which is recorded as a deviation against FR-088 in
[spec.md](../spec.md).

**Where the site is built.** The `docs` job above is the pull-request build, and it is what gates.
`docs.yml` builds and deploys only on pushes to the default branch (FR-078, [research.md](../research.md)
R46); it does not run on pull requests, so the two never race and a broken book fails the change
that broke it.

## Informational checks

| Job | Why it does not gate |
|---|---|
| `advisories` (`cargo deny check advisories`) | An advisory is news about the world, not a defect in the change under review; blocking every contributor on someone else's disclosure is how a project learns to ignore red. Acceptances are recorded and time-bounded — see below. |

The **expiry of an acceptance is gating**, in `unit`: `deny.toml`'s `ignore` entries must carry a
`reason` beginning `until YYYY-MM-DD:`, and a unit test fails once that date has passed or if the
form is wrong (FR-093, [research.md](../research.md) R38). An advisory can therefore be accepted
deliberately, but not forgotten.

## The document checks (`checks`)

Shell steps, because their subject is files rather than values:

- **`licence-files`** — `LICENSE` exists, names a holder and a year, and matches `Cargo.toml`'s
  `license`; `Cargo.toml` carries description, repository, documentation and keywords; every path
  under `protocols/` and `assets/` is accounted for in `THIRD-PARTY.md` (FR-062, FR-063, FR-065).
- **`docs-map`** — the document map of [documentation.md](./documentation.md) holds: the required
  page set exists, the README carries no development instructions, `DEVELOPMENT.md` names every
  top-level directory and every `src/**/*.rs` module, the site's front page names a released
  version, and every failure in the troubleshooting page names a real `diag::Condition`
  (FR-068, FR-074, FR-077–FR-084).
- **`changelog`** — a change touching `src/` has a non-empty `[Unreleased]` section (FR-102a).

Two checks are deliberately **not** shell steps but unit tests, because they compare a document
against values that live in the code: the settings-catalogue walk (FR-083) and the advisory expiry
above. A contributor sees them fail in `cargo test --lib`, next to the code they changed.

## The E2E environment (FR-089)

**The end-to-end tier does not run in automation.** Both routes for supplying a compositor were
built and measured and neither can host the nested compositor the harness needs
([research.md](../research.md) R29, marked failed): a `vkms` parent has no render node to allocate
from, and a `virtio-gpu` parent has a render node but no driver behind it, so the nested compositor
is refused KMS dumb buffers on a node the parent holds master on. The tier's verification is a
developer's machine — the deviation is recorded against FR-088 in [spec.md](../spec.md).

`docker/e2e/Dockerfile` survives, with a narrower purpose: **local compatibility testing against
pinned versions.** It carries a pinned Hyprland, `foot`, mesa, cairo/pango and a `rustup` toolchain
matching `rust-version`, runs as an unprivileged user, and runs
`cargo test --no-fail-fast --test 'e2e_*'` against the repository mounted at `/work`. It answers
"does this still work against the compositor and toolchain the project supports?" without changing
what is installed on the machine asking. It is **not** published to any registry: nothing consumes
it but the person who built it, and FR-089 asks only that the image be defined in the repository
and usable locally. [docker/e2e/README.md](../../../docker/e2e/README.md) is its documentation.

It runs against a session the contributor already has — passed in with the host's
`XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY` and one render node. That command is in
[quickstart.md](../quickstart.md) and in the image's README. The harness is unchanged: it nests
inside whichever session it is given.

## Where this is published

The developer section of the site carries this page's content and the requirement-to-tier table
`::include[]`d from [plan.md](../plan.md)'s `verification-tiers` section, beside the rows derived
for features 001 and 002 — together they are FR-092: every requirement of the project, not only
this feature's, has a named tier and none is unknown.
