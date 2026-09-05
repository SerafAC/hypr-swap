---
title: Testing
description: The three tiers, which of them needs a live compositor, the nested-Hyprland harness, and how to run the compositor-dependent tier without a Wayland session.
---

Three tiers, and the practical difference between them is what each one needs from the machine it
runs on.

| Tier | Command | Needs |
|---|---|---|
| Unit | `cargo test --lib` | Nothing. No compositor, no display |
| End-to-end | `cargo test --test 'e2e_*'` | A live Wayland session with Hyprland ≥ 0.55, and `foot` |
| Document checks | `./scripts/checks.sh` | Nothing |

Lint and format sit alongside them and gate a merge the same way:

```bash
cargo clippy --all-targets -- -D warnings   # pedantic is enabled at warn level
cargo fmt --check
cargo deny check licenses                  # the dependency graph's licences (FR-064)
```

## The unit tier

Every pure module's tests live in that module's own file, next to the code they cover. This is the
tier that runs anywhere, and it is where the great majority of the project's behaviour is actually
verified — the [architecture](./architecture.md) seam exists precisely so that decisions land on the
side that can be tested this way.

```bash
cargo test --lib                # everything
cargo test --lib ordering       # one module
cargo test --lib theme          # style values, colour parsing, ranges, precedence
cargo test --lib icons          # the matching ladder, set lookup, cache, decoding
```

Two of these tests read published documents rather than only code, and they are the reason
documentation cannot drift: `theme.rs` walks the style catalogue against its own `const` tables, so
a setting added without being documented fails the build, and `ui/shortcuts.rs` `include_str!`s the
bind page and asserts it quotes every line the program generates. Both fail as ordinary unit-test
failures, in front of the person who made the change.

## The end-to-end tier

This is the one that needs a compositor. `tests/e2e/harness.rs` starts a **nested Hyprland as an
ordinary Wayland client of your own session**, adds headless outputs to it, spawns `foot` windows
into them, and injects real key events through `virtual-keyboard-unstable-v1`. Every assertion then
goes through the nested instance's own IPC socket.

Nothing is mocked. The daemon under test talks to a real compositor over the real protocols, which
is what makes this tier worth the trouble of running it.

```bash
cargo test --test 'e2e_*'                                  # the whole tier
cargo test --test e2e_harness -- nested_instance_starts    # one test
```

**Do not fight the harness with `--test-threads`.** Only one nested compositor can exist at a time,
so the tests serialise on an internal lock. Raising the thread count does not make them faster and
does make failures confusing.

## The E2E tier needs a Wayland session, and nothing can fake one

There is no way to run this tier without one — not in a plain container, and not in automation.
The harness nests a compositor as an ordinary Wayland client, so it needs a parent session that can
hand it a dmabuf allocator, and that needs a real GPU underneath.

That is measured rather than assumed. A plain container cannot start Hyprland at all: the DRM
backend fails with `libseat: failed to open a seat` and the Wayland backend with
`no allocator available`. Two ways of supplying automation with a synthetic GPU were then built and
measured, and both failed at the same point — the *nested* compositor:

| Attempt | How far it got |
|---|---|
| `vkms` on a CI runner | The parent compositor starts and publishes a session. The nested one never reports a monitor: `vkms` is display-only, so it has no render node to allocate from |
| A QEMU virtual machine with `virtio-gpu` | A render node appears and the parent starts. The nested compositor is then refused KMS dumb buffers — without virgl there is no driver behind the render node, and the fallback path needs a node the parent holds DRM master on |

**So this project has no end-to-end CI job.** The requirement asking for one is recorded as unmet
rather than quietly dropped, and the tier is verified on a developer's machine. The measurements,
and the routes that were rejected along the way, are in
[`specs/003-oss-release-readiness/research.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/003-oss-release-readiness/research.md)
R29.

## Running the tier against pinned versions

`docker/e2e/` is a **local compatibility-testing tool**, not a CI environment. It holds the
compositor, the toolchain and the test dependencies at fixed versions, so you can ask "does this
still work against the compositor the project supports?" without changing what is installed on your
machine — and tell "my compositor updated under me" apart from "my change broke it". It is also how
to exercise the tier from a distribution that does not ship a current Hyprland, since Arch is the
only family that does.

It still needs a session; it runs the suite against yours.

```bash
docker build -t hypr-swap-e2e docker/e2e

docker run --rm \
  --device /dev/dri/renderD128 \
  -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
  -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
  -v "$PWD:/work" -w /work \
  hypr-swap-e2e
```

Two things in the image are not obvious and are there deliberately: it creates and drops to an
unprivileged user, because **Hyprland refuses to run as root** and containers run as root by
default; and it strips the `cap_sys_nice` file capabilities from the compositor binaries, because
executing a file that carries capabilities fails with `Operation not permitted` under a container's
default bounding set. If your `/dev/dri` nodes are not world-accessible, start it with
`--user root` — it then joins the groups that own them, by number, and drops privileges before
running anything.

[`docker/e2e/README.md`](https://github.com/SerafAC/hypr-swap/blob/master/docker/e2e/README.md) is
the image's own documentation.

## The document checks

```bash
./scripts/checks.sh
```

Shell rather than Rust, because their subject is *files* — that the README does not carry
development instructions, that the required documentation pages exist, that every page the site
navigation names is really there. A check whose subject is one of the **program's own values** is a
unit test instead, where a contributor meets it next to the code.

Every failure names what is wrong and what to do about it, and the script reports everything wrong
in one run rather than stopping at the first thing. It is the same script the `checks` job runs, so
running it locally is running the gate.

## The documentation site

```bash
pnpm install --frozen-lockfile
pnpm build          # the site, in site/
pnpm dev            # a live server at http://localhost:3000 while you write
pnpm validate       # every internal link and reference
```

**pnpm is this project's package manager — not npm or Yarn.** `package.json` pins the version in
`packageManager`, `pnpm-lock.yaml` is the committed lockfile, and `pnpm-workspace.yaml` holds the
dependency build approvals that pnpm 11 requires before it will run a `postinstall` at all. Another
manager leaves a second lockfile behind and installs a tree nothing here describes. `corepack enable`
gets you the pinned version if you do not have it.

This needs Node ≥ 22 and pnpm ≥ 11, and nothing else in the repository does — a contributor who
touches only Rust never installs either. The build is gating: a broken page fails the change that
broke it.

The pages are the plain Markdown under `docs/`, and that is the whole of what you edit. There is no
page framework to learn: navigation order is the list in `docmd.config.mjs`, everything else the
site needs it works out from the tree.

One directive is not plain Markdown, and it is the one that matters:

```text
::include[../../specs/002-overlay-visuals/contracts/config.md]
::include[../../specs/002-overlay-visuals/contracts/style-values.md#colours]
```

`::include[]` pulls a document, or one heading's section of it, into the page at build time — which
is how a site page and a `specs/` contract can say the same thing without being able to diverge
(FR-084). It is implemented in `scripts/docmd-include.mjs`, in about a hundred lines, and it fails
the build naming both the page and the target when a path or a heading slug is wrong. Included
headings are demoted to sit under the page heading above them, and a relative link inside an
included contract is re-pointed at the repository, since the document it names lives in `specs/`
rather than on this site.

## What verifies what

Every requirement in the project has a named verification tier, and the table is published in full
on [verification coverage](./verification.md).
