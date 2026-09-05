# Contributing to hypr-swap

Thank you for considering it. This document is what review looks for and how a change gets from an
idea to merged code. It does not repeat the setup: [DEVELOPMENT.md](DEVELOPMENT.md) takes you from
a fresh clone to a running program and a passing test suite, and is the place to start if you have
not built the project yet.

If you are reporting a problem rather than proposing a change, the issue forms ask for what would
otherwise have to be asked for, and a security finding goes to [SECURITY.md](SECURITY.md)'s private
channel rather than the public tracker.

## The rules the project holds to

`.specify/memory/constitution.md` is binding rather than aspirational, and a reviewer will apply it.
Three principles do most of the work:

- **Simplicity first (KISS).** The simplest thing that satisfies the requirement. Plain functions
  over frameworks, direct calls over indirection, the standard library over a new dependency. A new
  abstraction is justified in writing — in the feature's `plan.md` Complexity Tracking table —
  before it is introduced, not after it is reviewed.
- **Build only what is needed (YAGNI).** Only what a current, accepted requirement demands.
  Speculative configuration options, unused parameters and extension points "for later" are the
  things most often asked to be removed. If a capability is not traceable to a requirement, it does
  not get built.
- **Single source of truth (DRY).** Knowledge — a rule, a constant, a schema — has exactly one
  authoritative definition. This applies to *knowledge*, not to code that merely looks alike; do
  not couple unrelated things because their lines resemble each other.

