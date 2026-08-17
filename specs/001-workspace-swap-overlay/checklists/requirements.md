# Specification Quality Checklist: Workspace Swap Overlay

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-27
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

- Iteration 1: two [NEEDS CLARIFICATION] markers open — miniature content (FR-015) and hotkey
  ownership (FR-022).
- Iteration 2: both resolved by the user.
  - Miniatures are schematic layouts of labelled window rectangles, independent of screen
    capture (FR-015a, FR-015b, SC-008, US3 scenarios 2-3).
  - Hotkeys are declared in the application's own configuration file and claimed by the
    application itself (FR-022, FR-022a, SC-006, US5 scenarios 1 and 6).
- Iteration 3 (`/speckit-clarify`, session 2026-07-27): four clarifications integrated — entry
  order and initial highlight (FR-008a–d), compositor reconnection (FR-026a–d), diagnostic
  channels (FR-029–032), and all-or-nothing swap with rollback (FR-013a–c). FR-025 was narrowed to
  start-up so it no longer contradicts the reconnection behaviour.
- Iteration 4 (`/speckit-clarify`, session 2026-07-27, second run): five clarifications integrated.
  - **FR-022 released.** Key combinations are no longer owned by the application; they are bound in
    the compositor's configuration and delivered as two named shortcuts (open switcher, new
    workspace). Wayland offers ordinary clients no global-hotkey mechanism, so the original
    requirement was not achievable. Commit-on-release is preserved as a hard requirement (FR-022a).
  - In-overlay keys are handled by the application under exclusive keyboard focus with fixed
    defaults (FR-002a, FR-004a); backwards navigation is no longer a bound shortcut.
  - New-workspace behaviour pinned to lowest unused number, no-op when already empty (FR-020/021).
  - Overflow pinned to fixed-size entries plus scrolling, never scale-to-fit (FR-019, SC-005).
  - Documented defaults stated: flat list, active monitor only, MRU order (FR-023).
- All checklist items still pass (16/16, unchanged). Spec is ready for `/speckit-plan`.
