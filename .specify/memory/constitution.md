<!--
Sync Impact Report
------------------
Version change: (unfilled template) → 1.0.0
Bump rationale: Initial ratification — first concrete constitution for this project.

Principles defined (all new):
  - I. Simplicity First (KISS)
  - II. Build Only What Is Needed (YAGNI)
  - III. Single Source of Truth (DRY)
  - IV. Unit Tests for All Code (NON-NEGOTIABLE)
  - V. End-to-End Coverage of Major Requirements (NON-NEGOTIABLE)

Added sections:
  - Testing Standards
  - Development Workflow

Removed sections: none (all template placeholders replaced)

Templates requiring updates:
  ✅ .specify/templates/plan-template.md — Constitution Check gates made concrete
  ✅ .specify/templates/tasks-template.md — tests are mandatory, not optional;
     test-first ordering removed to match Principle IV
  ✅ .specify/templates/spec-template.md — reviewed, no change needed
     (acceptance scenarios already carry E2E intent)
  ✅ .claude/skills/speckit-tasks/SKILL.md — "Tests are OPTIONAL" rule replaced with the
     mandatory unit + E2E rule from Principles IV & V
  ✅ .claude/skills/speckit-*/SKILL.md (others) — reviewed, no agent-specific stale
     references found; all use generic `/speckit-*` naming

Deferred TODOs: none
-->

# hypr-swap Constitution

## Core Principles

### I. Simplicity First (KISS)

Every solution MUST be the simplest one that satisfies the stated requirement. Prefer
straightforward, readable code over clever code: plain functions over frameworks, direct calls
over indirection layers, standard library over new dependencies. A new abstraction, layer, or
dependency MUST be justified in writing (in the plan's Complexity Tracking table) before it is
introduced.

Rationale: The cost of a system is dominated by the cost of understanding it. Simple code is
cheaper to review, debug, and change than code that anticipates problems that may never arrive.

### II. Build Only What Is Needed (YAGNI)

Implement only what a current, accepted requirement demands. Speculative configuration options,
unused parameters, extension points "for later", and dead code paths MUST NOT be added. If a
capability is not traceable to a requirement in the feature spec, it does not get built.

Rationale: Unused flexibility is not free — it must still be read, tested, and maintained, and
it is almost always the wrong shape when a real need finally appears.

### III. Single Source of Truth (DRY)

Knowledge — logic, constants, schemas, validation rules — MUST have exactly one authoritative
definition. Duplicated logic MUST be factored out once the duplication is real and confirmed
(not merely anticipated). This principle applies to knowledge, not to coincidentally similar
lines of code; do not couple unrelated code just because it looks alike today.

Rationale: Divergent copies of the same rule are a leading source of defects. Extracting shared
knowledge keeps a change in one place from silently failing to apply everywhere else.

### IV. Unit Tests for All Code (NON-NEGOTIABLE)

All production code MUST be covered by unit tests. Tests MAY be written after the implementation
— test-first development is explicitly NOT required — but a unit of work is not complete until
its unit tests exist and pass. Bug fixes MUST add a unit test that fails against the old
behaviour.

Rationale: Coverage protects against regression regardless of when it is written; mandating a
particular authoring order adds ceremony without adding safety.

### V. End-to-End Coverage of Major Requirements (NON-NEGOTIABLE)

Every major requirement MUST have at least one end-to-end test that exercises it through the
system's real external interface. Coverage is traced explicitly: each E2E test names the
requirement or acceptance scenario it verifies, and a feature is not done while any major
requirement lacks one.

Rationale: Unit tests confirm the parts work; only E2E tests confirm the system delivers what
was actually asked for.

## Testing Standards

- Unit tests MUST be fast, deterministic, and independent of test execution order.
- E2E tests MUST drive the system through its real interface, not through internal APIs.
- External dependencies MAY be stubbed in unit tests; E2E tests SHOULD use real components
  wherever practical, and any substitution MUST be documented in the plan.
- The full test suite MUST pass before any change is considered complete. A failing or skipped
  test MUST be fixed or removed with justification — never left red.
- Test code is subject to the same principles as production code: KISS, YAGNI, and DRY apply.

## Development Workflow

- Work follows the Spec Kit flow: specify → plan → tasks → implement.
- The Constitution Check gate in `plan.md` MUST be evaluated before Phase 0 research and
  re-evaluated after Phase 1 design. Violations MUST be recorded in Complexity Tracking with the
  simpler alternative and why it was rejected, or the design MUST be changed.
- Every task list MUST include unit-test tasks for the code it introduces and E2E-test tasks for
  the major requirements it satisfies.
- Reviews MUST verify constitutional compliance, not just correctness. A reviewer MUST reject
  unjustified complexity, speculative generality, and duplicated knowledge.

## Governance

This constitution supersedes all other development practices for this project. Where other
guidance conflicts with it, this document wins.

**Amendments**: Changes MUST be made through `/speckit-constitution`, which updates this file
and propagates the change to dependent templates in the same commit. Each amendment MUST record
the version change and rationale in the Sync Impact Report at the top of this file.

**Versioning**: This constitution uses semantic versioning.
MAJOR — a principle is removed or redefined in a backward-incompatible way.
MINOR — a principle or section is added, or guidance is materially expanded.
PATCH — clarifications, wording, and typo fixes that do not change meaning.

**Compliance**: Every plan and review MUST verify compliance with these principles. Any
deviation MUST be explicitly justified and documented in the feature's Complexity Tracking
table; undocumented deviations are defects.

**Version**: 1.0.0 | **Ratified**: 2026-07-27 | **Last Amended**: 2026-07-27
