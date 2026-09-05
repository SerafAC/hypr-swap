//! What the built program says about itself: its version (FR-103, FR-104, US1-AS6) and the record
//! it leaves of its own lifetime (FR-112–FR-118, User Story 7).
//!
//! The version tests are end-to-end rather than unit tests because the question is not what
//! [`hypr_swap::compose_version`] computes — `src/lib.rs` owns that, and tests it against all four
//! documented forms — but whether the binary a user actually runs reports the version this
//! checkout carries. That can only be asked of the built artefact, through the command line, and
//! it is the same question the release workflow asks of the installed package before it publishes
//! anything.
//!
//! The lifecycle tests are end-to-end for the same reason in a different key: `diag.rs` owns and
//! unit-tests the policy, but "no daemon run ends without a record of why" (SC-042) is a claim
//! about a process — the order its output arrives in, and whether it arrives at all on the paths
//! that never finish start-up — which can only be observed from outside it.
//!
//! `--version`, `--help` and `--environment` need no compositor; the rest run against a nested
//! instance like the rest of the suite.

mod e2e;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use e2e::harness::{Nested, Setup};
use e2e::notify::NotifyLog;

use hypr_swap::compose_version;
use hypr_swap::hypr::ipc::COMPOSITOR_VERSION_VAR;

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

// ---------------------------------------------------------------------------
// User Story 7 — the daemon's record of its own lifetime
// ---------------------------------------------------------------------------

/// The application configuration used wherever a test needs one that is not the default, chosen so
/// the daemon has something to report under `settings` without depending on the machine.
const NON_DEFAULT: &str = "presentation = \"grid\"\norder = \"compositor\"\n";

/// The daemon's stderr as a list of records, with the empty trailing line dropped.
fn records(stderr: &str) -> Vec<&str> {
    stderr.lines().filter(|line| !line.is_empty()).collect()
}

