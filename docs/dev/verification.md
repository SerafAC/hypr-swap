---
title: Verification coverage
description: Every requirement in the project, the tier that verifies it, and the named test or check that does the verifying.
---

**Every requirement in this project has a named verification tier, and none is unknown.** That is
itself a requirement, and this page is the published statement of it.

Five tiers, in the order you meet them:

| Tier | What it means |
|---|---|
| **Unit** | a decision, checked without a compositor or a display |
| **E2E** | the running program, against a real nested Hyprland |
| **CI** | a check that gates a pull request |
| **Release** | a precondition or a step of the release workflow |
| **Inspection** | a named item on the release checklist, walked by a person |

The commands behind the first three are in [testing](./testing.md).

Inspection is not a shrug. It is what a requirement gets when only a person can judge whether it
holds — whether the README actually answers a newcomer's questions, whether a security channel
actually reaches someone. Each one is an item on a checklist that is walked before every release,
not a gap.

## Requirements of feature 003 — release readiness

This table is not copied from the plan; it **is** the plan's table, included at build time from
[`specs/003-oss-release-readiness/plan.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/003-oss-release-readiness/plan.md).
The published statement and the planning document are the same bytes, so they cannot diverge.

::include[../../specs/003-oss-release-readiness/plan.md#verification-tier-for-every-requirement]

## Requirements of features 001 and 002 — the program itself

Derived from those features' own E2E coverage mappings in
[`001-workspace-swap-overlay/plan.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/001-workspace-swap-overlay/plan.md)
and
[`002-overlay-visuals/plan.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/002-overlay-visuals/plan.md),
which remain authoritative for what each test drives. The overwhelming majority are **E2E**, because
the overwhelming majority describe what the program does when a real person presses a real key
against a real compositor — and that is the only place such a claim can honestly be checked.

| Requirement | Tier | Verified by |
|---|---|---|
| FR-001 overlay opens on the switcher shortcut | E2E | `e2e_activate_same_monitor` |
| FR-002 stays open while the modifier is held | E2E | `e2e_activate_same_monitor` |
| FR-002a holds exclusive keyboard focus | E2E | `e2e_focus_returns_on_close` |
| FR-003 a tap advances the highlight | E2E | `e2e_navigation_wraps_and_reverses`, `e2e_repeat_trigger_advances` |
| FR-004 the highlight moves backwards | E2E | `e2e_navigation_wraps_and_reverses` |
| FR-004a the in-overlay keys are fixed | E2E | `e2e_navigation_wraps_and_reverses` |
| FR-005 commits on release | E2E | `e2e_activate_same_monitor`, `e2e_fast_tap_commits` |
| FR-006 cancellable without changing workspaces | E2E | `e2e_cancel_leaves_state` |
| FR-007 lists every ordinary workspace | E2E | `e2e_grid_empty_workspace`, `e2e_special_workspaces_excluded` |
| FR-008 indicates the highlight and the active workspace | Unit | `ui/layout.rs` highlight-index tests; a visual property 001 R14 rules out asserting by screenshot |
| FR-008a entry order is configurable | E2E | `e2e_mru_order_and_highlight`, `e2e_configured_order` |
| FR-008b MRU opens the highlight on the second entry | E2E | `e2e_mru_order_and_highlight`, `e2e_configured_order` |
| FR-008c an activation history is maintained | E2E | `e2e_external_switch_tracked` |
| FR-008d never-active workspaces still appear | E2E | `e2e_mru_order_and_highlight` |
| FR-009 same-monitor selection switches | E2E | `e2e_activate_same_monitor`, `e2e_swap_single_monitor_degrades` |
| FR-010 cross-monitor selection swaps | E2E | `e2e_swap_active_workspaces`, `e2e_swap_inactive_target` |
| FR-011 selecting the active workspace does nothing | E2E | `e2e_select_active_is_noop` |
| FR-012 a swap loses no window | E2E | `e2e_swap_active_workspaces` |
| FR-013 both monitors active after a swap | E2E | `e2e_swap_active_workspaces` |
| FR-013a a swap is all-or-nothing | E2E | `e2e_swap_rollback_on_failure` |
| FR-013b a rolled-back swap is reported | E2E | `e2e_swap_rollback_on_failure` |
| FR-013c a failed rollback is reported | Unit | injected double failure in `actions.rs` — provoking it end-to-end would mean corrupting the compositor |
| FR-014 flat list presentation | E2E | `e2e_list_shows_window_names` |
| FR-015 grid presentation | E2E | `e2e_grid_miniature_layout` |
| FR-015a miniatures are schematic and to scale | E2E | `e2e_grid_miniature_layout`, `e2e_grid_offscreen_workspace` |
| FR-015b long titles truncate visibly | E2E | `e2e_title_truncation` |
| FR-016 presentation is configurable, both commit alike | E2E | `e2e_grid_commit_matches_list` |
| FR-017 placement is configurable | E2E | `e2e_placement_all_monitors` |
| FR-018 renders above all other windows | E2E | `e2e_above_fullscreen` |
| FR-019 fixed readable size; scrolls rather than shrinks | E2E | `e2e_scrolls_many_workspaces`, `e2e_overlay_scales_with_the_monitor` |
| FR-020 new-workspace shortcut jumps to the lowest unused | E2E | `e2e_new_workspace_lowest_unused` |
| FR-021 no-op when already on an empty one | E2E | `e2e_new_workspace_noop_when_empty` |
| FR-022 exactly two named shortcuts | E2E | `the_application_registers_both_named_shortcuts` |
| FR-022a commit-on-release is mandatory | E2E | `e2e_activate_same_monitor`, `a_held_modifier_with_taps_is_delivered_as_one_gesture` |
| FR-022b documents the exact bind lines; starts unbound | E2E + Unit | `e2e_unbound_shortcut_is_harmless` for starting unbound; `the_documented_bind_lines_are_the_ones_this_module_generates` for the documented lines |
| FR-022c sticky mode without a modifier | E2E | `e2e_sticky_mode_commits_on_enter` |
| FR-023 documented defaults with no configuration file | E2E | `e2e_defaults_without_config` |
| FR-024 invalid values reported, per-setting fallback | E2E | `e2e_invalid_config_falls_back` |
| FR-025 unreachable compositor reported, non-zero exit | E2E | `e2e_no_compositor_at_start` |
| FR-025a a second instance is refused | E2E | `e2e_second_instance_refuses_to_start` |
| FR-026 external changes are reflected | E2E | `e2e_external_switch_tracked` |
| FR-026a connection loss is survived | E2E | `e2e_reconnects_after_restart` |
| FR-026b state is rebuilt on reconnect | E2E | `e2e_reconnects_after_restart` |
| FR-026c history is discarded and rebuilt | E2E | `e2e_reconnects_after_restart` |
| FR-026d disconnected retries are bounded | E2E | `e2e_no_overlay_while_disconnected` |
| FR-027 a vanished target cancels the commit | E2E | `e2e_vanished_target_cancels`, `e2e_monitor_removed_degrades` |
| FR-028 re-triggering does not stack overlays | E2E | `e2e_repeat_trigger_advances` |
| FR-029 every diagnostic goes to standard error | E2E | `e2e_invalid_config_falls_back` |
| FR-030 notifications for what the user must act on | E2E | `e2e_invalid_config_falls_back`, `e2e_version_and_help` |
| FR-031 self-recovered conditions are not notified | E2E | asserted as part of `e2e_reconnects_after_restart`; the notification policy itself is unit-tested in `diag.rs` |
| FR-032 runs on without a notification service | E2E | `e2e_no_notification_daemon` |
| FR-033 starts with no arguments; --version/--help | E2E | `e2e_version_and_help` |
| FR-034 --config names an alternative file | E2E | `e2e_explicit_config_path_is_used_and_must_exist` |
| FR-035 every window carries its program icon | E2E | `e2e_icons_in_flat_list` |
| FR-036 the icon precedes the name in the list | E2E | `e2e_icons_in_flat_list`, `e2e_icons_keep_row_height_and_count` |
| FR-036a icons occupy row space, truncating sooner | E2E | `e2e_icons_truncate_names_sooner` |
| FR-037 icons are drawn inside grid rectangles | E2E | `e2e_icons_in_grid_miniatures` |
| FR-038 a small rectangle drops title, then icon | E2E | `e2e_miniature_drops_title_then_icon` |
| FR-039 icons scale without distorting aspect | Unit | `icons/decode.rs` against computed rectangles (001 R14) |
| FR-040 the icon follows the window's program identity | E2E | `e2e_icons_in_flat_list` |
| FR-040a both raster and vector icons decode | E2E | `e2e_vector_icon_renders`, `e2e_raster_icon_renders` |
| FR-041 a placeholder where nothing resolves | E2E | `e2e_icon_placeholder_for_unknown_program`, `e2e_no_icon_set_installed` |
| FR-042 resolution happens once per program, cached | E2E | `e2e_icon_resolved_once_per_program` |
| FR-043 resolution precedes the overlay opening | E2E | `e2e_icons_resolved_before_overlay_opens` |
| FR-043a an unresolved icon shows the placeholder | E2E | `e2e_icons_resolved_before_overlay_opens` |
| FR-043b icons are held in memory, never written to disk | E2E | `e2e_no_icon_cache_on_disk` |
| FR-044 a malformed icon is reported once | E2E | `e2e_malformed_icon_reported_once` |
| FR-045 eleven colours are configurable | E2E | `e2e_builtin_theme_applies` |
| FR-046 font family and text size are configurable | E2E | `e2e_font_override_applies` |
| FR-047 the geometry values are configurable | E2E | `e2e_geometry_override_resizes`, `e2e_grid_geometry_override` |
| FR-048 a theme applies to both presentations, every monitor | E2E | `e2e_builtin_theme_applies`, `e2e_theme_on_all_monitors` |
| FR-049 built-in themes selectable by name | E2E | `e2e_theme_switch_does_not_move_layout` |
| FR-049a the default appearance is unchanged, byte for byte | E2E | `e2e_refactor_is_pixel_neutral`, `e2e_default_appearance_unchanged` |
| FR-050 any single style value is overridable | E2E | `e2e_colour_override_wins_over_theme`, `e2e_overrides_without_theme` |
| FR-051 program artwork is never recoloured | Unit | `icons/decode.rs` — artwork is drawn as supplied (001 R14) |
| FR-052 the icon slot follows the themed text height | Unit | `ui/layout.rs` icon-slot tests against computed rectangles (001 R14) |
| FR-053 layout guarantees survive icons and theming | E2E | `e2e_builtin_theme_applies`, `e2e_geometry_override_still_caps_and_scrolls` |
| FR-054 every geometry value has a documented range, clamped | E2E | `e2e_out_of_range_geometry_clamped` |
| FR-055 themed geometry uses the same units and scaling | E2E | `e2e_geometry_override_resizes`, `e2e_geometry_scales_with_monitor` |
| FR-056 icons can be turned off | E2E | `e2e_icons_disabled_matches_pre_feature` |
| FR-057 the icon set is selectable, independent of theme | E2E | `e2e_icon_set_selected`, `e2e_unknown_icon_set_falls_back` |
| FR-058 an unknown theme name falls back and is reported | E2E | `e2e_unknown_theme_falls_back` |
| FR-059 an unparseable visual value falls back alone | E2E | `e2e_invalid_value_falls_back_alone` |
| FR-060 visual settings are read once, at start-up | E2E | `e2e_visual_settings_need_restart` |
| FR-061 every visual setting is documented | Unit | the catalogue walk in `theme.rs` against `contracts/style-values.md` |

### The seven that are not end-to-end, and why

Six of them are **pixel properties** — that the highlight is visible, that an icon keeps its aspect
ratio, that artwork is not recoloured, that the icon slot follows the text height. Asserting those
end-to-end would mean comparing screenshots, which was considered and rejected as brittle across
fonts, scaling and compositor versions. They are unit-tested against computed rectangles instead,
and confirmed by eye on the quickstart walk.

One, **FR-013c**, is the failure of a rollback — provoking it end-to-end would mean corrupting the
compositor mid-swap, so it is unit-tested against an injected double failure.

**FR-061** is documentation completeness, which no end-to-end test could see. It is the catalogue
walk: a unit test reads the published style catalogue and compares it against the `const` tables
`theme.rs` actually resolves from. Adding a setting without documenting it fails
`cargo test --lib` — which is the same mechanism that keeps the [configuration](../user/configuration.md)
and [appearance](../user/styling.md) pages honest.

## What is measured rather than tested

Three of the project's success criteria are human measurements, not checks: how long it takes a
newcomer to get an overlay on screen, how long to get a test suite running, and whether a user can
assemble a custom appearance from the documentation alone. These are walked once along the published
path and recorded, in the same way feature 001 measures its own usability criterion. A number that
can only be got by watching a person is not improved by automating something adjacent to it.
