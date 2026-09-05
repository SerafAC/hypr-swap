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
# licence-files — the project is redistributable and every file is attributable
# (FR-062, FR-063, FR-065, SC-041)
# ---------------------------------------------------------------------------

licence_files() {
    local licence=LICENSE account=THIRD-PARTY.md

    # FR-062: the full text, at the conventional top-level location.
    if [ ! -f "$licence" ]; then
        fail "licence-files: $licence is missing (FR-062)" \
            "Without it the project is not redistributable; nobody may use what it does not licence."
        return
    fi

    # The holder and the year, stated rather than implied by the file's name. Both halves are
    # required: a licence naming no holder grants nothing that can be relied on.
    local copyright
    copyright=$(sed -n 's/^Copyright ([cC]) \([0-9]\{4\}\)\(-[0-9]\{4\}\)\{0,1\} \(..*\)$/\1 \3/p' "$licence" | head -1)
    if [ -n "$copyright" ]; then
        pass "licence-files: $licence names a holder and a year ($copyright) (FR-062)"
    else
        fail "licence-files: $licence has no \`Copyright (c) <year> <holder>\` line (FR-062)" \
            "State the year and the copyright holder; the licence has to say who is granting it."
    fi

    # FR-062: and it is the licence the manifest declares. Two statements of one fact is one place
    # too many, so the check is that they agree rather than that either is right.
    local declared
    declared=$(sed -n 's/^license[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
    if [ -z "$declared" ]; then
        fail "licence-files: Cargo.toml declares no \`license\` (FR-062, FR-065)" \
            "A packager reads the manifest first; it must name the licence LICENSE spells out."
    elif head -1 "$licence" | grep -qF -- "$declared"; then
        pass "licence-files: $licence is the \`$declared\` licence Cargo.toml declares (FR-062)"
    else
        fail "licence-files: $licence does not open with Cargo.toml's \`$declared\` (FR-062)" \
            "The manifest and the licence text must be the same licence; change both or neither."
    fi

    # FR-065: what a packager and a source index expect to find in the manifest.
    local key missing_keys=""
    for key in description license repository documentation keywords; do
        grep -qE "^$key[[:space:]]*=[[:space:]]*[^[:space:]]" Cargo.toml || missing_keys="$missing_keys $key"
    done
    if [ -n "$missing_keys" ]; then
        fail "licence-files: Cargo.toml carries no:$missing_keys (FR-065)" \
            "See specs/003-oss-release-readiness/contracts/packaging.md → Metadata for the full set."
    else
        pass "licence-files: Cargo.toml carries the source-index metadata (FR-065)"
    fi

    # FR-063, SC-041: zero unattributed files. Everything shipping inside the tree that could have
    # come from elsewhere is listed in the account — including what did not, because "this one is
    # ours" is an answer and silence is not.
    if [ ! -f "$account" ]; then
        fail "licence-files: $account is missing (FR-063)" \
            "Every third-party component shipping inside the tree is accounted for there."
        return
    fi

    local path unlisted="" unheaded=""
    while IFS= read -r path; do
        [ -z "$path" ] && continue
        grep -qF -- "$path" "$account" || unlisted="$unlisted $path"
        # And the other half of R45's answer: a file carries its own provenance too, so that a
        # copy taken out of this tree does not lose it.
        grep -qF -- "$account" "$path" || unheaded="$unheaded $path"
    done <<INNER_EOF
$(find protocols assets -type f 2>/dev/null | sort)
INNER_EOF

    if [ -n "$unlisted" ]; then
        fail "licence-files: $account does not account for:$unlisted (FR-063, SC-041)" \
            "Give each one its origin, its version or revision, and its licence."
    else
        pass "licence-files: $account accounts for every path under protocols/ and assets/ (FR-063)"
    fi

    if [ -n "$unheaded" ]; then
        fail "licence-files: no provenance header pointing at $account in:$unheaded (FR-063)" \
            "A file read on its own must still say where it came from; see research.md R45."
    else
        pass "licence-files: every such file carries its own provenance header (FR-063)"
    fi
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

# ---------------------------------------------------------------------------
# docs-map — the required pages exist and each answers what it is meant to
# (FR-074, FR-077, FR-078a, FR-081, FR-082, FR-084, FR-084a)
# ---------------------------------------------------------------------------

# The site's required page set, from contracts/documentation.md's "The site's shape".
readonly USER_PAGES="install binds configuration styling icons troubleshooting"
readonly DEV_PAGES="architecture workflow testing verification releasing"

docs_pages() {
    local page

    for page in $USER_PAGES; do
        if [ -f "docs/user/$page.md" ]; then
            pass "docs-map: docs/user/$page.md exists (FR-081)"
        else
            fail "docs-map: docs/user/$page.md is missing (FR-081)" \
                "The end-user section's page set is fixed by specs/003-oss-release-readiness/contracts/documentation.md."
        fi
    done

    for page in $DEV_PAGES; do
        if [ -f "docs/dev/$page.md" ]; then
            pass "docs-map: docs/dev/$page.md exists (FR-082)"
        else
            fail "docs-map: docs/dev/$page.md is missing (FR-082)" \
                "The developer section's page set is fixed by specs/003-oss-release-readiness/contracts/documentation.md."
        fi
    done

    # FR-077: two navigable sections, each titled, and every page reachable from the navigation.
    # The navigation is the list in docmd.config.mjs; a page that exists but is not in it is a
    # page no reader arrives at, which the build itself has no reason to complain about.
    local config=docmd.config.mjs section title

    [ -f "$config" ] || {
        fail "docs-map: $config is missing (FR-076)" \
            "The site's navigation, its published URL and its include settings all live there."
        return
    }

    for section in user:'User guide' dev:'Developer guide'; do
        title=${section#*:}
        section=${section%%:*}
        if grep -qF -- "'$title'" "$config"; then
            pass "docs-map: the navigation titles the $section section \"$title\" (FR-077)"
        else
            fail "docs-map: $config does not title the $section section \"$title\" (FR-077)" \
                "FR-077's two sections are \"User guide\" and \"Developer guide\"."
        fi
    done

    local missing="" route
    for page in $USER_PAGES; do
        route="/user/$page/"
        grep -qF -- "'$route'" "$config" || missing="$missing $route"
    done
    for page in $DEV_PAGES; do
        route="/dev/$page/"
        grep -qF -- "'$route'" "$config" || missing="$missing $route"
    done
    if [ -n "$missing" ]; then
        fail "docs-map: the navigation in $config does not reach:$missing (FR-077)" \
            "A page absent from the navigation is a page no reader arrives at. Add it to the section's children."
    else
        pass "docs-map: the navigation reaches every page of both sections (FR-077)"
    fi
}

# FR-074: DEVELOPMENT.md names every top-level directory and every module under src/.
docs_development_tree() {
    local development=DEVELOPMENT.md

    [ -f "$development" ] || {
        fail "docs-map: $development is missing (FR-072)" \
            "The developer's document is required."
        return
    }

    local directory missing_dirs=""
    for directory in */; do
        directory=${directory%/}
        # target/ is Cargo's output and is not in the repository.
        [ "$directory" = "target" ] && continue
        grep -qF -- "$directory/" "$development" || missing_dirs="$missing_dirs $directory"
    done
    if [ -n "$missing_dirs" ]; then
        fail "docs-map: $development does not name the top-level director(ies):$missing_dirs (FR-074)" \
            "Name each one and what it holds, in the tree section."
    else
        pass "docs-map: $development names every top-level directory (FR-074)"
    fi

    local module missing_mods=""
    while IFS= read -r module; do
        # Named as `ui/layout.rs` would be, i.e. relative to src/.
        grep -qF -- "${module#src/}" "$development" || missing_mods="$missing_mods ${module#src/}"
    done <<EOF
$(find src -name '*.rs' | sort)
EOF
    if [ -n "$missing_mods" ]; then
        fail "docs-map: $development does not name the module(s):$missing_mods (FR-074)" \
            "Every module under src/ is named with its responsibility, on one side of the seam or the other."
    else
        pass "docs-map: $development names every module under src/ (FR-074)"
    fi
}

# FR-078a: the front page states which release it documents, and cannot drift from the version the
# tree actually carries. Below 1.0.0 nothing has been released (FR-101 makes the first release
# exactly 1.0.0), so the only true statement is that the site documents the default branch.
docs_front_page_version() {
    local front=docs/index.md

    [ -f "$front" ] || {
        fail "docs-map: $front is missing (FR-078a)" \
            "The site's front page is also the first thing a reader opening docs/ sees."
        return
    }

    local version
    version=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
    if [ -z "$version" ]; then
        fail "docs-map: version is not readable from Cargo.toml" \
            "The check derives what the front page must say from the manifest; keep the key."
        return
    fi

    case "$version" in
        0.*)
            if grep -qF -- 'master' "$front"; then
                pass "docs-map: $front states it documents the default branch, and nothing is released yet (FR-078a)"
            else
                fail "docs-map: $front does not say which version it documents (FR-078a)" \
                    "Cargo.toml is at $version, so nothing is released: the front page must say it documents \`master\`."
            fi
            ;;
        *)
            if grep -qF -- "$version" "$front"; then
                pass "docs-map: $front names the released version \`$version\` (FR-078a)"
            else
                fail "docs-map: $front does not name the released version \`$version\` (FR-078a)" \
                    "Cargo.toml says \`$version\`; the front page states which release the site documents."
            fi
            ;;
    esac
}

# FR-081: every failure the troubleshooting page names is tied to a condition the program really
# emits, so the page cannot describe a diagnostic that does not exist.
docs_troubleshooting_conditions() {
    local page=docs/user/troubleshooting.md

    [ -f "$page" ] || {
        fail "docs-map: $page is missing (FR-081)" \
            "The end-user section's page set is fixed by contracts/documentation.md."
        return
    }

    # FR-115: where the compositor collects the daemon's output.
    if grep -qF -- 'hyprland.log' "$page"; then
        pass "docs-map: $page says where the compositor collects the output (FR-115)"
    else
        fail "docs-map: $page does not say where the compositor collects the output (FR-115)" \
            "Name the log and how to retrieve it."
    fi

    # The five failures FR-081 names, each recognised by the condition it is tied to.
    local condition missing=""
    for condition in ShortcutRegistrationFailed OverlayFocusRefused SecondInstance \
        CompositorUnreachableAtStartup IconUnreadable InvalidConfigValue; do
        grep -qF -- "$condition" "$page" || missing="$missing $condition"
    done
    if [ -n "$missing" ]; then
        fail "docs-map: $page does not name the condition(s):$missing (FR-081)" \
            "Each of FR-081's named failures is tied to the diag::Condition the program emits for it."
    else
        pass "docs-map: $page ties each of FR-081's failures to a real diag::Condition (FR-081)"
    fi

    # And the other direction: nothing the page presents *as* a condition may be one the program
    # cannot emit. A mention is a backticked CamelCase name in a table's first column — which is
    # the shape the page's condition tables use, and which excludes the all-capital level names
    # (`WARN`, `ERROR`, `INFO`) that share those tables.
    local mentioned bogus=""
    while IFS= read -r mentioned; do
        [ -z "$mentioned" ] && continue
        grep -qE "^[[:space:]]+${mentioned}(,|\$)" src/diag.rs || bogus="$bogus $mentioned"
    done <<EOF
$(grep -oE '^\| `[A-Z][a-zA-Z]*[a-z][a-zA-Z]*` \|' "$page" | tr -d '`|' | tr -d ' ' | sort -u)
EOF
    if [ -n "$bogus" ]; then
        fail "docs-map: $page names condition(s) src/diag.rs does not define:$bogus (FR-081)" \
            "A troubleshooting entry must name a condition the program can actually emit."
    else
        pass "docs-map: every condition $page names is a variant of diag::Condition (FR-081)"
    fi
}

# FR-084a: the developer pages link to specs/ rather than restating it — the contracts stay
# authoritative and the site presents them.
docs_dev_links_to_specs() {
    local page silent=""
    for page in $DEV_PAGES; do
        [ -f "docs/dev/$page.md" ] || continue
        grep -qE 'specs/00[0-9]|::include\[[^]]*specs/' "docs/dev/$page.md" ||
            silent="$silent docs/dev/$page.md"
    done
    if [ -n "$silent" ]; then
        fail "docs-map: developer page(s) never reach specs/:$silent (FR-084a)" \
            "A developer page links to the specification or includes it; it does not restate it."
    else
        pass "docs-map: every developer page links to or includes specs/ (FR-084a)"
    fi
}

# ---------------------------------------------------------------------------
# changelog — a change to the program is a change a user can read about (FR-102a)
# ---------------------------------------------------------------------------

# The range this change is measured against: the merge base with the branch it is proposed into
# on a pull request, with the default branch otherwise. Prints nothing and fails when there is no
# range to compute — a shallow clone, or the default branch itself — and the check then falls back
# to the working tree, which is what a contributor running this locally is asking about.
changelog_base() {
    local base
    if [ -n "${GITHUB_BASE_REF:-}" ]; then
        base=$(git merge-base "origin/$GITHUB_BASE_REF" HEAD 2>/dev/null) && {
            printf '%s' "$base"
            return 0
        }
    fi
    if [ "$(git rev-parse -q --verify HEAD 2>/dev/null)" != "$(git rev-parse -q --verify master 2>/dev/null)" ]; then
        base=$(git merge-base master HEAD 2>/dev/null) && {
            printf '%s' "$base"
            return 0
        }
    fi
    return 1
}

changelog() {
    local file=CHANGELOG.md

    [ -f "$file" ] || {
        fail "changelog: $file is missing (FR-102)" \
            "Every release's entry is written by hand, in Keep a Changelog form."
        return
    }

    if ! grep -q '^## \[Unreleased\]' "$file"; then
        fail "changelog: $file has no [Unreleased] section (FR-102a)" \
            "The release workflow renames it to the version; without it there is nothing to close."
        return
    fi

    # What this change touched under src/. Committed changes when there is a range to compare
    # against, and the working tree when there is not.
    local touched base
    if base=$(changelog_base); then
        touched=$(git diff --name-only "$base" -- src/)
    else
        touched=$(git status --porcelain -- src/ | sed 's/^...//')
    fi

    if [ -z "$touched" ]; then
        pass "changelog: nothing under src/ changed, so no entry is owed (FR-102a)"
        return
    fi

    # An entry is a list item between the [Unreleased] heading and the next release heading. Its
    # own subsection headings do not count: a heading with nothing under it is not a change a user
    # can read about.
    local entries
    entries=$(awk '/^## \[Unreleased\]/ {inside = 1; next} /^## / {inside = 0} inside && /^- /' "$file" | wc -l)
    if [ "$entries" -eq 0 ]; then
        fail "changelog: src/ changed and $file's [Unreleased] section is empty (FR-102a)" \
            "Add what a user can now do, what changed, or what broke — in their vocabulary, not the code's. A change that alters no documented behaviour can say so in the pull request instead."
        printf '     files: %s\n' "$(printf '%s' "$touched" | tr '\n' ' ')" >&2
    else
        pass "changelog: src/ changed and $file's [Unreleased] section has $entries entr(ies) (FR-102a)"
    fi
}

licence_files
docs_map
docs_pages
docs_development_tree
docs_front_page_version
docs_troubleshooting_conditions
docs_dev_links_to_specs
changelog

if [ "$failures" -ne 0 ]; then
    printf '\n%d check(s) failed. Reproduce with: ./scripts/checks.sh\n' "$failures" >&2
    exit 1
fi

printf '\nAll checks passed.\n'