/// The `key: value` lines of an `--environment` block, in order.
fn report_lines(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// FR-112, US7-AS1, SC-042: a daemon that came up says so, and says which version it is — the
/// first thing a bug report needs and the anchor every later record hangs off.
///
/// FR-113, US7-AS2: and when the session manager stops it, it says what stopped it, as the last
/// line it writes.
#[test]
fn e2e_records_start_with_version() {
    let nested = Nested::start_with(&Setup::documented());
    let daemon = nested.start_daemon();
    let stderr = daemon.stderr();
    let records = records(&stderr);

    let start = format!("INFO  daemon: hypr-swap {} started", hypr_swap::version());
    assert_eq!(
        records.first(),
        Some(&start.as_str()),
        "FR-112: the start record is the first thing the daemon says: {stderr}"
    );

    // Once, not once per connected lifetime: the daemon started once (contracts/diagnostics.md).
    assert_eq!(
        records
            .iter()
            .filter(|line| line.contains("started"))
            .count(),
        1,
        "exactly one start record: {stderr}"
    );

    // SC-042: and the run ends with a record of why, so the pair brackets the session.
    assert_eq!(
        records.last(),
        Some(&"INFO  daemon: stopping: SIGTERM"),
        "FR-113: the stopping record is the last line the process writes: {stderr}"
    );
}

/// FR-113, US7-AS2: every ordinary stop names its signal, and does so *last* — a record that
/// arrived before the daemon had finished stopping would not answer "why did it go away".
#[test]
fn e2e_records_stop_on_signal() {
    for (signal, expected) in [("TERM", "SIGTERM"), ("INT", "SIGINT")] {
        let nested = Nested::start_with(&Setup::documented());
        let daemon = nested.start_daemon();
        let pid = daemon.pid();

        let status = Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(pid.to_string())
            .status()
            .expect("kill(1) is available");
        assert!(status.success(), "SIG{signal} was delivered");

        let stderr = daemon.stderr();
        let records = records(&stderr);
        assert_eq!(
            records.last(),
            Some(&format!("INFO  daemon: stopping: {expected}").as_str()),
            "SIG{signal} is named as the cause, last: {stderr}"
        );
        // FR-114: the record is at the level the policy table gives it, and raises nothing.
        assert!(
            !stderr.contains("ERROR daemon:") && !stderr.contains("WARN  daemon:"),
            "stopping is an Info condition: {stderr}"
        );
    }
}

/// FR-113, and the spec's last edge case: a daemon that never finishes start-up is exactly the one
/// a user cannot otherwise diagnose, so it still says why it went — and still exits 3.
#[test]
fn e2e_records_stop_on_fatal_startup() {
    let notify = NotifyLog::new();
    let output = Command::new(binary())
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env("PATH", notify.path())
        .stdin(Stdio::null())
        .output()
        .expect("the application under test is built");

    assert_eq!(
        output.status.code(),
        Some(3),
        "contracts/cli.md: exit 3 when the compositor cannot be reached at start-up"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let records = records(&stderr);

    // The fatal condition first, at Error level, then the stopping record naming it — in that
    // order, because the second is the summary of the first.
    assert!(
        records[0].starts_with("ERROR compositor: cannot connect at start-up"),
        "the fatal condition is reported as it always was: {stderr}"
    );
    assert_eq!(
        records.last(),
        Some(&"INFO  daemon: stopping: cannot reach the compositor at start-up"),
        "FR-113: a start-up death still says why: {stderr}"
    );
    assert!(
        !stderr.contains("started"),
        "a daemon that never came up does not claim to have started: {stderr}"
    );
}

/// FR-114, US7-AS3: the conditions this feature added joined the existing record rather than
/// reshaping it. An invalid configuration value still reports at its existing level, under its
/// existing subject, in the existing format — and still notifies.
#[test]
fn e2e_existing_diagnostics_unchanged() {
    let notify = NotifyLog::new();
    let nested = Nested::start_with(
        &Setup::documented().with_app_config("presentation = \"tiles\"\norder = \"compositor\"\n"),
    );
    let daemon = nested.start_daemon_with_env(&[], &[("PATH", &notify.path())]);
    let stderr = daemon.stderr();

    assert!(
        stderr.contains(r#"WARN  config.presentation: unknown value "tiles""#),
        "the record is unchanged in level, subject and shape: {stderr}"
    );
    let raised = notify.wait_for(1);
    assert_eq!(
        raised.len(),
        1,
        "and it still notifies, exactly as before: {raised:?}"
    );

    // The new records sit around it without disturbing it. The configuration file is read before
    // the daemon has a compositor to serve against, so a complaint about it precedes the start
    // record — which is the right way round: the daemon reports what it found on the way up, and
    // only then claims to be up.
    let records = records(&stderr);
    let at = |needle: &str| {
        records
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no record contains {needle:?}: {stderr}"))
    };
    assert!(
        at("config.presentation") < at("started"),
        "the file was read before the daemon said it had started: {stderr}"
    );
    assert_eq!(
        at("stopping:"),
        records.len() - 1,
        "and the stopping record is still last (SC-042): {stderr}"
    );
}

/// FR-116, US7-AS5: the block a bug reporter is asked to paste, produced on demand — every line
/// present, in the documented order, with nothing from the configuration file's own text in it.
#[test]
fn e2e_environment_report() {
    let notify = NotifyLog::new();
    let nested = Nested::start_with(&Setup::documented().with_app_config(NON_DEFAULT));

    let mut command = Command::new(binary());
    nested.env(&mut command);
    let output = command
        .arg("--environment")
        .env("PATH", notify.path())
        .stdin(Stdio::null())
        .output()
        .expect("the application under test is built");

    assert_eq!(
        output.status.code(),
        Some(0),
        "contracts/cli.md: --environment prints and exits 0"
    );
    assert!(
        output.stderr.is_empty(),
        "it is an answer, not a diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = report_lines(&stdout);
    let keys: Vec<&str> = lines.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "hypr-swap",
            "hyprland",
            "config",
            "settings",
            "icon-set",
            "notify-send"
        ],
        "the six lines of contracts/cli.md, in order: {stdout}"
    );

    let value = |key: &str| {
        lines
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| panic!("{key} is present: {stdout}"), |(_, v)| v.clone())
    };
    assert_eq!(value("hypr-swap"), hypr_swap::version());
    // The nested instance is reachable, so this is a version rather than the absent-value word.
    assert!(
        value("hyprland").starts_with(char::is_numeric),
        "the compositor's own version: {stdout}"
    );
    assert!(
        value("config").ends_with("(present)"),
        "the resolved path and whether it exists: {stdout}"
    );
    assert!(
        value("notify-send") == "present",
        "the stub on PATH is found: {stdout}"
    );

    // FR-071: only the settings that differ from their defaults, in the file's own key names.
    let settings = value("settings");
    assert!(
        settings.contains(r#"presentation = "grid""#)
            && settings.contains(r#"order = "compositor""#),
        "the two settings the file changed: {stdout}"
    );
    for untouched in ["placement", "icons", "theme", "style."] {
        assert!(
            !settings.contains(untouched),
            "{untouched} is at its default and is not listed: {stdout}"
        );
    }
    // No line is ever left blank, so a pasted report has no silent gaps.
    for (key, value) in &lines {
        assert!(!value.is_empty(), "{key} has an explicit value: {stdout}");
    }
}

/// FR-117, US7-AS6, SC-043: a configuration file written for the previous release is read by this
/// build without a word — no key gone unrecognised, no value reinterpreted.
///
/// The fixture is refreshed at each release; at 1.0.0 it is the 1.0.0 contract itself, so the
/// test has something to run against before there is a previous release (research.md R43).
#[test]
fn e2e_config_from_previous_release() {
    let fixture = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("config-previous-release.toml");
    assert!(
        fixture.is_file(),
        "the committed fixture is at {}",
        fixture.display()
    );

    let notify = NotifyLog::new();
    let nested = Nested::start_with(&Setup::documented());
    let daemon = nested.start_daemon_with_env(
        &["--config", &fixture.to_string_lossy()],
        &[("PATH", &notify.path())],
    );
    let stderr = daemon.stderr();

    // Every record the run produced, minus the two that bracket every run: nothing should remain.
    let complaints: Vec<&str> = records(&stderr)
        .into_iter()
        .filter(|line| !line.starts_with("INFO  daemon:"))
        .collect();
    assert!(
        complaints.is_empty(),
        "a file written for the previous release is read in silence: {complaints:?}"
    );
    assert!(
        notify.raised().is_empty(),
        "and raises nothing on the user's screen: {:?}",
        notify.raised()
    );
}

/// FR-118, US1-AS5: a compositor older than this release supports is named, and the daemon carries
/// on — because it may well work, and taking the user's switcher away over a version comparison is
/// a worse failure than the obscure one this record replaces (research.md R42).
#[test]
fn e2e_unsupported_compositor_version() {
    // Both halves run against one nested instance: the harness allows only one at a time, and the
    // question — what the daemon does with a version it was handed — is the same compositor's.
    let nested = Nested::start_with(&Setup::documented());

    let daemon = nested.start_daemon_with_env(&[], &[(COMPOSITOR_VERSION_VAR, "0.52.1")]);
    let stderr = daemon.stderr();

    assert!(
        stderr.contains(&format!(
            "WARN  compositor: Hyprland 0.52.1 is below the supported range \
             ({}); continuing anyway",
            hypr_swap::SUPPORTED_HYPRLAND
        )),
        "the record names the version found and the range it was measured against: {stderr}"
    );
    // Not fatal: the daemon reached the point of reporting that it had started, which is after the
    // world, the client and the event stream are all up.
    assert!(
        stderr.contains("INFO  daemon: hypr-swap"),
        "the daemon carried on and served: {stderr}"
    );
    // Once, not once per anything: the question is asked at start-up and not asked again.
    assert_eq!(
        stderr.matches("is below the supported range").count(),
        1,
        "reported at most once: {stderr}"
    );

    // And a version that cannot be read at all is the same kind of record, not a different fate.
    let daemon = nested.start_daemon_with_env(&[], &[(COMPOSITOR_VERSION_VAR, "next")]);
    let stderr = daemon.stderr();
    assert!(
        stderr.contains(&format!(
            "WARN  compositor: Hyprland version \"next\" could not be read; \
             supported range is {}",
            hypr_swap::SUPPORTED_HYPRLAND
        )),
        "an unreadable version quotes back what the compositor said: {stderr}"
    );
    assert!(
        stderr.contains("INFO  daemon: hypr-swap"),
        "and is likewise not fatal: {stderr}"
    );
}
