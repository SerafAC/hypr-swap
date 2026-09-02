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
| `e2e` | `cargo test --test 'e2e_*'` in the image | `docker run … ghcr.io/<owner>/hypr-swap-e2e` | FR-088 |

Every job's failure step prints the command in its "reproduce locally" column, so a contributor is
never left to guess what to run (FR-090).

**How `e2e` is aggregated.** The E2E tier keeps its own file, `.github/workflows/e2e.yml`, declared
`on: workflow_call`; `ci.yml` invokes it as the `e2e` job (`uses: ./.github/workflows/e2e.yml`) so
that `ci-required` can `needs:` it. A job in a workflow `ci.yml` does not call cannot be a
dependency of `ci-required`, and making the tier a second required check would defeat the single
visible gate FR-091 asks for. The same rule binds every gating job: the `needs:` list of
`ci-required` and the table above are the gating set, and a job added to one is added to the other
in the same change.

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

## The E2E environment (FR-088, FR-089)

`docker/e2e/Dockerfile` defines one image, used identically by automation and by a contributor.
It carries a pinned Hyprland, `foot`, `seatd`, mesa, the cairo/pango development libraries and a
`rustup` toolchain, runs as an unprivileged user, and its entry point runs
`cargo test --test 'e2e_*'` against the repository mounted at `/work`.

It needs a Wayland session, and gets one of two ways ([research.md](../research.md) R29):

- **a contributor's own session** — the image is run with the host's `XDG_RUNTIME_DIR`,
  `WAYLAND_DISPLAY` and one render node; verified working, and the exact command is in
  [quickstart.md](../quickstart.md);
- **automation supplies one** — a virtual GPU and a seat, on which the image starts its own parent
  compositor before running the suite. A plain container cannot do this: Hyprland 0.56 has no
  headless-only mode, its DRM backend needs a seat and its Wayland backend needs a dmabuf
  allocator, all three measured in R29.

Either way the harness is unchanged: it nests inside whichever session it is given, exactly as it
does on a developer's machine.

**Where the image is published.** `e2e-image.yml` pushes the default branch's image to
`ghcr.io/<owner>/hypr-swap-e2e`, tagged `latest` and by commit, so a contributor can pull the image
rather than build it and a bug report can quote its digest. The `e2e` job *builds* from the
Dockerfile rather than pulling, so a change to the image is verified by the change that makes it.

**When the environment itself fails** — the image cannot start a compositor, rather than a test
failing — the job reports that distinctly, by asserting the parent session is up before invoking
`cargo test`. An environment failure is a broken runner, not a broken change, and the message says
so.

## Where this is published

The developer section of the site carries this page's content and the requirement-to-tier table
`::include[]`d from [plan.md](../plan.md)'s `verification-tiers` section, beside the rows derived
for features 001 and 002 — together they are FR-092: every requirement of the project, not only
this feature's, has a named tier and none is unknown.
