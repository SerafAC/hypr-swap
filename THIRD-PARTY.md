# Third-party components

Everything in this repository is the project's own work under the MIT terms of [LICENSE](./LICENSE),
**except** the files listed below, which originate elsewhere and carry their own licences (FR-063).

The test a reader should be able to apply is this one: with only the source tree in front of them —
no network, no package index — they can name where every file came from and under what terms it may
be redistributed. That is why each file below also carries the same account in a comment at its own
head: a file copied out of the tree does not lose its provenance, and a tree read without this index
does not either.

## Files that ship inside the tree

### `protocols/hyprland-global-shortcuts-v1.xml`

| | |
|---|---|
| Origin | [hyprwm/hyprland-protocols](https://github.com/hyprwm/hyprland-protocols), `protocols/hyprland-global-shortcuts-v1.xml` |
| Revision | `5d6a3ee1474dc560fee59cd571c4c20920ca8b17` |
| Copyright | © 2022 Vaxry |
| Licence | BSD-3-Clause (the full text is the `<copyright>` element of the file itself) |

The Wayland protocol description for `hyprland-global-shortcuts-v1`, which is how the daemon
registers its two shortcuts with the compositor. It is vendored rather than depended on because
`wayland-scanner` expands the XML at compile time (see `build.rs`) and no crate publishes this
protocol. **Do not edit it**: to update, re-vendor from upstream and change the revision recorded
here and in the file's own header.

BSD-3-Clause requires that the copyright notice, the conditions and the disclaimer be reproduced in
source form and, for binary distribution, in the accompanying materials. The `<copyright>` element
satisfies the first; this file, shipped in every package (see below), satisfies the second.

### `assets/placeholder.svg`

| | |
|---|---|
| Origin | Written for this project — no upstream |
| Revision | n/a |
| Copyright | © 2026 SerafAC |
| Licence | MIT, the same as the rest of the project ([LICENSE](./LICENSE)) |

The generic icon shown for a program whose own icon could not be resolved (FR-041). It is listed
here even though it is the project's own work, because FR-063's test is that *every* path under
`protocols/` and `assets/` is attributable from the tree alone — "this one is ours" is an answer,
and an unlisted file is not.

## The dependency graph's licences

FR-064: a packager needs to judge what they are redistributing without auditing the graph
themselves. This section is that judgement, and it is enforced rather than asserted — `deny.toml`'s
`[licenses]` section allows exactly the list below and nothing else, and the gating `licenses` job
(`cargo deny check licenses`) fails on any dependency that introduces a licence outside it. A crate
added under different terms fails the merge gate; it does not quietly change what this page says.

**Every licence in the graph is permissive.** There is no copyleft anywhere in it — no GPL, LGPL,
MPL or EUPL — so linking the binary imposes no obligation to publish source, and redistribution in
binary form requires only that the notices be preserved.

| Licence | Where it appears |
|---|---|
| MIT | most of the tree, including every direct dependency |
| Apache-2.0 | commonly dual-licensed with MIT (the Rust ecosystem's usual pairing) |
| Apache-2.0 WITH LLVM-exception | `rustix`, `linux-raw-sys`, `target-lexicon` |
| BSD-2-Clause | `arrayref` |
| BSD-3-Clause | `tiny-skia`, `tiny-skia-path` (the rasteriser under `resvg`) |
| 0BSD | `adler2` |
| Zlib | `bytemuck`, `cursor-icon`, `miniz_oxide`, `xkeysym` |
| Unicode-3.0 | `unicode-ident` |
| Unlicense | `memchr` |

The list covers build-time and runtime dependencies alike, and development dependencies too: the
check reads the whole graph rather than the shipped subset, which is the stricter reading and the
one that needs no explanation of what was excluded.

**What this section does not cover.** The system libraries the binary links against — cairo, pango,
glib — are not vendored, not statically linked, and not redistributed by this project; they come
from the distribution, under their own terms (LGPL-2.1 for all three). The documentation site's npm
tree is likewise absent: `cargo-deny` reads Cargo metadata only, nothing in that tree ships to a
user or runs on their machine, and it is watched by Dependabot instead
([research.md](./specs/003-oss-release-readiness/research.md) R38).

## Where this file goes

Both distribution packages install it beside the README, so the account travels with the binary and
not only with the source (FR-066, [contracts/packaging.md](./specs/003-oss-release-readiness/contracts/packaging.md)):

| | Debian family | RPM family |
|---|---|---|
| This file | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` | `/usr/share/doc/hypr-swap/THIRD-PARTY.md` |
| The project's licence | `/usr/share/doc/hypr-swap/copyright` | `/usr/share/licenses/hypr-swap/LICENSE` |
