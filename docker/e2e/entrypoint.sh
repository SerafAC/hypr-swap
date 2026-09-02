#!/usr/bin/env bash
# Resolve a Wayland session, then run whatever this container was given (FR-088, FR-089).
#
# The harness (tests/e2e/harness.rs) starts a nested Hyprland as an ordinary client of *some*
# session. It never needed to change for automation: the only difference between a developer's
# machine and a runner is where that session comes from (research.md R29). There are two ways, and
# this script picks whichever it is given:
#
#   1. A session handed in — the contributor's own. Run the image with the host's XDG_RUNTIME_DIR,
#      WAYLAND_DISPLAY and one render node; this is quickstart.md scenario 5, and it is [verified]:
#
#          docker run --rm \
#            --device /dev/dri/renderD128 \
#            -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
#            -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
#            -v "$PWD:/work" -w /work \
#            hypr-swap-e2e
#
#   2. No session — automation. Given a virtual GPU and the privileges to open a seat, the image
#      starts its own parent Hyprland on the DRM backend and the suite nests inside that. This
#      needs the container started as root so seatd can run; the compositor still drops to the
#      unprivileged `hypr` user, because Hyprland refuses to run as root (research.md R30).
#
# Exit status 78 means **the environment failed**, not the tests: no session could be resolved.
# A broken runner is not a broken change, and the `e2e` job of contracts/ci.md says so distinctly.

set -u

# Distinct from any status `cargo test` produces, so a caller can tell a broken environment from a
# real failure without parsing output (contracts/ci.md, spec edge case).
readonly ENVIRONMENT_FAILURE=78

# How long the parent compositor gets to publish its socket. Generous: a cold container on a
# software renderer is slower than a desktop, and the cost of being wrong here is a false red.
readonly SESSION_TIMEOUT=60

readonly UNPRIVILEGED_USER=hypr

note() { printf 'e2e-env: %s\n' "$*" >&2; }

# Report an environment failure and stop. Every caller of this is a reason the *runner* is wrong.
environment_failure() {
    printf 'e2e-env: ENVIRONMENT FAILURE: %s\n' "$1" >&2
    printf 'e2e-env: this is a broken environment, not a broken change.\n' >&2
    exit "$ENVIRONMENT_FAILURE"
}

# --------------------------------------------------------------------------------------------
# Running as root: start the seat, then drop. Only the automation path arrives here.
# --------------------------------------------------------------------------------------------

start_seat_and_drop() {
    note "started as root — bringing up a seat, then dropping to $UNPRIVILEGED_USER"

    local uid
    uid="$(id -u "$UNPRIVILEGED_USER")" || environment_failure "no $UNPRIVILEGED_USER user in this image"

    # libseat talks to seatd over this socket; the compositor, running unprivileged, cannot open a
    # seat any other way inside a container.
    seatd -g seat &
    local waited=0
    while [ ! -S /run/seatd.sock ]; do
        if [ "$waited" -ge 10 ]; then
            environment_failure "seatd did not publish /run/seatd.sock within 10s"
        fi
        sleep 1
        waited=$((waited + 1))
    done
    chgrp seat /run/seatd.sock && chmod 0660 /run/seatd.sock

    # A runtime directory of the shape every Wayland client expects, if the caller supplied none.
    if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
        export XDG_RUNTIME_DIR="/run/user/$uid"
    fi
    mkdir -p "$XDG_RUNTIME_DIR"
    chown "$UNPRIVILEGED_USER:$UNPRIVILEGED_USER" "$XDG_RUNTIME_DIR"
    chmod 0700 "$XDG_RUNTIME_DIR"

    # `--init-groups` so the `seat` group membership is actually carried across.
    exec setpriv --reuid="$UNPRIVILEGED_USER" --regid="$UNPRIVILEGED_USER" --init-groups \
        env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" "$0" "$@"
}

# --------------------------------------------------------------------------------------------
# Resolving the session
# --------------------------------------------------------------------------------------------

# True when WAYLAND_DISPLAY names a socket that is actually there. An absolute value is honoured
# as-is; the ordinary relative one is resolved against XDG_RUNTIME_DIR, exactly as libwayland does.
session_is_up() {
    local display="${WAYLAND_DISPLAY:-}"
    [ -n "$display" ] || return 1
    case "$display" in
        /*) [ -S "$display" ] ;;
        *) [ -S "${XDG_RUNTIME_DIR:-}/$display" ] ;;
    esac
}

# Start a parent Hyprland on the DRM backend and wait for it to publish a socket.
start_parent_compositor() {
    note 'no session was handed in — starting a parent compositor on the DRM backend'

    [ -d /dev/dri ] || environment_failure \
        'no /dev/dri in this container: the DRM backend has no GPU to open (research.md R29 — automation must supply a virtual GPU)'

    # Hyprland picks its own `wayland-N`; it reports which in its log, but waiting for the socket
    # to appear is both simpler and the thing that actually matters.
    local socket
    Hyprland -c /etc/hypr-swap-e2e/parent.conf >"${XDG_RUNTIME_DIR}/parent-hyprland.log" 2>&1 &
    local compositor=$!

    local waited=0
    while [ "$waited" -lt "$SESSION_TIMEOUT" ]; do
        if ! kill -0 "$compositor" 2>/dev/null; then
            note '--- the parent compositor exited; its log follows ---'
            cat "${XDG_RUNTIME_DIR}/parent-hyprland.log" >&2
            environment_failure 'the parent compositor exited before it published a session'
        fi
        socket="$(ls "$XDG_RUNTIME_DIR" 2>/dev/null | grep '^wayland-[0-9]*$' | head -n1)"
        if [ -n "$socket" ]; then
            export WAYLAND_DISPLAY="$socket"
            note "parent compositor is up on $WAYLAND_DISPLAY"
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done

    note '--- the parent compositor never published a socket; its log follows ---'
    cat "${XDG_RUNTIME_DIR}/parent-hyprland.log" >&2
    environment_failure "no Wayland socket appeared within ${SESSION_TIMEOUT}s"
}

# --------------------------------------------------------------------------------------------

main() {
    if [ "$(id -u)" -eq 0 ]; then
        start_seat_and_drop "$@"
    fi

    [ -n "${XDG_RUNTIME_DIR:-}" ] || environment_failure \
        'XDG_RUNTIME_DIR is unset: pass the host'"'"'s runtime directory in, or start the container as root so one can be made'

    if session_is_up; then
        note "using the session handed in: WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
    else
        start_parent_compositor
    fi

    # The assertion the `e2e` job depends on. Everything above this line is environment; everything
    # below is the change under test.
    session_is_up || environment_failure 'no Wayland session could be resolved'

    note "running: $*"
    exec "$@"
}

main "$@"
