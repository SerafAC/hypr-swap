#!/usr/bin/env bash
# Run the E2E tier against the Wayland session you give this container (FR-089).
#
# The harness (tests/e2e/harness.rs) starts a nested Hyprland as an ordinary client of *some*
# session. This image supplies the compositor, the toolchain and the test dependencies at pinned
# versions; it cannot supply the session, and no longer tries to — see docker/e2e/README.md, and
# research.md R29 for the measurements that settled it.
#
#     docker run --rm \
#       --device /dev/dri/renderD128 \
#       -e WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
#       -v "$XDG_RUNTIME_DIR:/run/host" -e XDG_RUNTIME_DIR=/run/host \
#       -v "$PWD:/work" -w /work \
#       hypr-swap-e2e
#
# Add `--user root` if your `/dev/dri` nodes are not world-accessible: started as root this joins
# the groups that own them and then drops to the unprivileged user anyway, because Hyprland refuses
# to run as root.

set -u

readonly UNPRIVILEGED_USER=hypr

note() { printf 'e2e-env: %s\n' "$*" >&2; }

fail() {
    printf 'e2e-env: %s\n' "$1" >&2
    exit 1
}

# --------------------------------------------------------------------------------------------
# Started as root: take what only root can take, then drop.
# --------------------------------------------------------------------------------------------

drop_privileges() {
    note "started as root — joining the device groups, then dropping to $UNPRIVILEGED_USER"

    # Device nodes come from the host's devtmpfs and carry the **host's** numeric group ids, which
    # mean nothing in this image's /etc/group — one distribution's `video` is 44, Arch's is 983.
    # Join whatever group actually owns each node, by number.
    #
    # The nodes themselves are deliberately not touched: with `--privileged` this is the host's own
    # /dev, and a `chgrp` here would change permissions on the machine running the container.
    local node gid
    for node in /dev/dri/*; do
        [ -c "$node" ] || continue
        gid="$(stat -c %g "$node")"
        getent group "$gid" > /dev/null || groupadd --gid "$gid" "hostdrm$gid"
        usermod --append --groups "$gid" "$UNPRIVILEGED_USER"
    done

    # `setpriv` changes credentials and nothing else, so `HOME` would stay root's and everything
    # derived from it would be wrong — the compositor's cache, and the config directory the harness
    # overrides per test.
    local home
    home="$(getent passwd "$UNPRIVILEGED_USER" | cut -d: -f6)"
    [ -n "$home" ] || fail "$UNPRIVILEGED_USER has no home directory"

    exec setpriv --reuid="$UNPRIVILEGED_USER" --regid="$UNPRIVILEGED_USER" --init-groups \
        env HOME="$home" USER="$UNPRIVILEGED_USER" LOGNAME="$UNPRIVILEGED_USER" \
            "$0" "$@"
}

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

main() {
    if [ "$(id -u)" -eq 0 ]; then
        drop_privileges "$@"
    fi

    session_is_up || fail "no Wayland session. Pass the host's XDG_RUNTIME_DIR and WAYLAND_DISPLAY in, as the header of this script shows. This image runs the suite against a session you already have and cannot create one: a container has no GPU to start a compositor on, which is what research.md R29 measured."

    note "using the session handed in: WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
    note "running: $*"
    exec "$@"
}

main "$@"
