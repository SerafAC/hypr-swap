---
title: Installing
description: Every published channel — the Debian and RPM packages, the AUR recipe, the prebuilt binary, and building from source.
---

Five channels, and the right one depends mostly on your distribution. Everything below installs the
same `x86_64` binary; on any other architecture, build from source.

## What it needs first

| | |
|---|---|
| Compositor | Hyprland `>= 0.55`, on Wayland |
| System libraries | cairo, pango, pangocairo |
| To build from source | Rust `1.96` or newer |

Two dependencies are optional and each costs you only the thing it provides: without an installed
icon set every window shows the built-in placeholder, and without `notify-send` problems are
reported on standard error only. Hyprland itself is deliberately **not** declared as a package
dependency — pinning a compositor version in a package would refuse installs the program supports.

## Verify what you downloaded

Every release publishes a `SHA256SUMS` file covering all of its artefacts. Before installing
anything from the [releases page](https://github.com/SerafAC/hypr-swap/releases):

```bash
sha256sum -c SHA256SUMS
```

## Debian, Ubuntu and derivatives

```bash
sudo apt install ./hypr-swap_<version>_amd64.deb
```

Installing the `.deb` through `apt` rather than `dpkg -i` is what pulls in `libcairo2`,
`libpango-1.0-0`, `libpangocairo-1.0-0` and `libc6` for you. `libnotify-bin` is a *recommendation*,
so it comes along by default and can be declined.

The package is built in a container of the **oldest still-supported Ubuntu LTS**, so one package
runs across that family's currently supported releases — it is verified on that oldest release, on
the current Ubuntu LTS, and on Debian stable before each release is published.

## Fedora, RHEL and derivatives

```bash
sudo dnf install ./hypr-swap-<version>.x86_64.rpm
```

Requires `cairo` and `pango`; `libnotify` is a suggestion. Built in a container of the oldest
supported Fedora release and verified there and on the current one.

## Arch

From the AUR, with whichever helper you use:

```bash
paru -S hypr-swap    # or: yay -S hypr-swap
```

The recipe builds from the release's **source archive** rather than from the default branch, so it
has an integrity value to check, and its `pkgver` and `sha256sums` are rewritten by the release
workflow from the artefacts it has just published — the recipe cannot fall behind the release.
`optdepends` names `libnotify` and an icon set.

## The prebuilt binary

For a distribution with no package of its own, and for trying it without installing anything:

```bash
curl -LO https://github.com/SerafAC/hypr-swap/releases/latest/download/SHA256SUMS
curl -LO https://github.com/SerafAC/hypr-swap/releases/latest/download/hypr-swap-<version>-x86_64
sha256sum -c --ignore-missing SHA256SUMS
chmod +x hypr-swap-<version>-x86_64
sudo install -Dm755 hypr-swap-<version>-x86_64 /usr/local/bin/hypr-swap
```

You supply cairo, pango and pangocairo yourself — nothing checks for them until the daemon starts,
and a missing one shows up as a dynamic-link failure.

## From source

The supported path on every architecture and distribution that has no package:

```bash
git clone https://github.com/SerafAC/hypr-swap.git
cd hypr-swap
cargo build --release
sudo install -Dm755 target/release/hypr-swap /usr/local/bin/hypr-swap
```

You need the cairo, pango and pangocairo **development** packages, not just the runtime libraries —
`libcairo2-dev libpango1.0-dev` on Debian family, `cairo-devel pango-devel` on Fedora,
`cairo pango` on Arch. Building the project as a contributor, rather than installing it, is
[`DEVELOPMENT.md`](https://github.com/SerafAC/hypr-swap/blob/master/DEVELOPMENT.md)'s subject.

## Where each channel puts things

| File | Debian family | RPM family |
|---|---|---|
| Binary | `/usr/bin/hypr-swap` | `/usr/bin/hypr-swap` |
| Licence | `/usr/share/doc/hypr-swap/copyright` | `/usr/share/licenses/hypr-swap/LICENSE` |
| Third-party account | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` |
| README | `/usr/share/doc/hypr-swap/README.md` | `/usr/share/doc/hypr-swap/README.md` |
| Changelog | `/usr/share/doc/hypr-swap/changelog.gz` | `/usr/share/doc/hypr-swap/CHANGELOG.md` |

No package installs a unit file, registers a service, or drops a configuration file. The daemon is
started by your own `hyprland.conf` and runs perfectly well with no configuration at all.

## After installing

Nothing runs yet — the daemon needs to be started and given two key combinations. That is
[binding the shortcuts](./binds.md), and it is two lines. If something looks wrong afterwards,
[troubleshooting](./troubleshooting.md) covers the failures that turn out not to be bugs.
