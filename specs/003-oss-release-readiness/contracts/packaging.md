# Contract: Packages and the distribution matrix

Three channels, `x86_64` only (FR-106, FR-107, FR-109). Everything else — other architectures,
other families — is served by building from source, and the documentation says so rather than
leaving it implied.

## What a package installs (FR-109, FR-066)

| File | Debian family | RPM family |
|---|---|---|
| Binary | `/usr/bin/hypr-swap` | `/usr/bin/hypr-swap` |
| Licence | `/usr/share/doc/hypr-swap/copyright` | `/usr/share/licenses/hypr-swap/LICENSE` |
| Third-party account | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` |
| README | `/usr/share/doc/hypr-swap/README.md` | `/usr/share/doc/hypr-swap/README.md` |
| Changelog | `/usr/share/doc/hypr-swap/changelog.gz` | `/usr/share/doc/hypr-swap/CHANGELOG.md` |

No unit file, no service registration, no configuration file is installed: the daemon is started
by the user's `hyprland.conf` and runs with no configuration at all (FR-023).

## Declared dependencies

| | Debian family | RPM family |
|---|---|---|
| Required | `libcairo2`, `libpango-1.0-0`, `libpangocairo-1.0-0`, `libc6` | `cairo`, `pango` |
| Recommended / Suggested | `libnotify-bin` (for `notify-send`) | `libnotify` |
| Not declared | Hyprland — a user installing this has it, and pinning a compositor version in a package would refuse installs the program supports | |

Icon sets are not a dependency: without one, every window shows the placeholder (FR-041), which is
what the README's optional-dependency table says.

## Where the packages are built, and what they run on (FR-109a)

**The rule**, which is what the documentation states: each package is built in a container of the
**oldest still-supported release** of its family, so that one package runs across that family's
currently supported releases. Both are then installed and run in a clean container of the oldest
*and* the current release before the release is published (SC-039).

**The matrix, as of 2026-08-30** — the one place the concrete numbers live, and a named item on
the release checklist (FR-109a): confirm before each release that the oldest release named here is
still the oldest one its family supports, and raise it if it is not:

| Family | Built on | Verified on |
|---|---|---|
| Debian / Ubuntu | Ubuntu 22.04 LTS | Ubuntu 22.04 LTS, current Ubuntu LTS, Debian stable |
| Fedora / RPM | Fedora 43 | Fedora 43 and current Fedora |
| Arch | n/a — built from source by the user | current Arch |

This is comfortable rather than tight: the crates' own minimums are **cairo 1.14, pango 1.40,
glib 2.56** ([research.md](../research.md) R33, verified from the `system-deps` metadata), while
Ubuntu 22.04 already carries cairo 1.16, pango 1.50 and glib 2.72. The binding constraint is glibc,
which is why the build container is the oldest supported release rather than the newest.

## Metadata

Both recipes read `Cargo.toml`, so the version has one definition (FR-105). `Cargo.toml` also
carries what a packager and a source index expect (FR-065): `description`, `license`,
`repository`, `documentation`, `homepage`, `keywords`, `categories`, `readme`.

## The Arch recipe (FR-107)

`packaging/aur/PKGBUILD` builds from the release's **source archive**, not from the default
branch, so it has an integrity value to check. `pkgver` and `sha256sums` are rewritten by the
release workflow from the artefacts it just published, which is what keeps the recipe from falling
behind. `depends`: `cairo`, `pango`; `makedepends`: `rust`, `pkgconf`; `optdepends`: `libnotify`,
an icon set. It installs the same files to Arch's conventional locations
(`/usr/share/licenses/hypr-swap/LICENSE`).

## What a packager needs (FR-111)

Every release's notes carry the dependency list above with minimum versions, the build command,
and this install map, so that a distribution's packager never has to ask.