One structural rule sits on top of them, and it decides where most changes go. The codebase is
organised around a single seam: **pure decision logic on one side, a thin I/O shell on the other**,
and **a new decision rule belongs on the pure side**, in a module with a unit test, with the shell
calling it. [DEVELOPMENT.md's architecture section](DEVELOPMENT.md#architecture) explains the seam
and why the shell is kept as small as it is. A new condition, ordering, fallback or piece of
arithmetic written into the Wayland handler is the most common reason a change is sent back.

## Adding a dependency

One consequence of the simplicity principle comes up often enough to state on its own, so that you
know the bar before you write against it rather than after: **a new dependency is justified in
writing before it is added, not defended after it is reviewed.** The justification is a row in the
feature's `plan.md` Complexity Tracking table — what it is, which requirement needs it, and what
simpler thing was rejected in its favour — and the constitution makes that binding rather than
customary. A change that arrives with a new entry in `Cargo.toml` and nothing in that table is sent
back for the table, not for the crate.

If your change is a bug fix with no feature directory of its own and it needs a dependency, that is
a sign it is not a bug fix. Open an issue and it can be worked out there.

The bar applies to a development dependency and to the documentation site's npm tree exactly as it
does to something the program links: `deny.toml` reads the whole graph, dev dependencies included,
and a crate nobody ships is still a crate somebody has to keep working.

What the justification answers:

- **Which requirement needs it**, by number. A dependency traceable to no requirement is the YAGNI
  case above, in a more expensive form.
- **Why the standard library, or something already in the tree, will not do.** This is the one that
  decides most of them. `toml`, `serde`, `cairo` and `resvg` are already here and already carry
  their cost; a second way to do something they do needs more than a nicer interface.
- **What arrives with it.** Transitive dependencies, a build script, a native toolchain, a minimum
  toolchain higher than `rust-version` — all of them land on every contributor and every packager,
  not only on you.
- **Its licence.** It must already be in `deny.toml`'s allow-list, or your change adds it there.
  `cargo deny check licenses` gates the merge, so this is not a matter of intent. Adding a licence
  to that list changes what the project tells a packager it redistributes, so
  [`THIRD-PARTY.md`](THIRD-PARTY.md#the-dependency-graphs-licences)'s table changes in the same
  commit.
- **Who maintains it, and what happens if they stop.** Not a veto — most of the tree would fail a
  strict reading of it — but an answer worth having in writing before the answer is needed.

None of this is a judgement on the crate. It is that a dependency is the easiest thing in a project
to add and the hardest to remove, and the asymmetry is worth a paragraph of writing up front.

## How a change is specified

The project is developed spec-first, with [spec-kit](https://github.com/github/spec-kit): a feature
is specified, planned and broken into tasks before it is written, and all of it stays in `specs/`
afterwards as the record of what was decided and why. The full account of the workflow is
[the spec-driven workflow page](https://serafac.github.io/hypr-swap/dev/workflow/); what it means
for your change is short.

**A bug fix or something small and self-contained** needs no new specification. Fix it, add the unit
test that fails against the old behaviour, and open the change.

**A change that adds or alters behaviour needs a requirement to point at.** That may be an addition
to an existing `spec.md` or a new feature directory. Open an issue first and it can be worked out
there rather than in review — arriving with finished code that no requirement covers leaves no way
to say whether it is right, which is a bad position for everyone.

Two conventions apply to the code itself:

- **Code cites requirement numbers.** A comment reading `// FR-024: per-setting fallback` points at
  the `spec.md` where the requirement is stated in full, and `research.md R22` means the question
  was considered, the alternatives are written down, and this was the answer. Cite them rather than
  restating or re-litigating them. The research numbering runs continuously across features, so a
  citation never needs to name which one it belongs to.
- **A behavioural change updates the specification alongside the code** — the requirement in
  `spec.md`, the row in `plan.md`'s coverage table naming what verifies it, and the task list's
  `[X]` markers. These are part of the change, not follow-up work: a requirement with no verifying
  tier, or a tier table describing tests that do not exist, is the thing the record exists to
  prevent.

## What review looks for

In roughly the order it comes up:

| | |
|---|---|
| **Constitutional compliance** | Unjustified complexity, speculative generality and duplicated knowledge are rejected on their own, not only when they are also wrong |
| **Which side of the seam** | A decision in a pure module with a unit test; the shell doing what it is told |
| **Tests** | Unit tests for the code, and a bug fix carries a test that fails against the old behaviour. Test-first is explicitly not required — the tests existing and passing is |
| **The specification** | For a behavioural change: the requirement, the coverage row, the task markers |
| **Documentation** | Whatever the change made untrue. One question is answered authoritatively in one document and linked to from the others (`specs/003-oss-release-readiness/contracts/documentation.md` is the map) |
| **The changelog** | An entry under `[Unreleased]`, written in a user's vocabulary rather than the code's. `./scripts/checks.sh` requires one when `src/` changed |
| **Green checks** | Below |

## Before you open it

```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo fmt --check
./scripts/checks.sh
cargo deny check licenses    # only if you added or changed a dependency
```

**The end-to-end tier needs a live Hyprland session, and there is no way to run it without one.**
If you do not have one — or want to test against the versions the project pins rather than the ones
you happen to have installed — [DEVELOPMENT.md's *Which tier needs what*](DEVELOPMENT.md#which-tier-needs-what)
is the authoritative account of what each tier requires and how to run the end-to-end tier in the
project's `docker/e2e/` image instead. Automation verifies the release build, the unit tier, clippy,
formatting, the minimum-toolchain build, the documentation site, the document checks and the
dependency licences on your behalf, and reports one verdict; it cannot run the end-to-end tier at all, which is recorded as an
unmet requirement rather than quietly dropped
([`research.md` R29](specs/003-oss-release-readiness/research.md)).

Every automated job prints the command that reproduces it locally, so a red check never leaves you
guessing what to run.

## What to expect

**hypr-swap is maintained by one person, on a best-effort basis, in their own time.** That is worth
saying plainly rather than leaving you to infer it from silence:

- A first response usually arrives within a week or two. Longer gaps happen and are not a verdict on
  your change.
- A small, self-contained, tested change with a clear description is the one most likely to be
  merged quickly. A large change touching many modules will take longer, and is much better started
  as an issue than as a finished branch.
- A change can be declined on scope alone. hypr-swap targets Hyprland on Wayland and deliberately
  does not manage windows, replace a bar or launcher, or draw outside its own overlay
  ([README](README.md#scope-and-privacy)). This is not a comment on the idea; some good ideas belong
  in a different program.
- Review will ask for changes. It is applying the rules above to the code rather than to you, and
  [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) applies in both directions.
