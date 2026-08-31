//! `wayland-scanner` expands the vendored protocol XML through a procedural macro in
//! `src/ui/shortcuts.rs`. Cargo cannot see that dependency by itself, so this script declares it:
//! editing the protocol re-runs the codegen.
//!
//! It also asks git what source this build came from, for `hypr_swap::version()` to compose
//! (FR-104, research.md R37). The decision — whether a suffix applies at all — belongs to
//! `compose_version`, not here, which is what makes it unit-testable; this script only reports
//! the raw fact.

use std::path::Path;
use std::process::Command;

const PROTOCOL: &str = "protocols/hyprland-global-shortcuts-v1.xml";

/// The files that change when the answer to `git describe` changes: the current commit, and the
/// tags once git has packed them. Each is declared only when it exists — cargo treats a missing
/// `rerun-if-changed` path as perpetually changed, and a source archive has no `.git` at all.
const GIT_INPUTS: [&str; 2] = [".git/HEAD", ".git/packed-refs"];

fn main() {
    println!("cargo::rerun-if-changed={PROTOCOL}");
    describe();
}

/// Emit `git describe --tags --always --dirty` verbatim as `HYPR_SWAP_GIT_DESCRIBE`, or emit
/// nothing at all when there is no answer — git absent, not a repository, or no output. A build
/// with no git is the source-archive case, and `compose_version` reads its absence as "report the
/// package version alone" (FR-104).
fn describe() {
    for input in GIT_INPUTS {
        if Path::new(input).exists() {
            println!("cargo::rerun-if-changed={input}");
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
