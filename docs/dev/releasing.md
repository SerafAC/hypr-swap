---
title: Releasing
description: The one triggered workflow that cuts a release, what it refuses to do, and how to resume it after a partial failure.
---

A release is **one workflow, triggered by hand, with one input**. Nothing about it lives only in a
maintainer's head, and nothing is published from a tree that is not ready.

You give it a version. It does the rest — raising the version, closing the changelog section,
tagging, building the binary and both packages, installing them in clean containers to prove they
work, publishing the release with its notes, verifying every asset against its checksum, and
pushing the updated Arch recipe.

## Before you trigger it

Three things are worth checking, because they are the three that will stop it:

- the `[Unreleased]` section of `CHANGELOG.md` is written and non-empty — it becomes the release
  notes, so it needs to read as something a user would want;
- the gating checks are green on the commit you are releasing;
- the release checklist has been walked. That is where the requirements no automated check can
  judge get judged: the supported-versions list in `SECURITY.md`, the distribution matrix, the
  packager block, the previous-release configuration fixture.

## The procedure

What follows is not a description of the workflow — it **is** the workflow's contract, included at
build time from
[`specs/003-oss-release-readiness/contracts/release.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/003-oss-release-readiness/contracts/release.md),
so this page and the specification the workflow is built to cannot drift apart.

::include[../../specs/003-oss-release-readiness/contracts/release.md]

## Two properties worth understanding

**A refused release leaves no trace.** Every precondition is checked before the first commit is
written, so a run that is going to fail fails having changed nothing — no orphan tag, no bumped
version to revert, no half-written changelog.

**Every artefact is built from the tag, never from the branch head.** This is what makes a resumed
run safe: re-running for a version whose tag exists checks out that tag rather than creating one,
rebuilds from that exact commit, and replaces the draft's assets. The same version cannot produce
two different files.

## What counts as a breaking change

Versioning is semantic, from `1.0.0`. What makes a change breaking is defined over the whole
**contract surface** — the shortcut names, the configuration schema, the command line, the
diagnostic conditions and the install map, not merely the Rust API — and that definition is in
[`contracts/versioning.md`](https://github.com/SerafAC/hypr-swap/blob/master/specs/003-oss-release-readiness/contracts/versioning.md).
Read it before deciding whether the number you are about to release is a minor or a major one.

## Between releases

Builds that are not from a tag identify themselves as such: `--version` reports the commit it was
built from, so a bug report from a development build can be traced to the exact source it came out
of. Nothing about that path is manual.
