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

    # `setpriv` changes the credentials and nothing else, so `HOME` would still be root's — which
    # is how the compositor came to report `failed to mkdir() crash report directory: Permission
    # denied` [verified, CI 2026-09-02]. Every path derived from `HOME` is wrong until it is set:
    # the crash report, `XDG_CACHE_HOME`, and the config directory the harness overrides per test.
    local home
    home="$(getent passwd "$UNPRIVILEGED_USER" | cut -d: -f6)"
    [ -n "$home" ] || environment_failure "$UNPRIVILEGED_USER has no home directory"

    # `--init-groups` so the `seat` group membership is actually carried across.
    exec setpriv --reuid="$UNPRIVILEGED_USER" --regid="$UNPRIVILEGED_USER" --init-groups \
        env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
            HOME="$home" USER="$UNPRIVILEGED_USER" LOGNAME="$UNPRIVILEGED_USER" \
            "$0" "$@"
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

# Everything the compositor wrote, wherever it wrote it. Hyprland turns its own stdout logging off
# a few lines in and continues into `$XDG_RUNTIME_DIR/hypr/<signature>/hyprland.log`, so the
# interesting half of a failure is in a file rather than on the pipe — which is how the first CI
# run reported a crash with no reason attached.
dump_compositor_logs() {
    note '--- the parent compositor: stdout ---'
    cat "${XDG_RUNTIME_DIR}/parent-hyprland.log" >&2 2>/dev/null
    local logfile
    for logfile in "${XDG_RUNTIME_DIR}"/hypr/*/hyprland.log; do
        [ -f "$logfile" ] || continue
        note "--- the parent compositor: $logfile ---"
        cat "$logfile" >&2
    done
}

# The kernel driver behind a `card*` node.
#
# Two ways, because one is not enough: `device/uevent` carries `DRIVER=` for an ordinary bus, but
# `vkms` registers on the **faux** bus (`/sys/devices/faux/vkms/drm/card0`) where that lookup finds
# nothing — which is why the first attempt at this reported "no vkms node found" on a runner that
# had one [verified, CI 2026-09-02]. The sysfs path names the driver in that case.
drm_driver() {
    local card="$1" driver path
    driver="$(sed -n 's/^DRIVER=//p' "/sys/class/drm/$card/device/uevent" 2>/dev/null)"
    # The faux bus answers `faux_driver` for **every** device on it, which names nothing — the
    # first attempt at this treated that as an answer and so still missed vkms [verified, CI
    # 2026-09-02]. An empty answer and that placeholder are the same thing: ask the path instead,
    # which carries the real name (`/sys/devices/faux/vkms/drm/card0`).
    if [ -z "$driver" ] || [ "$driver" = faux_driver ]; then
        path="$(readlink -f "/sys/class/drm/$card" 2>/dev/null)"
        case "$path" in
            */faux/*) driver="$(printf '%s' "$path" | sed -n 's|.*/faux/\([^/]*\)/.*|\1|p')" ;;
        esac
    fi
    printf '%s' "${driver:-unknown}"
}

# The order to hand the DRM backend, when there is more than one device.
#
# A runner has two: an Azure image already carries a Hyper-V framebuffer and `vkms` arrives beside
# it. Left to scan, aquamarine makes the framebuffer primary — and `hyperv_drm` has no render node
# and one plane format, so Hyprland's `openRenderNode` falls back to the primary node and the
# renderer never comes up. vkms is the purpose-built virtual KMS device and reports 22 plane
# formats, so it goes first; the others stay in the list behind it rather than being hidden, so
# nothing that works today stops working.
drm_devices() {
    local node card driver first="" rest=""
    for node in /dev/dri/card*; do
        [ -e "$node" ] || continue
        card="$(basename "$node")"
        driver="$(drm_driver "$card")"
        note "  $node -> $driver"
        if [ "$driver" = vkms ] && [ -z "$first" ]; then
            first="$node"
        else
            rest="${rest:+$rest:}$node"
        fi
    done
    [ -n "$first" ] || return 1
    printf '%s' "${first}${rest:+:$rest}"
}

# Start a parent Hyprland on the DRM backend and wait for it to publish a socket.
start_parent_compositor() {
    note 'no session was handed in — starting a parent compositor on the DRM backend'

    [ -d /dev/dri ] || environment_failure \
        'no /dev/dri in this container: the DRM backend has no GPU to open (research.md R29 — automation must supply a virtual GPU)'

    # What the backend has to work with, in the log before it is asked to work with it. A virtual
    # GPU is display-only, so a `card*` with no `renderD*` is expected; if the compositor then
    # fails on an allocator, this is the line that says why.
    note "device nodes: $(ls /dev/dri 2>/dev/null | tr '\n' ' ')"

    # There is no hardware renderer behind a virtual GPU, so mesa is told to stop looking for one
    # rather than probing and falling back. Set only here: a contributor's own session has a real
    # GPU and must keep using it.
    export LIBGL_ALWAYS_SOFTWARE=1

    note 'DRM devices:'
    local devices
    if devices="$(drm_devices)"; then
        export AQ_DRM_DEVICES="$devices"
        note "ordering the DRM backend as $devices (vkms first)"
    else
        note 'no vkms node found; letting the DRM backend scan for itself'
    fi

    # Hyprland picks its own `wayland-N`; it reports which in its log, but waiting for the socket
    # to appear is both simpler and the thing that actually matters.
    local socket
    Hyprland -c /etc/hypr-swap-e2e/parent.conf >"${XDG_RUNTIME_DIR}/parent-hyprland.log" 2>&1 &
    local compositor=$!

    local waited=0
    while [ "$waited" -lt "$SESSION_TIMEOUT" ]; do
        if ! kill -0 "$compositor" 2>/dev/null; then
            dump_compositor_logs
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

    dump_compositor_logs
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
