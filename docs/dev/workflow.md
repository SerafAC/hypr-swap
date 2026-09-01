---
title: The spec-driven workflow
description: How a change gets from an idea to merged code, and why the specifications rather than the code are the record of what was promised.
---

This project is developed **spec-first**, with [spec-kit](https://github.com/github/spec-kit). The
short version: a feature is specified, planned and broken into tasks before it is written, and each
of those artefacts stays in the repository afterwards as the record of what was decided and why.

## Where the record lives

Everything is under [`specs/`](https://github.com/SerafAC/hypr-swap/tree/master/specs), one
directory per feature:

| Feature | State |
|---|---|
| [`001-workspace-swap-overlay`](https://github.com/SerafAC/hypr-swap/tree/master/specs/001-workspace-swap-overlay) | Delivered — the switcher, the overlay, the swap |
| [`002-overlay-visuals`](https://github.com/SerafAC/hypr-swap/tree/master/specs/002-overlay-visuals) | Delivered — icons, themes, the style catalogue |
| [`003-oss-release-readiness`](https://github.com/SerafAC/hypr-swap/tree/master/specs/003-oss-release-readiness) | In progress — packaging, CI, documentation, releases |

All of them are live references, not history. A delivered feature's specification still describes
behaviour the program has today.

## What each document is for

Rather than restate them, here is what to open and when:

| Document | Read it when you want |
|---|---|
| `spec.md` | To know what was actually promised. Numbered requirements (`FR-xxx`) and acceptance scenarios — **the authority on behaviour** |
| `plan.md` | Architecture decisions, the module map, and the table mapping every requirement to what verifies it |
| `tasks.md` | The task list with its `[X]` completion markers |
| `contracts/` | The external surface: shortcut names, the configuration schema, the CLI, diagnostics, the exact IPC commands used |
| `research.md` | Numbered decisions (`R1`, `R2`, …) each with the alternatives that were rejected and why |

## The two conventions that matter when you write code

**Comments cite requirement numbers.** A comment reading `// FR-024: per-setting fallback` is
pointing at `spec.md`, where the requirement is stated in full. Keep the convention — it is what
lets someone reading an unfamiliar branch of the code find out what it was for.

**Cite research decisions rather than re-litigating them.** `research.md R22` in a comment means the
question was considered, the alternatives are written down, and this was the answer. The numbering
runs continuously across features — 001 holds R1–R17, 002 holds R18–R28, 003 continues from R29 —
so a citation never needs to name which feature it belongs to. If you think a decision is wrong,
the thing to change is the decision, in the document, with its rationale — not the code alone.

**`tasks.md` is updated as tasks complete.** Marking a task `[X]` is part of doing it.

## The constitution

[`.specify/memory/constitution.md`](https://github.com/SerafAC/hypr-swap/blob/master/.specify/memory/constitution.md)
is binding rather than aspirational. KISS, YAGNI and DRY; unit tests for all code; end-to-end
coverage of major requirements. A new abstraction or a new dependency has to be justified in
`plan.md`'s Complexity Tracking table — which is a real table with real entries, not a formality:
the documentation framework's second language toolchain is in it, argued and accepted rather than
waved through.

## Proposing a change

If the change is a bug fix or something small and self-contained, open a pull request; the checklist
in the template is what review looks for, and
[`CONTRIBUTING.md`](https://github.com/SerafAC/hypr-swap/blob/master/CONTRIBUTING.md) is the full
account of the process and what to expect.

If it adds or alters behaviour, it needs a requirement to point at. That may mean an addition to an
existing `spec.md`, or a new feature directory — open an issue first and it can be worked out there
rather than in review. The thing to avoid is arriving with finished code that no requirement covers,
because there is then no way to say whether it is right.

## Why the specifications stay authoritative

The site you are reading does not restate the contracts; it **includes** them, so the published
reference and the specification are the same bytes and cannot drift apart. The
[configuration](../user/configuration.md) and [appearance](../user/styling.md) pages are the
specification's own contract files with prose around them, and
[verification coverage](./verification.md) is the plan's own table.

That is the mechanism behind a rule worth stating plainly: **`specs/` is where a question is
answered authoritatively, and everything else links to it.** A page that copied a contract would be
a second place for the truth to live, and eventually a second, different truth.
