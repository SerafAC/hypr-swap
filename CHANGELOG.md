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

## [1.0.0] - 2026-09-12

### Added

- Alt-Tab style workspace switching for Hyprland, driven by two global shortcuts you bind in
  `hyprland.conf`: hold the modifier to browse workspaces in an overlay, release it to switch.
  Bound to a bare key with no modifier to release, the overlay stays open in sticky mode instead.
- Selecting a workspace that lives on another monitor swaps the two, moving both, so a workspace
  can be pulled across monitors without losing the one you were on. A swap that cannot be completed
  is rolled back whole and reported, rather than leaving the monitors half-moved.
- A second shortcut that jumps to the lowest-numbered unused workspace on the focused monitor, so
  reaching somewhere that does not exist yet takes one key rather than a detour through the overlay.
  It does nothing when you are already on an empty workspace.
- Empty workspaces are offered in the overlay alongside occupied ones, so somewhere you have not
  used yet is reachable the same way as everywhere else.
- Two presentations of the overlay — a vertical list and a grid of miniatures — selected with
  `presentation` in the configuration file.
- Window miniatures carry their program's real icon, taken from the icon set the desktop is already
  configured to use, or `icon_set` to name a different one. Without an icon set installed, every
  window shows a built-in placeholder and nothing else changes.
- A configuration file at `~/.config/hypr-swap/config.toml`, every setting optional: ordering,
  presentation, the workspaces offered, two built-in colour themes and per-value overrides for
  the eleven colours, the font and the ten geometry values.
- `--config <path>` to read a different configuration file, `--version`, and `--help`, whose usage
  text carries the bind lines so a user who has the binary has the instructions.
- Problems are reported on standard error and, where the failure is one only the user can fix,
  as a desktop notification through `notify-send`.
- The daemon says when it started and why it stopped. One record on start-up naming the version it
  is running, and one on the way out naming the cause — the signal that arrived, or the failure
  that stopped it before it ever came up. Under `exec-once` both land in the compositor's log, so
  "when did it restart, and why" is answerable after the fact rather than only while watching.
- `--environment` prints what the daemon can actually see — its version, the compositor it found,
  the session, the configuration file in use and whether an icon set and a notification service are
  available. It is what the bug report form asks you to paste, and it never prints the contents of
  your configuration file. Where a value cannot be determined it says so in words rather than
  leaving a blank.
- A compositor older than the supported minimum is named as such at start-up, with the version
  found and the version needed, instead of failing later in a way that looks like a bug in the
  program.
- An `hypr-swap-bin` Arch package alongside `hypr-swap`, installing the release's prebuilt
  binary instead of compiling it, so installing on Arch needs no Rust toolchain and no build. The
  two install the same files and conflict with each other, so you install whichever you prefer and
  switching is an ordinary replace. The prebuilt one installs the binary unmodified — byte-for-byte
  the file its published checksum covers.

### Fixed

- The packages now declare `libxkbcommon`, which the program links directly and none of them
  listed. Nothing else pulls it in, so installing on a machine that did not already have it — any
  clean system that is not already running Hyprland — would have produced a binary that could not
  start. `glib2` is now declared on the same grounds.

- After swapping a workspace in from another monitor, the next hold-and-release now bounces back to
  the workspace you just left. It used to land on whichever workspace the other monitor happened to
  be showing before the swap — a workspace you never visited — because the compositor reports a
  swap partly as a focus change over the monitor that is about to lose its workspace, and that was
  being read as you having gone there.
