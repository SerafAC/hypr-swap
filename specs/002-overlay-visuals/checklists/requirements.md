# Specification Quality Checklist: Overlay Visuals

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-27
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- This spec is the merge of two earlier drafts — program icons and overlay theming — into one
  feature. Both change the overlay's appearance, both touch the same drawing and the same
  configuration file, and they interact (FR-051, FR-052, SC-018). The five clarifications from both
  drafting sessions are preserved in the Clarifications section.
- Requirement numbering continues feature 001 (FR-035+, SC-011+) because this codebase cites FR
  numbers in code comments. Numbers were reassigned contiguously during the merge; the pre-merge
  drafts were untracked and unplanned, so nothing referenced the old numbers.
- The merge resolved three real conflicts between the drafts, which planning must not undo:
  - The two drafts each carried an "invalid configuration value" rule (icon settings, style values).
    They are now one rule, FR-059, covering every visual setting.
  - Both drafts claimed the overlay would be "pixel-identical to before" — mutually impossible, since
    icons default to on. Split into SC-018 (default config: only the icons differ) and SC-019 (icons
    disabled: genuinely pixel-identical).
  - The icon set (FR-057) and the overlay theme (FR-049) are two independent settings. FR-057 states
    that explicitly so neither is implemented as a facet of the other.
- FR-053 restates feature 001's FR-019 and FR-015a as invariants that neither icons nor themeable
  geometry may weaken; SC-023 makes that testable. Planning must not treat themeable geometry as
  licence to relax them.
- The spec supersedes feature 001's "theming is out of scope" assumption, and only for the values in
  FR-045 to FR-047. That header note must survive into planning.
- A `/speckit-clarify` session on 2026-08-28 asked and integrated 5 questions: icon image formats,
  icon resolution timing, flat-row horizontal budget, miniature omission order, and what a built-in
  theme contains. All 16 items re-validated as passing against the updated spec.
- Deliberately deferred to plan.md/contracts, not specified here: the colour notation (FR-045), the
  concrete geometry ranges (FR-054), and the icon lookup path (FR-040). The miniature
  content-omission order is no longer deferred — clarification fixed it in FR-038.
- Six user stories is above the usual size for one feature. They are independently testable and
  priority-ordered (two P1, two P2, two P3), so the P1 pair remains a viable MVP on its own, but
  planning should expect to phase this rather than land it in one pass.
- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
