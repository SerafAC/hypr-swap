//! `wayland-scanner` expands the vendored protocol XML through a procedural macro in
//! `src/ui/shortcuts.rs`. Cargo cannot see that dependency by itself, so this script declares it:
//! editing the protocol re-runs the codegen.
//!
//! It also asks git what source this build came from, for `hypr_swap::version()` to compose
//! (FR-104, research.md R37). The decision — whether a suffix applies at all — belongs to
//! `compose_version`, not here, which is what makes it unit-testable; this script only reports
//! the raw fact.

use std::path::PathBuf;
use std::process::Command;

const PROTOCOL: &str = "protocols/hyprland-global-shortcuts-v1.xml";

/// The files that change when the answer to `git describe` changes: which branch is checked out,
/// the tags once git has packed them, and what is staged — the last standing in for the `--dirty`
/// flag, which no file tracks exactly. The branch's own ref file is added at run time by
/// [`head_ref`], because a commit on the current branch rewrites that and leaves `.git/HEAD`
/// untouched.
const GIT_INPUTS: [&str; 3] = [".git/HEAD", ".git/packed-refs", ".git/index"];

fn main() {
    println!("cargo::rerun-if-changed={PROTOCOL}");
    describe();
}

/// Emit `git describe --tags --always --dirty` verbatim as `HYPR_SWAP_GIT_DESCRIBE`, or emit
/// nothing at all when there is no answer — git absent, not a repository, or no output. A build
/// with no git is the source-archive case, and `compose_version` reads its absence as "report the
/// package version alone" (FR-104).
fn describe() {
    for input in GIT_INPUTS.iter().map(PathBuf::from).chain(head_ref()) {
        // Declared only when it exists: cargo treats a missing `rerun-if-changed` path as
        // perpetually changed, and a source archive has no `.git` at all.
        if input.exists() {
            println!("cargo::rerun-if-changed={}", input.display());
        }
    }

    let Ok(output) = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let Ok(text) = String::from_utf8(output.stdout) else {
        return;
    };
    // Trimming the trailing newline is not a decision about the version: a cargo environment
    // value is a single line by construction.
    let text = text.trim();
    if !text.is_empty() {
        println!("cargo::rustc-env=HYPR_SWAP_GIT_DESCRIBE={text}");
    }
}

/// The ref file `.git/HEAD` points at, when it points at one — `.git/refs/heads/<branch>` for an
/// ordinary checkout, nothing for a detached HEAD, which names its commit directly and so changes
/// `.git/HEAD` itself.
fn head_ref() -> Option<PathBuf> {
    let head = std::fs::read_to_string(".git/HEAD").ok()?;
    let target = head.trim().strip_prefix("ref: ")?;
    Some(PathBuf::from(".git").join(target))
}
