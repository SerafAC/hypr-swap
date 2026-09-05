<!--
FR-099: the checklist covering tests, documentation, changelog and specification updates. It
restates only the checklist — CONTRIBUTING.md is the account of what review looks for and why.
-->

## What this changes

<!-- What it does, and what problem it solves. If there is an issue, link it. -->

## Checklist

- [ ] **Tests** — unit tests cover the code; a bug fix carries a test that fails against the old
      behaviour. `cargo test --lib`, `cargo clippy --all-targets -- -D warnings` and
      `cargo fmt --check` pass locally.
- [ ] **Documentation** — anything this made untrue is updated, in the one document that answers it.
      `./scripts/checks.sh` passes.
- [ ] **Changelog** — an entry under `[Unreleased]` in `CHANGELOG.md`, written in a user's
      vocabulary. *(Required when `src/` changed; if this alters no documented behaviour, say so
      below instead.)*
- [ ] **Specification** — for a change that adds or alters behaviour: the requirement in the
      feature's `spec.md`, its row in `plan.md`'s coverage table, and the `[X]` markers in
      `tasks.md`. *(Not applicable to a fix that changes no promised behaviour.)*

## Anything the checklist does not cover

<!--
A box you could not tick and why; an end-to-end tier you had no session to run (automation cannot
run it either — see CONTRIBUTING.md); a decision you would like looked at before it goes further.
Saying so here is better than leaving it to be discovered in review.
-->
