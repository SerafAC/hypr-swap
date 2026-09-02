# History review (FR-066a, T084)

The whole history, checked for credentials, personal data and material the project has no right to
publish, before it is read by anyone who did not write it.

**Run on 2026-09-02**, against `master` at `aa3a81b`, 33 commits.

> **Ordering note, recorded rather than smoothed over.** FR-066a and T084a require this review to be
> clean *before* the repository is made public. The repository was made public first, and this
> review ran immediately afterwards. The outcome is clean, so nothing was exposed — but the
> sequence was not the one the requirement asks for, and a re-run before any future history rewrite
> should follow it.

## Credentials

```
gitleaks detect --log-opts=--all --redact
```

`gitleaks` 8.30.1 — the version [quickstart.md](./quickstart.md)'s tooling table names.

```
33 commits scanned.
scanned ~2127429 bytes (2.13 MB) in 324ms
no leaks found
```

**Outcome: clean.** No secret, token, key or credential in any commit.

## Personal data

| Checked | Finding |
|---|---|
| Author and committer identities | One human author across all 33 commits, `SerafAC <seraf_ac@hotmail.com>`, plus `GitHub <noreply@github.com>` on the repository-creation commit that brought `LICENSE` in. The address is the maintainer's own, deliberately published as the project's point of contact. |
| The two overlay screenshots | `docs/assets/overlay-{list,grid}.png` show a staged desktop, not a real one: the window titles and program classes were written for the capture (T013 record). No personal file name, path or account appears in either. |
| Paths and home directories | No absolute path under a developer's home directory is committed anywhere. |

**Outcome: clean.** The only personal datum in the history is the maintainer's own contact address,
which is there on purpose.

## Material the project has no right to publish

| Checked | Finding |
|---|---|
| Every non-source path ever committed | `assets/placeholder.svg`, `docs/assets/overlay-{list,grid}.png`, `protocols/hyprland-global-shortcuts-v1.xml`, `LICENSE`. Nothing else: no vendored source, no binary, no archive. |
| Largest blobs in the history | All are the project's own text — `src/ui/layout.rs`, `src/theme.rs` and this feature's `tasks.md`. Nothing large arrived and was later deleted. |
| The vendored protocol | `protocols/hyprland-global-shortcuts-v1.xml` is redistributed under its own licence and is accounted for in `THIRD-PARTY.md` (T078, FR-063). |
| Icon artwork in the screenshots | The captures render program icons from the icon set installed on the machine that took them. They are incidental to a screenshot of the program's own interface, are not redistributed as icon files, and no icon file is committed. |

**Outcome: clean.** Nothing in the history is republished without the right to do so.

## Verdict

**Clean on all three counts.** The history is safe to publish, and is published.

Re-run this review before any future history rewrite, and record the outcome here rather than
replacing it.
