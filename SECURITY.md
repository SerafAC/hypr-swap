# Security policy

**Do not report a vulnerability in a public issue.** The tracker is world-readable, and an issue
describing an unfixed weakness tells everyone about it at the same time as it tells the maintainer.
Both channels below are private.

## Reporting a vulnerability

**Preferred — [GitHub's private vulnerability reporting](https://github.com/SerafAC/hypr-swap/security/advisories/new).**
The *Report a vulnerability* button on the repository's Security tab opens a private advisory that
only you and the maintainer can read. It keeps the report, the discussion and the eventual fix
together, and it is where a CVE is requested from if one turns out to be warranted.

**Otherwise — <seraf_ac@hotmail.com>**, the maintainer's own address, the same one
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) uses. Use it if you have no GitHub account, or would
rather not open one to tell someone their program is broken. A plain, unencrypted mail is fine; if
you would rather not send details in the clear, say so and something can be arranged.

What helps, in rough order of usefulness:

- what an attacker can do, in one sentence — that is the part that sets everything else;
- the steps that reproduce it, and the output of `hypr-swap --environment` from the machine you saw
  it on ([the troubleshooting page](https://serafac.github.io/hypr-swap/user/troubleshooting/)
  explains what that command prints);
- the version — `hypr-swap --version` — and how it was installed;
- whether you have told anyone else, and whether you are working to a disclosure date.

Nothing here is a requirement. A vague report of something real is worth more than no report, and
the missing details can be asked for.

## What to expect

**hypr-swap is maintained by one person, on a best-effort basis, in their own time**, and a security
policy that promises otherwise would be worth nothing. So, plainly:

| | |
|---|---|
| **Acknowledgement** | Within **14 days**. If you have heard nothing by then, assume the message went astray and send it again rather than assume it was ignored |
| **Assessment** | After acknowledgement, an honest answer on whether it is a vulnerability, what it lets an attacker do, and roughly when a fix can be expected |
| **The fix** | Released as its own version — see the supported versions below — with a `Security` entry in [`CHANGELOG.md`](CHANGELOG.md) |
| **Credit** | You are named in the advisory and the changelog entry unless you ask not to be |
| **Disclosure** | Coordinated: the advisory is published when the fix is released. If a fix is taking longer than you think reasonable, say so and a date can be agreed — you are not obliged to wait indefinitely |

A report that turns out not to be a vulnerability is answered the same way, and is not a waste of
anyone's time. Reports are read by the maintainer alone and are not shared further without your
agreement.

## Supported versions

**Only the most recent release receives fixes**, and a fix is issued as a new release rather than
backported. There is no long-term-support line and no security branch: one maintainer cannot honestly
promise to maintain two.

| Version | Receives fixes |
|---|---|
| The most recent release | ✅ |
| Every release before it | ❌ — upgrade to the most recent |

**Nothing is released yet.** The public history begins at **1.0.0**
([contracts/versioning.md](specs/003-oss-release-readiness/contracts/versioning.md)); the `0.1.0` in
`Cargo.toml` is pre-publication and has never been published. Until 1.0.0 exists, the supported
version is whatever the default branch holds. This table is reviewed at every release
([the release checklist](specs/003-oss-release-readiness/quickstart.md)), so if it names a version
you cannot find, the omission is the bug.

Note that a package you installed from your distribution is built and shipped by that distribution,
not by this project. If it lags the most recent release, its security fixes lag with it, and that is
a question for whoever packages it.

## What the project does on its own side

Published advisories against the program's own dependencies are watched by `cargo-deny` on every
change and once a week on a schedule, and against the documentation site's dependencies by
Dependabot; `deny.toml` explains how an advisory with no available fix is accepted for a bounded
time rather than left to sit red forever, and a unit test fails once that time is up. The full
history was reviewed for credentials, personal data and material the project has no right to publish
before it was made public, and the outcome is recorded in
[history-review.md](specs/003-oss-release-readiness/history-review.md).

## Scope, so you know what is worth looking at

hypr-swap is a user-session daemon: it draws an overlay, reads the compositor's state over
Hyprland's own IPC sockets, and asks it to change workspaces. **It performs no network access of any
kind, runs with no elevated privileges, and opens no port or socket of its own.** It reads a
configuration file, `.desktop` entries and icon files from the paths the desktop already uses, and
talks to the compositor as the user who started it ([README](README.md#scope-and-privacy)).

That shape is what makes something interesting: a path the program reads that a less privileged
process can write, a way to make it act on input it should have rejected, or anything that lets one
user's session reach another's. Anything requiring an attacker who already runs code as you is out
of scope — at that point they can drive the compositor directly, and this program is not what stands
in the way.
