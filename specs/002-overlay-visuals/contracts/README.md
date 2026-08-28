# Contracts: Overlay Visuals

The external surface this feature adds or changes. Feature 001's contracts
([shortcuts](../../001-workspace-swap-overlay/contracts/shortcuts.md),
[cli](../../001-workspace-swap-overlay/contracts/cli.md),
[diagnostics](../../001-workspace-swap-overlay/contracts/diagnostics.md),
[compositor-ipc](../../001-workspace-swap-overlay/contracts/compositor-ipc.md)) are unchanged — this
feature adds no shortcut, no CLI option, no IPC command, and no diagnostic format.

| Document | Covers |
|---|---|
| [config.md](./config.md) | The four visual settings added to the configuration file |
| [style-values.md](./style-values.md) | The FR-061 catalogue: every value, form, range, default |
| [icon-lookup.md](./icon-lookup.md) | Class → entry → icon name → file, and the E2E fixture format |

## Requirement trace

Every functional requirement, the module that owns it, and how it is verified. "Unit" means an
in-module `#[cfg(test)]` test; E2E names are from the coverage table in [plan.md](../plan.md).

| FR | Owner | Contract | Verified by |
|---|---|---|---|
| FR-035 | `icons/mod.rs`, `ui/render.rs` | icon-lookup | `e2e_icons_in_flat_list`, `e2e_icons_in_grid_miniatures` |
| FR-036 | `ui/layout.rs` | style-values | Unit (row height invariant) + `e2e_icons_keep_row_height_and_count` |
| FR-036a | `ui/render.rs` | — | `e2e_icons_truncate_names_sooner` |
| FR-037 | `ui/render.rs` | — | `e2e_icons_in_grid_miniatures` |
| FR-038 | `ui/layout.rs` | — | Unit (shedding thresholds) + `e2e_miniature_drops_title_then_icon` |
| FR-039 | `icons/decode.rs` | icon-lookup | Unit (aspect ratio, device-size request) + quickstart |
| FR-040 | `icons/entries.rs` | icon-lookup | Unit (five-step ladder) + `e2e_icons_in_flat_list` |
| FR-040a | `icons/decode.rs` | icon-lookup | Unit + `e2e_vector_icon_renders`, `e2e_raster_icon_renders` |
| FR-041 | `icons/mod.rs` | icon-lookup | Unit + `e2e_icon_placeholder_for_unknown_program`, `e2e_no_icon_set_installed` |
| FR-042 | `icons/mod.rs` | icon-lookup | Unit (resolve-once) + `e2e_icon_resolved_once_per_program` |
| FR-043 | `main.rs`, `icons/mod.rs` | icon-lookup | `e2e_icons_resolved_before_overlay_opens` |
| FR-043a | `ui/render.rs` | icon-lookup | `e2e_icons_resolved_before_overlay_opens` |
| FR-043b | `icons/mod.rs` | icon-lookup | `e2e_no_icon_cache_on_disk` |
| FR-044 | `icons/decode.rs`, `diag.rs` | icon-lookup | Unit + `e2e_malformed_icon_reported_once` |
| FR-045 | `theme.rs` | style-values | Unit (colour parsing) + `e2e_builtin_theme_applies` |
| FR-046 | `theme.rs`, `ui/render.rs` | style-values | `e2e_font_override_applies`, `e2e_missing_font_substitutes` |
| FR-047 | `theme.rs`, `ui/layout.rs` | style-values | Unit + `e2e_geometry_override_resizes`, `e2e_grid_geometry_override` |
| FR-048 | `ui/mod.rs` | config | `e2e_theme_on_all_monitors` |
| FR-049 | `theme.rs` | style-values | Unit (theme is colours only) + `e2e_theme_switch_does_not_move_layout` |
| FR-049a | `theme.rs` | style-values | Unit (defaults equal today's constants) + `e2e_default_appearance_unchanged` |
| FR-050 | `theme.rs` | config | Unit (precedence chain) + `e2e_colour_override_wins_over_theme`, `e2e_overrides_without_theme` |
| FR-051 | `ui/render.rs` | style-values | Unit (placeholder tint only) + quickstart |
| FR-052 | `ui/layout.rs` | style-values | Unit (slot follows text height) |
| FR-053 | `ui/layout.rs` | style-values | Unit (invariants under every valid range) + `e2e_geometry_override_still_caps_and_scrolls` |
| FR-054 | `theme.rs` | style-values | Unit (clamping) + `e2e_out_of_range_geometry_clamped` |
| FR-055 | `ui/layout.rs` | style-values | Unit (scale round-trip) + `e2e_geometry_scales_with_monitor` |
| FR-056 | `config.rs`, `ui/render.rs` | config | `e2e_icons_disabled_matches_pre_feature` |
| FR-057 | `icons/iconset.rs` | icon-lookup | Unit + `e2e_icon_set_selected`, `e2e_unknown_icon_set_falls_back` |
| FR-058 | `config.rs`, `theme.rs` | config | `e2e_unknown_theme_falls_back` |
| FR-059 | `config.rs`, `diag.rs` | config | Unit + `e2e_invalid_value_falls_back_alone` |
| FR-060 | `config.rs` | config | `e2e_visual_settings_need_restart` |
| FR-061 | this directory | style-values | Unit (catalogue matches `theme.rs`) |

## What this feature does **not** add to the external surface

Recorded so the absences read as decisions (Principle II):

- **No new shortcut and no new CLI option.** Appearance is configuration, not a runtime action.
- **No new diagnostic format.** Every message goes through `diag.rs` in feature 001's existing
  record format; only the content is new.
- **No IPC command.** Nothing about drawing requires talking to the compositor.
- **No query interface.** The env-gated paint records of research R22 are a test hook, inert in
  normal operation — deliberately not a supported surface.
- **No on-disk artefact.** FR-043b forbids an icon cache; nothing is written anywhere.
