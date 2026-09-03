# The E2E test image

A pinned stack — Hyprland, `foot`, mesa, cairo/pango and a Rust toolchain — to run the end-to-end
suite against, **on a machine whose own stack is different**.

## What it is for

Compatibility testing, locally. The suite already runs natively on any machine with a Hyprland
session; you do not need this image to run it, and most of the time you should not. Reach for it
when the question is about *versions* rather than about your change:

- **Does this still work against the compositor the project supports?** Your Hyprland moves with
  your distribution. This one is pinned, so "the compositor updated under me" is distinguishable
  from "my change broke it".
- **Does it build and pass on the declared minimum toolchain?** The image carries the `rust-version`
  from `Cargo.toml`, not whatever `rustup` gave you.
- **Does it work on a stack I do not run?** Arch is the only family shipping a current Hyprland, so
  a contributor on Debian or Fedora can exercise the tier without building a compositor from source
  or changing what is installed on their machine.

Everything is pinned: the base image by digest, the toolchain by version, and the compositor by
whatever the pinned base resolves to.

## What it is not for

**It is not a CI environment, and there is no CI job that uses it.** It cannot create a Wayland
session — it runs the suite against one you already have.

That is a measured limit, not an omission. The harness starts a nested Hyprland as an ordinary
Wayland client, so it needs a parent session that can hand it a dmabuf allocator, and that needs a
real GPU underneath. Both ways of faking one were tried and neither works
([research.md](../../specs/003-oss-release-readiness/research.md) R29):

| Attempt | Result |
|---|---|
| A plain container | Hyprland will not start at all — no seat, no allocator |
| `vkms` on a CI runner | Parent starts; the nested compositor never gets a monitor. `vkms` is display-only, so it has no render node to allocate from |
| A QEMU VM with `virtio-gpu` | A render node *does* appear and the parent starts, but without virgl there is no usable GPU driver, so mesa falls back to KMS dumb buffers on the primary node and the nested compositor is refused them |

The end-to-end tier is therefore verified on a developer's machine, which is recorded as a
deviation against FR-088 in the [spec](../../specs/003-oss-release-readiness/spec.md).

## Running it

```bash
docker build -t hypr-swap-e2e docker/e2e

docker run --rm \
  --device /dev/dri/renderD128 \
  -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
  -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
  -v "$PWD:/work" -w /work \
  hypr-swap-e2e
```

Anything you pass after the image name replaces the suite, so `… hypr-swap-e2e true` is a "can this
image see my session" probe and `… hypr-swap-e2e cargo test --test e2e_switcher` runs one binary.

**If your `/dev/dri` nodes are not world-accessible**, add `--user root`. The entry point then joins
the groups that actually own them — by number, because a host's group ids do not match an image's —
and drops to an unprivileged user before starting anything, since Hyprland refuses to run as root.

Build artefacts stay inside the container (`CARGO_TARGET_DIR=/tmp/...`), so a run here does not
force your next native `cargo build` to start over with a different toolchain's objects.

## Keeping it current

`RUST_TOOLCHAIN` tracks `rust-version` in `Cargo.toml` and the `FROM` line is pinned by digest.
Both are updated by hand, deliberately: the point of this image is that it does not move unless
someone decides it should.
