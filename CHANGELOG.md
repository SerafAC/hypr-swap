# Changelog

Everything worth knowing about each release, written for the people who use the program rather than
for the people who wrote it. Entries answer what you can now do, what changed under you, and what
broke — never which functions moved.

The form is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): the section headings are
`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed` and `Security`, and a section with nothing in
it is left out. The versions are [semantic](https://semver.org/), and what counts as a breaking
change is not a matter of taste here — it is defined over the whole contract surface, the shortcut
names, the configuration keys, the style values, the command line, the exit codes and the
diagnostic subjects, in
[`specs/003-oss-release-readiness/contracts/versioning.md`](specs/003-oss-release-readiness/contracts/versioning.md).

This file is written by hand as changes land, and is not derived from commit messages: there is no
commit-message convention to learn. A change that alters what a user can do adds a line to
`[Unreleased]` in the same pull request — `./scripts/checks.sh` fails a change to `src/` that does
not. Documentation- and specification-only changes need no entry. Nothing but the release workflow
edits the released sections.

## [Unreleased]

### Added

- Alt-Tab style workspace switching for Hyprland, driven by two global shortcuts you bind in
  `hyprland.conf`: hold the modifier to browse workspaces in an overlay, release it to switch.
  Bound to a bare key with no modifier to release, the overlay stays open in sticky mode instead.
- A second shortcut that swaps the highlighted workspace with the one on the focused monitor,
  moving both, so a workspace can be pulled across monitors without losing the other.
- Empty workspaces are offered alongside occupied ones, and the last entry creates a new workspace,
  so switching to somewhere that does not exist yet takes the same two keys as everywhere else.
- Two presentations of the overlay — a vertical list and a grid of miniatures — selected with
  `presentation` in the configuration file.
- Window miniatures carry their program's real icon, taken from the icon set the desktop is already
  configured to use, or `icon_set` to name a different one. Without an icon set installed, every
  window shows a built-in placeholder and nothing else changes.
- A configuration file at `~/.config/hypr-swap/config.toml`, every setting optional: ordering,
  presentation, the workspaces offered, five built-in colour themes and per-value overrides for
  the eleven colours, the font and the ten geometry values.
- `--config <path>` to read a different configuration file, `--version`, and `--help`, whose usage
  text carries the bind lines so a user who has the binary has the instructions.
- Problems are reported on standard error and, where the failure is one only the user can fix,
  as a desktop notification through `notify-send`.
