# Contract: Command line (delta to feature 001)

Feature 001's [`contracts/cli.md`](../../001-workspace-swap-overlay/contracts/cli.md) remains the
authority on the command line. This page states only what feature 003 adds. Both are part of the
1.0.0 stable surface: changing anything here is a **major** version change
([versioning.md](./versioning.md)).

## Options (added)

```text
    --environment     Print the facts a bug report needs, and exit
```

The full option set becomes `[--config <path>] [--environment] [--version] [--help]`. As with
`--version` and `--help`, `--environment` prints and exits **0** without starting the daemon, and
it is honoured wherever it appears on the line. Two of these options together is a usage error
(exit **2**), unchanged.

## `--version` output (changed shape, same line)

```text
hypr-swap <version>
```

`<version>` is `CARGO_PKG_VERSION` for a build made from a release tag, and
`CARGO_PKG_VERSION+<git describe --tags --always --dirty>` otherwise (FR-103, FR-104):

```text
hypr-swap 1.0.0                                 # built from the v1.0.0 tag
hypr-swap 1.0.0+v1.0.0-14-gabc1234              # 14 commits after it
hypr-swap 1.0.0+v1.0.0-14-gabc1234-dirty        # …with local edits
hypr-swap 1.0.0                                 # from a source archive: no git, no suffix
```

The same string appears in the usage text's first line and in the FR-112 start record, because all
three read `hypr_swap::version()`.

## `--environment` output

One `key: value` line per fact, in this order, to **stdout**. Absent or unavailable values are
printed with an explicit word rather than omitted, so a pasted report has no silent gaps.

```text
hypr-swap:    1.0.0+v1.0.0-14-gabc1234
hyprland:     0.56.2 (v0.56.2)
config:       /home/user/.config/hypr-swap/config.toml (present)
settings:     presentation = "grid", theme = "light"
icon-set:     Papirus-Dark
notify-send:  present
```

Rules:

- `hyprland` is the `version` field of the compositor's `j/version` response, with its `tag` in
  brackets; `unavailable` when the compositor cannot be reached, which is itself the answer to
  most reports.
- `settings` lists **only settings that differ from their defaults**, in the file's own key names;
  `defaults` when none do. The file's contents are never printed (FR-071).
- `icon-set` is the set that was actually resolved, not the one configured — those differ exactly
  when something is wrong — or `none`.
- Nothing else is read: no window titles, no paths outside the configuration file and the icon
  set, no network access.

The bug report form's environment field asks for this block verbatim (FR-097, FR-116).

## Exit codes

Unchanged from 001: `0` success, `2` usage error, `3` compositor unreachable at start-up or a
second instance already running. Exit codes are part of the stable surface.
