---
title: hypr-swap
description: An Alt-Tab-style workspace switcher with cross-monitor swapping for Hyprland.
---

An Alt-Tab-style workspace switcher with cross-monitor swapping for
[Hyprland](https://hyprland.org/). Hold a hotkey and an overlay lists every workspace with the
windows it contains; tap to move the highlight, release to switch. Selecting a workspace that
lives on another monitor swaps the two.

![The overlay's flat list: one line per workspace, each with its windows and their program icons](./assets/overlay-list.png)

It is a single Rust binary that runs as a user-session daemon. It performs no network access of
any kind and collects no telemetry — including this site, whose search index is built at build
time and queried in your browser.

## Where to go

**[User guide](./user/install.md)** — installing it, binding the shortcuts, every setting, the
appearance catalogue, program icons, and what to do when something looks wrong.

**[Developer guide](./dev/architecture.md)** — how it is put together, the spec-driven workflow it
is developed with, the test tiers, what verifies each requirement, and how a release is cut.

If you are deciding whether to install it at all, the [README](https://github.com/SerafAC/hypr-swap#readme)
is the shorter answer.

## Which version this documents

This site documents **the `master` branch** — the latest release, plus whatever has been merged
since. There are no per-release snapshots: one site, describing the code beside it. Behaviour that
is on `master` but not yet in a published version is marked in place, like this:

> **Unreleased.** Behaviour described this way is on the default branch and is not in any
> published version yet.

No page here names a version, so none can go stale: what each release changed is in the
[changelog](./changelog.md), and the published versions themselves are on the
[releases page](https://github.com/SerafAC/hypr-swap/releases).

## Getting help

[Open an issue](https://github.com/SerafAC/hypr-swap/issues) — the bug form asks for the
environment facts that make a report actionable, and
[troubleshooting](./user/troubleshooting.md) covers the failures that turn out not to be bugs.
Vulnerabilities go through the private channel in `SECURITY.md` instead.
