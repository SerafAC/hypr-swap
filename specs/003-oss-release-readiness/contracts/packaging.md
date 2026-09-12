# Contract: Packages and the distribution matrix

Four packages across three families, `x86_64` only (FR-106, FR-107, FR-107a, FR-109). Everything
else — other architectures, other families — is served by building from source, and the
documentation says so rather than leaving it implied.

Arch has **two**: `hypr-swap` compiles the release's source archive, `hypr-swap-bin` installs the
binary that same release published. They install identical files and declare each other as
conflicts, so a user picks one ([research.md](../research.md) R49).

## What a package installs (FR-109, FR-066)

| File | Debian family | RPM family | Arch (both recipes) |
|---|---|---|---|
| Binary | `/usr/bin/hypr-swap` | `/usr/bin/hypr-swap` | `/usr/bin/hypr-swap` |
| Licence | `/usr/share/doc/hypr-swap/copyright` | `/usr/share/licenses/hypr-swap/LICENSE` | `/usr/share/licenses/hypr-swap/LICENSE` |
| Third-party account | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` |
| README | `/usr/share/doc/hypr-swap/README.md` | `/usr/share/doc/hypr-swap/README.md` | `/usr/share/doc/hypr-swap/README.md` |
| Changelog | `/usr/share/doc/hypr-swap/changelog.gz` | `/usr/share/doc/hypr-swap/CHANGELOG.md` | `/usr/share/doc/hypr-swap/CHANGELOG.md` |

`hypr-swap-bin` installs this same set, which is why it fetches the release's source archive
alongside the binary: the binary asset carries no licence or documentation of its own.

No unit file, no service registration, no configuration file is installed: the daemon is started
by the user's `hyprland.conf` and runs with no configuration at all (FR-023).

## Declared dependencies

These are the binary's direct `DT_NEEDED` set, stated explicitly rather than derived, so the
contract and the package cannot disagree:

| | Debian family | RPM family | Arch (both recipes) |
|---|---|---|---|
| Required | `libcairo2`, `libpango-1.0-0`, `libpangocairo-1.0-0`, `libglib2.0-0`, `libxkbcommon0`, `libc6` | `cairo`, `pango`, `glib2`, `libxkbcommon` | `cairo`, `pango`, `glib2`, `libxkbcommon` |
| Recommended / Suggested | `libnotify-bin` (for `notify-send`) | `libnotify` | `libnotify` (optdepend) |
| Not declared | Hyprland — a user installing this has it, and pinning a compositor version in a package would refuse installs the program supports | | |

**`libxkbcommon` is not optional and is not transitive.** Neither cairo nor pango pulls it in, and
the program links it directly for keyboard handling. Every machine running Hyprland happens to
have it, which is why only a clean container ever notices — see [research.md](../research.md) R49
for how it was found.

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
| Arch (`hypr-swap`) | n/a — built from source by the user | current Arch |
| Arch (`hypr-swap-bin`) | n/a — installs the release's own `x86_64` binary | current Arch |

This is comfortable rather than tight: the crates' own minimums are **cairo 1.14, pango 1.40,
glib 2.56** ([research.md](../research.md) R33, verified from the `system-deps` metadata), while
Ubuntu 22.04 already carries cairo 1.16, pango 1.50 and glib 2.72. The binding constraint is glibc,
which is why the build container is the oldest supported release rather than the newest.

## Metadata

Both recipes read `Cargo.toml`, so the version has one definition (FR-105). `Cargo.toml` also
carries what a packager and a source index expect (FR-065): `description`, `license`,
`repository`, `documentation`, `homepage`, `keywords`, `categories`, `readme`.

## The Arch recipes (FR-107, FR-107a)

Two, one directory each, both rewritten by the release workflow from the artefacts it just
published — `pkgver`, `pkgrel` and `sha256sums` — which is what keeps them from falling behind.
Each is pushed to its own AUR repository.

| | `packaging/aur/hypr-swap/PKGBUILD` | `packaging/aur/hypr-swap-bin/PKGBUILD` |
|---|---|---|
| Installs | compiles the release's **source archive** | the release's **prebuilt `x86_64` binary** |
| `source` | the archive | the binary, **and** the archive for the documentation files |
| `makedepends` | `rust`, `pkgconf` | none — nothing is compiled |
| `provides` / `conflicts` | — | `hypr-swap=$pkgver` / `hypr-swap` |
| `check()` | `cargo test --release --locked --lib` | runs the downloaded binary and matches `--version` against `pkgver` |
| `options` | default | `!strip`, `!debug` |

Neither builds from the default branch: both name a published release, so there is an integrity
value to check.

**`options=('!strip' '!debug')` on the prebuilt recipe is load-bearing.** makepkg strips binaries
by default, which would install a file that is not the artefact whose digest was verified —
measured, not assumed ([research.md](../research.md) R49). Installing exactly what was published
is the only thing that package is for.

## What a packager needs (FR-111)

Every release's notes carry the dependency list above with minimum versions, the build command,
and this install map, so that a distribution's packager never has to ask.
