//! What the built program says about itself (FR-103, FR-104, US1-AS6).
//!
//! These are end-to-end rather than unit tests because the question is not what
//! [`hypr_swap::compose_version`] computes — `src/lib.rs` owns that, and tests it against all four
//! documented forms — but whether the binary a user actually runs reports the version this
//! checkout carries. That can only be asked of the built artefact, through the command line, and
//! it is the same question the release workflow asks of the installed package before it publishes
//! anything.
//!
//! Neither test needs a compositor: `--version` prints and exits (`contracts/cli.md`).

use std::path::{Path, PathBuf};
use std::process::Command;

use hypr_swap::compose_version;

/// The repository this test binary was built from.
fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The program under test, as cargo built it.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hypr-swap"))
}

/// `--version`, as a user would run it: one line, exit 0.
fn reported_version() -> String {
    let output = Command::new(binary())
        .arg("--version")
        .output()
        .expect("the application under test is built");
    assert!(
        output.status.success(),
        "--version exits 0 (contracts/cli.md); it exited {:?}",
        output.status.code()
    );
    let line = String::from_utf8(output.stdout).expect("--version prints text");
    let line = line.trim_end().to_owned();
    assert!(
        line.starts_with("hypr-swap "),
        "--version prints `hypr-swap <version>`; it printed `{line}`"
    );
    line
}

/// The `version` of the `[package]` section, read off the manifest on disk rather than through
/// `CARGO_PKG_VERSION`: the point is to compare the binary against the file the release workflow
/// raises, not against the same constant twice.
fn manifest_version() -> String {
    let manifest = std::fs::read_to_string(manifest_dir().join("Cargo.toml"))
        .expect("the manifest is beside the tests");
    let mut in_package = false;
    for line in manifest.lines() {
        if line.starts_with('[') {
            in_package = line.trim() == "[package]";
            continue;
        }
        if in_package && let Some(value) = line.strip_prefix("version = ") {
            return value.trim().trim_matches('"').to_owned();
        }
    }
    panic!("Cargo.toml's [package] section declares no version");
}

/// What `build.rs` would have been told when this binary was built (research.md R37). `None` is
/// the source-archive case — no git, or a checkout git will not answer for — which FR-104 reads
/// as "report the package version alone".
fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .current_dir(manifest_dir())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// FR-103: the version the binary reports is the version the manifest declares — which is what
/// makes "the runtime version, the tag and the changelog heading agree" checkable at release time.
#[test]
fn e2e_version_matches_metadata() {
    let manifest = manifest_version();
    let reported = reported_version();
    let version = reported
        .strip_prefix("hypr-swap ")
        .expect("the prefix was asserted above");

    // The part before the `+` is the version proper; anything after it says which source this
    // build came from and is FR-104's business, asserted by the test below.
    let package = version.split('+').next().unwrap_or(version);
    assert_eq!(
        package, manifest,
        "`{reported}` disagrees with Cargo.toml's version `{manifest}`"
    );
}

/// FR-104, US1-AS6: a build that is not exactly a release tag identifies the source it came from,
/// so a bug report from a development build can be traced to one commit.
///
/// Both directions of the rule are asserted, because which one applies depends on where the
/// checkout is: from the `v<version>` tag there is nothing to add and the plain version stands,
/// and from anywhere else — the ordinary case, and the one US1-AS6 describes — the describe is
/// carried as a `+` suffix.
#[test]
fn e2e_version_reports_build() {
    let manifest = manifest_version();
    let describe = git_describe();
    let reported = reported_version();
    let version = reported
        .strip_prefix("hypr-swap ")
        .expect("the prefix was asserted above");

    assert_eq!(
        version,
        compose_version(&manifest, describe.as_deref()),
        "`{reported}` is not what FR-104 composes from `{manifest}` and {describe:?}"
    );

    match describe.as_deref() {
        // A source archive, or a checkout git cannot speak for: the package version alone.
        None | Some("") => assert!(
            !version.contains('+'),
            "`{reported}` carries a source suffix, but there is no git describe to have got it from"
        ),
        Some(tag) if tag.strip_prefix('v') == Some(manifest.as_str()) => assert_eq!(
            version, manifest,
            "`{reported}` was built from its own release tag and needs no suffix"
        ),
        Some(tag) => assert_eq!(
            version,
            format!("{manifest}+{tag}"),
            "a build from `{tag}` reports where it came from (FR-104)"
        ),
    }
}
