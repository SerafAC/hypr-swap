#!/usr/bin/env bash
# The document checks of contracts/ci.md, runnable locally exactly as the `checks` job runs them.
#
# Their subject is files rather than values, which is why they are shell rather than Rust: a check
# that compares a document against the *program's* own values (the settings catalogue, the
# advisory expiry) is a unit test instead, where a contributor meets it next to the code.
#
# Every failure names what to do about it. The whole script is reproduced with:
#
#     ./scripts/checks.sh
#
# Exit status is 0 if every check passed and 1 otherwise; each failure is printed as it is found,
# so one run reports everything wrong rather than only the first thing.

set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

failures=0

# Report one failed assertion: what is wrong, then how to put it right.
fail() {
    printf 'FAIL %s\n     %s\n' "$1" "$2" >&2
    failures=$((failures + 1))
}

pass() {
    printf 'ok   %s\n' "$1"
}

# ---------------------------------------------------------------------------
# docs-map — the document map of contracts/documentation.md holds (FR-068–FR-070)
# ---------------------------------------------------------------------------

docs_map() {
    local readme=README.md

    [ -f "$readme" ] || {
        fail "docs-map: $readme is missing" "The README is the end user's document (FR-067)."
        return
    }

    # FR-068: the README is the end user's document; development instructions belong to
    # DEVELOPMENT.md and CONTRIBUTING.md, and the README links to them rather than restating them.
    local forbidden
    for forbidden in 'cargo test' 'cargo clippy'; do
        if grep -qF -- "$forbidden" "$readme"; then
            fail "docs-map: $readme carries \`$forbidden\` (FR-068)" \
                "Move it to DEVELOPMENT.md and link there instead."
        else
            pass "docs-map: $readme carries no \`$forbidden\` (FR-068)"
        fi
    done

    if grep -qiE '^#+[[:space:]].*architecture' "$readme"; then
        fail "docs-map: $readme has an architecture heading (FR-068)" \
            "Architecture belongs to DEVELOPMENT.md and docs/dev/architecture.md."
    else
        pass "docs-map: $readme has no architecture heading (FR-068)"
    fi

    # FR-069: the requirements section states the ranges the program itself defines, so the
    # README cannot drift from `SUPPORTED_HYPRLAND` and `rust-version` (Principle III).
    local minimum range
    minimum=$(sed -n 's/.*SupportedRange[[:space:]]*{[[:space:]]*minimum:[[:space:]]*(\([0-9]\+\),[[:space:]]*\([0-9]\+\)).*/\1.\2/p' src/lib.rs | head -1)
    if [ -z "$minimum" ]; then
        fail "docs-map: SUPPORTED_HYPRLAND is not readable from src/lib.rs" \
            "The check derives the documented range from the constant; keep its literal form."
    else
        range=">= $minimum"
        if grep -qF -- "$range" "$readme"; then
            pass "docs-map: $readme states the compositor range \`$range\` (FR-069)"
        else
            fail "docs-map: $readme does not state the compositor range \`$range\` (FR-069)" \
                "SUPPORTED_HYPRLAND in src/lib.rs says \`$range\`; the README must say the same."
        fi
    fi

    local toolchain
    toolchain=$(sed -n 's/^rust-version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
    if [ -z "$toolchain" ]; then
        fail "docs-map: rust-version is not readable from Cargo.toml" \
            "The check derives the documented toolchain from the manifest; keep the key."
    elif grep -qF -- "$toolchain" "$readme"; then
        pass "docs-map: $readme states the toolchain \`$toolchain\` (FR-069)"
    else
        fail "docs-map: $readme does not state the toolchain \`$toolchain\` (FR-069)" \
            "Cargo.toml's rust-version is \`$toolchain\`; the README must say the same."
    fi

    # FR-069: each optional dependency is named with what degrades without it.
    local optional
    for optional in 'icon set' 'notify-send'; do
        if grep -qF -- "$optional" "$readme"; then
            pass "docs-map: $readme names the optional dependency \`$optional\` (FR-069)"
        else
            fail "docs-map: $readme does not name the optional dependency \`$optional\` (FR-069)" \
                "State it and what degrades without it."
        fi
    done

    # FR-070: a prospective user sees the overlay, in both presentations, before installing it.
    local shot
    for shot in docs/assets/overlay-list.png docs/assets/overlay-grid.png; do
        if [ ! -f "$shot" ]; then
            fail "docs-map: $shot is missing (FR-070)" \
                "Recapture the overlay screenshots; see specs/003-oss-release-readiness/tasks.md T013."
        elif grep -qF -- "$shot" "$readme"; then
            pass "docs-map: $readme shows $shot (FR-070)"
        else
            fail "docs-map: $readme does not reference $shot (FR-070)" \
                "Embed both presentations in the README."
        fi
    done

    # FR-071: scope and the privacy statement, in the reader's own terms rather than by heading.
    local statement
    for statement in 'no network access' 'no telemetry'; do
        if grep -qiF -- "$statement" "$readme"; then
            pass "docs-map: $readme states \`$statement\` (FR-071)"
        else
            fail "docs-map: $readme does not state \`$statement\` (FR-071)" \
                "The privacy statement is required, in plain words."
        fi
    done
}

docs_map

if [ "$failures" -ne 0 ]; then
    printf '\n%d check(s) failed. Reproduce with: ./scripts/checks.sh\n' "$failures" >&2
    exit 1
fi

printf '\nAll checks passed.\n'
