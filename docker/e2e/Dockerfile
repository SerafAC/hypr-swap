# The one E2E environment (FR-089): the image automation runs and the image a contributor runs are
# the same image, so an automation failure reproduces locally (SC-035, research.md R29, R30).
#
# Arch, because it is the only family that ships a current Hyprland, which is what this tier needs.
# The distribution the *packages* target is a separate question and deliberately a different image
# (research.md R33).
#
# Build:  docker build -t hypr-swap-e2e docker/e2e
# Run:    see quickstart.md scenario 5, or docker/e2e/entrypoint.sh's header.
FROM archlinux@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a

# Tracks `rust-version` in Cargo.toml — the toolchain the project declares as its minimum, which is
# what the `msrv` job of contracts/ci.md guards against drift (research.md R30).
ARG RUST_TOOLCHAIN=1.96.0

# The unprivileged user the compositor runs as. uid 1000 by default so that a bind-mounted
# checkout from a single-user desktop is readable without any further ceremony.
ARG UID=1000

# `hyprland` brings `Hyprland` and `hyprctl`; `foot` is the harness's documented substitute for an
# arbitrary application window; `seatd` supplies the seat the DRM backend needs when no session is
# handed in; `mesa` the GBM/EGL stack; `cairo`/`pango` are the program's own link dependencies —
# Arch ships headers in the same package, so there is no separate development package to name.
# `ttf-dejavu` exists because pango will not lay text out with no font installed at all, and the
# overlay is text. `libcap` is for the `setcap -r` below.
RUN pacman -Syu --noconfirm --needed \
        hyprland foot seatd mesa \
        cairo pango libxkbcommon \
        gcc pkgconf git rustup libcap ttf-dejavu \
    && rm -rf /var/cache/pacman/pkg/*

# Arch's compositor binaries carry `cap_sys_nice=ep`, and executing a file with capabilities fails
# outright with `Operation not permitted` under Docker's default bounding set — [verified],
# research.md R30. Stripping them costs the compositor a scheduling nicety it already warns about.
# `sway` is listed because R29 measured it as a candidate parent; it is not installed here, and the
# loop tolerates that rather than pretending otherwise.
RUN for binary in /usr/bin/Hyprland /usr/bin/sway; do \
        if [ -e "$binary" ]; then setcap -r "$binary"; fi; \
    done

# Hyprland refuses to run as root (`Hyprland was launched with superuser privileges…`), and
# containers run as root by default — [verified], research.md R30. The `--i-am-really-stupid`
# escape hatch is not used: a `useradd` line is cheaper than running a compositor as root beside a
# `cargo build`.
#
# `seat` is created by the seatd package and is what lets this user open a seat. `video` and
# `render` are what let it *open a DRM node directly* — libseat hands the compositor an fd for the
# card, but Hyprland opens a render node itself for GBM, and without the group that open() fails
# with the assertion `Couldn't open a gbm fd` [verified, CI 2026-09-02]. `input` is for libinput.
# Each group is created first if the base image has not already, so this does not depend on which
# packages happened to bring one in.
RUN for group in seat video render input; do \
        getent group "$group" > /dev/null || groupadd --system "$group"; \
    done \
    && useradd --create-home --uid "$UID" --groups seat,video,render,input hypr

# One toolchain, installed once, readable by every user — so the image works whether it is started
# as `hypr` (the default) or as root (the automation path, which needs to start seatd before
# dropping). CARGO_HOME is writable because the crate registry is fetched at run time: the
# Dockerfile has no Cargo.toml in its context to pre-fetch from, by design — the context is
# `docker/e2e` so that the published image does not rebuild on every source change.
ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo
ENV PATH=/opt/cargo/bin:$PATH
RUN rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal \
    && rustup default "$RUST_TOOLCHAIN" \
    && chmod -R a+rX /opt/rustup \
    && mkdir -p /opt/cargo \
    && chown -R hypr:hypr /opt/cargo

# Build artefacts stay inside the container. Writing into the mounted checkout's `target/` would
# mean the host's next `cargo build` rebuilds everything anyway — the toolchains differ — so this
# trades a cold build in the container for not disturbing the developer's own tree.
ENV CARGO_TARGET_DIR=/tmp/hypr-swap-target

# The mounted checkout belongs to whichever uid the host used. git refuses to read a repository it
# does not own, and `build.rs` asks it for `git describe` (FR-104); without this the build silently
# reports the package version alone rather than the source it came from.
RUN git config --system --add safe.directory '*'

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY parent.conf /etc/hypr-swap-e2e/parent.conf
RUN chmod 0755 /usr/local/bin/entrypoint.sh

USER hypr
WORKDIR /work

# The entry point resolves a Wayland session — one handed in, or one it starts itself — and then
# runs whatever it is given. The default is the E2E tier; `docker run … <image> true` is therefore
# a pure "did the environment come up" probe, which is what the `e2e` job asserts with before it
# invokes the suite (contracts/ci.md).
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
# `--no-fail-fast` because cargo otherwise stops at the first failing test *target*, and this tier
# has eleven: one failing binary would hide whatever the other ten would have said, which is the
# opposite of what a report from automation is for.
CMD ["cargo", "test", "--no-fail-fast", "--test", "e2e_*"]
