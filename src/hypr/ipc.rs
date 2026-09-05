//! Hyprland's request socket (`.socket.sock`): queries, dispatches and batches.
//!
//! One connection per request — write the request, read the response to EOF — which is what
//! `hyprctl` itself does and what the compositor expects. The wire format is a line-oriented
//! text protocol (`contracts/compositor-ipc.md`, research.md R2), so encoding and response
//! classification are plain string work and are unit-tested as such.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::de::DeserializeOwned;

use crate::actions::{CommandPlan, ExpectedState};
use crate::model::{CompositorVersion, Monitor, Window, Workspace};

/// Environment variable that makes a nominated batch step fail.
///
/// The one documented substitution reserved for the E2E rollback tests (research.md R14): a
/// genuine dispatcher failure cannot be provoked from outside the compositor, so it is injected.
/// Unset — which is always the case in normal use — this costs one environment read at start-up.
///
/// The injection fires **once** per process. A rollback that was sabotaged along with the batch
/// it repairs could only ever demonstrate FR-013c, and FR-013b — the ordinary case, where the
/// undo works — would be untestable.
pub const FAULT_INJECTION_VAR: &str = "HYPR_SWAP_E2E_FAIL_BATCH_STEP";

/// Environment variable that substitutes the version string the compositor reports.
///
/// FR-118's warning fires on a compositor older than this project supports, and there is no way
/// to run one of those from inside the E2E suite — the nested instance is the developer's own
/// Hyprland. So the one value the decision reads is substituted, exactly as
/// [`FAULT_INJECTION_VAR`] substitutes a failing dispatch step and `diag::PAINT_RECORDS_VAR`
/// opens the paint records (plan.md → Complexity Tracking).
///
/// Unset — which is always the case in normal use — this costs one environment read at start-up
/// and changes nothing.
pub const COMPOSITOR_VERSION_VAR: &str = "HYPR_SWAP_E2E_COMPOSITOR_VERSION";

/// Batched responses are concatenated with this separator.
const BATCH_SEPARATOR: &str = "\n\n\n";

/// The response every successful dispatch returns.
const OK: &str = "ok";

#[derive(Debug)]
pub enum IpcError {
    /// The socket could not be reached — the compositor is gone, or was never there.
    Unreachable(PathBuf, std::io::Error),
    /// The socket answered, but the answer was not what the request asked for.
    Rejected(String),
    /// A JSON reply did not have the shape this application expects.
    Malformed(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Rejected(message) | Self::Malformed(message) => f.write_str(message),
        }
    }
}

/// How a dispatched plan ended up (FR-013a–c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Applied, and the compositor reports the state the plan expected.
    Verified,
    /// Something failed, and the pre-state was restored (FR-013b).
    RolledBack { reason: String },
    /// Something failed and so did the undo, so the user is told where their workspaces
    /// actually are (FR-013c).
    RollbackFailed { reason: String, resulting: String },
}

/// The one-shot fault injected for the E2E rollback tests.
///
/// `used` is shared across clones because the application clones its [`Ipc`] into the event loop
/// callbacks; a per-clone flag would fire once per clone instead of once per process.
#[derive(Debug, Clone)]
struct Fault {
    step: usize,
    used: Arc<AtomicBool>,
}

/// A connection factory for the request socket. Holds no socket of its own: each request opens
/// and closes one, so a compositor restart cannot leave a stale handle behind.
#[derive(Debug, Clone)]
pub struct Ipc {
    socket: PathBuf,
    fault: Option<Fault>,
}

impl Ipc {
    /// Locate the request socket for a compositor instance.
    #[must_use]
    pub fn new(runtime_dir: &Path, signature: &str) -> Self {
        Self {
            socket: request_socket_path(runtime_dir, signature),
            fault: injected_fault(),
        }
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    /// Send one request and read the whole response.
    ///
    /// # Errors
    /// [`IpcError::Unreachable`] when the socket cannot be reached — the compositor is gone.
    pub fn request(&self, request: &str) -> Result<String, IpcError> {
        let mut stream = UnixStream::connect(&self.socket)
            .map_err(|e| IpcError::Unreachable(self.socket.clone(), e))?;
        stream
            .write_all(request.as_bytes())
            .map_err(|e| IpcError::Unreachable(self.socket.clone(), e))?;
        stream
            .flush()
            .map_err(|e| IpcError::Unreachable(self.socket.clone(), e))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| IpcError::Unreachable(self.socket.clone(), e))?;
        Ok(response)
    }

    fn query<T: DeserializeOwned>(&self, what: &str) -> Result<T, IpcError> {
        let response = self.request(&format!("j/{what}"))?;
        serde_json::from_str(&response)
            .map_err(|e| IpcError::Malformed(format!("j/{what} did not deserialise: {e}")))
    }

    /// # Errors
    /// If the compositor is unreachable, or answers with JSON this application cannot read.
    pub fn monitors(&self) -> Result<Vec<Monitor>, IpcError> {
        self.query("monitors")
    }

    /// # Errors
    /// If the compositor is unreachable, or answers with JSON this application cannot read.
    pub fn workspaces(&self) -> Result<Vec<Workspace>, IpcError> {
        self.query("workspaces")
    }

    /// # Errors
    /// If the compositor is unreachable, or answers with JSON this application cannot read.
    pub fn clients(&self) -> Result<Vec<Window>, IpcError> {
        self.query("clients")
    }

    /// The compositor's own version, for the FR-118 check and the `--environment` report.
    ///
    /// Asked once, at start-up. [`COMPOSITOR_VERSION_VAR`] substitutes the answer when it is set,
    /// and the socket is not touched at all in that case.
    ///
    /// # Errors
    /// If the compositor is unreachable, or answers with JSON this application cannot read.
    pub fn version(&self) -> Result<CompositorVersion, IpcError> {
        match substituted_version() {
            Some(version) => Ok(version),
            None => self.query("version"),
        }
    }

    /// Dispatch a list of commands as a single batch, so the compositor applies them in one pass
    /// and no intermediate state is ever presented (SC-010).
    ///
    /// `commands` are dispatcher invocations without the `dispatch` keyword, e.g. `workspace 3`.
    ///
    /// # Errors
    /// [`IpcError::Unreachable`] if the compositor is gone, or [`IpcError::Rejected`] naming the
    /// step that failed — which is what triggers the rollback path (FR-013a).
    pub fn dispatch(&self, commands: &[String]) -> Result<(), IpcError> {
        if commands.is_empty() {
            return Ok(());
        }

        // The injected fault omits the nominated step, so the compositor really is left in the
        // half-applied state the rollback path exists to repair.
        let (sent, injected) = match &self.fault {
            Some(fault)
                if fault.step >= 1
                    && fault.step <= commands.len()
                    && !fault.used.swap(true, Ordering::Relaxed) =>
            {
                let mut kept = commands.to_vec();
                let dropped = kept.remove(fault.step - 1);
                (kept, Some((fault.step, dropped)))
            }
            _ => (commands.to_vec(), None),
        };

        let response = self.request(&encode_dispatch(&sent))?;
        classify_dispatch(&response, sent.len())?;

        if let Some((step, dropped)) = injected {
            return Err(IpcError::Rejected(format!(
                "injected failure at step {step} ({dropped})"
            )));
        }
        Ok(())
    }

    /// Dispatch a plan, read the result back, and undo it if the compositor did not end up where
    /// the plan said it would (FR-013, FR-013a).
    ///
    /// Read-back rather than trust: a batch is not a transaction — a rejected step leaves its
    /// predecessors applied (research.md R8) — and a dispatcher that answers `ok` can still have
    /// done something other than what was asked. Comparing against the compositor makes it the
    /// source of truth for both the check and, in the tests, the oracle.
    ///
    /// The FR-013 post-condition, that both affected monitors still show an active workspace,
    /// falls out of the comparison: every monitor the plan touches is named in
    /// [`ExpectedState::active`], so a monitor left showing anything else is a mismatch.
    #[must_use]
    pub fn dispatch_verified(&self, plan: &CommandPlan) -> DispatchOutcome {
        let Some(reason) = self.attempt(&plan.commands, &plan.expected) else {
            return DispatchOutcome::Verified;
        };

        // FR-013a: undo the parts that did land. The rollback aims at the recorded pre-state, so
        // it does not matter how far the batch got.
        let rollback_failure = self.attempt(&plan.rollback.commands, &plan.rollback.expected);
        let resulting = rollback_failure
            .is_some()
            .then(|| self.describe(&plan.rollback.expected));
        classify(reason, rollback_failure, resulting)
    }

    /// Dispatch a batch and confirm the state it claimed it would produce. `None` on success,
    /// otherwise why not.
    fn attempt(&self, commands: &[String], expected: &ExpectedState) -> Option<String> {
        match self.dispatch(commands) {
            Ok(()) => self.verify(expected),
            Err(e) => Some(e.to_string()),
        }
    }

    fn verify(&self, expected: &ExpectedState) -> Option<String> {
        match (self.monitors(), self.workspaces()) {
            (Ok(monitors), Ok(workspaces)) => expected.mismatch(&monitors, &workspaces),
            // Unable to look is not the same as looking and seeing the wrong thing, but it is
            // just as much a reason not to claim success.
            (Err(e), _) | (_, Err(e)) => {
                Some(format!("the resulting state could not be read: {e}"))
            }
        }
    }

    /// Where the workspaces a plan touched have actually ended up, for the FR-013c report.
    fn describe(&self, state: &ExpectedState) -> String {
        self.workspaces().map_or_else(
            |e| format!("the compositor could not be asked where they are: {e}"),
            |workspaces| state.describe_actual(&workspaces),
        )
    }
}

/// What a dispatch-then-verify-then-undo sequence adds up to.
///
/// Pure, and separate from the I/O that feeds it, so every combination is unit-testable — which
/// matters most for the FR-013c arm, the one a live compositor cannot be made to produce.
fn classify(
    reason: String,
    rollback_failure: Option<String>,
    resulting: Option<String>,
) -> DispatchOutcome {
    match rollback_failure {
        None => DispatchOutcome::RolledBack { reason },
        Some(rollback) => DispatchOutcome::RollbackFailed {
            reason: format!("{reason}; the rollback failed too: {rollback}"),
            resulting: resulting.unwrap_or_else(|| "the resulting state is unknown".to_owned()),
        },
    }
}

/// `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`.
#[must_use]
pub fn request_socket_path(runtime_dir: &Path, signature: &str) -> PathBuf {
    instance_dir(runtime_dir, signature).join(".socket.sock")
}

/// `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`.
#[must_use]
pub fn event_socket_path(runtime_dir: &Path, signature: &str) -> PathBuf {
    instance_dir(runtime_dir, signature).join(".socket2.sock")
}

fn instance_dir(runtime_dir: &Path, signature: &str) -> PathBuf {
    runtime_dir.join("hypr").join(signature)
}

/// Encode dispatcher commands as one request: a lone command goes as `/dispatch …`, several go
/// as a `[[BATCH]]` so the compositor applies them together.
#[must_use]
pub fn encode_dispatch(commands: &[String]) -> String {
    match commands {
        [] => String::new(),
        [only] => format!("/dispatch {only}"),
        many => {
            let joined = many
                .iter()
                .map(|c| format!("/dispatch {c}"))
                .collect::<Vec<_>>()
                .join("; ");
            format!("[[BATCH]]{joined}")
        }
    }
}

/// A dispatch succeeded when every step answered `ok`.
///
/// Batched responses arrive concatenated, so the count is checked too: a compositor that silently
/// dropped a step must not read as success.
///
/// # Errors
/// [`IpcError::Rejected`] naming the first step whose result was not `ok`, or reporting a result
/// count that does not match the number of commands sent.
pub fn classify_dispatch(response: &str, expected: usize) -> Result<(), IpcError> {
    let results: Vec<&str> = response
        .split(BATCH_SEPARATOR)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();

    if results.len() != expected {
        return Err(IpcError::Rejected(format!(
            "expected {expected} result(s), got {}: {}",
            results.len(),
            response.trim()
        )));
    }
    match results.iter().position(|result| *result != OK) {
        Some(index) => Err(IpcError::Rejected(format!(
            "step {} failed: {}",
            index + 1,
            results[index]
        ))),
        None => Ok(()),
    }
}

/// The version [`COMPOSITOR_VERSION_VAR`] names, if it names one.
///
/// Split from [`Ipc::version`] so the gate's rule — an unset or empty value substitutes nothing —
/// is unit-testable without a compositor to ask.
fn substituted_version() -> Option<CompositorVersion> {
    substitution(std::env::var_os(COMPOSITOR_VERSION_VAR).as_deref())
}

fn substitution(value: Option<&std::ffi::OsStr>) -> Option<CompositorVersion> {
    let version = value?.to_str()?;
    if version.is_empty() {
        return None;
    }
    Some(CompositorVersion {
        version: version.to_owned(),
        // A substituted version has no tag: the report says so rather than inventing one.
        tag: None,
    })
}

fn injected_fault() -> Option<Fault> {
    let step = std::env::var(FAULT_INJECTION_VAR).ok()?.parse().ok()?;
    Some(Fault {
        step,
        used: Arc::new(AtomicBool::new(false)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_paths_follow_the_documented_layout() {
        let runtime = Path::new("/run/user/1000");
        assert_eq!(
            request_socket_path(runtime, "abc_123"),
            PathBuf::from("/run/user/1000/hypr/abc_123/.socket.sock")
        );
        assert_eq!(
            event_socket_path(runtime, "abc_123"),
            PathBuf::from("/run/user/1000/hypr/abc_123/.socket2.sock")
        );
    }

    #[test]
    fn a_single_command_is_encoded_as_a_plain_dispatch() {
        assert_eq!(
            encode_dispatch(&["workspace 3".to_owned()]),
            "/dispatch workspace 3"
        );
    }

    #[test]
    fn several_commands_are_encoded_as_one_batch() {
        let commands = vec![
            "swapactiveworkspaces eDP-1 HEADLESS-2".to_owned(),
            "focusmonitor eDP-1".to_owned(),
        ];
        assert_eq!(
            encode_dispatch(&commands),
            "[[BATCH]]/dispatch swapactiveworkspaces eDP-1 HEADLESS-2; /dispatch focusmonitor eDP-1"
        );
    }

    #[test]
    fn an_empty_command_list_encodes_to_nothing() {
        assert_eq!(encode_dispatch(&[]), "");
    }

    #[test]
    fn a_batch_of_three_keeps_every_step_in_order() {
        let commands = vec![
            "moveworkspacetomonitor 4 eDP-1".to_owned(),
            "moveworkspacetomonitor 2 HEADLESS-2".to_owned(),
            "focusworkspaceoncurrentmonitor 4".to_owned(),
        ];
        let encoded = encode_dispatch(&commands);
        assert!(encoded.starts_with("[[BATCH]]"));
        assert_eq!(encoded.matches("/dispatch ").count(), 3);
        let move_first = encoded
            .find("moveworkspacetomonitor 4")
            .expect("step 1 present");
        let move_second = encoded
            .find("moveworkspacetomonitor 2")
            .expect("step 2 present");
        let focus = encoded
            .find("focusworkspaceoncurrentmonitor")
            .expect("step 3 present");
        assert!(move_first < move_second && move_second < focus);
    }

    #[test]
    fn a_single_ok_is_success() {
        assert!(classify_dispatch("ok", 1).is_ok());
        assert!(classify_dispatch("ok\n", 1).is_ok());
    }

    #[test]
    fn concatenated_oks_are_success() {
        // The compositor separates batched results with a blank-line run.
        assert!(classify_dispatch("ok\n\n\nok", 2).is_ok());
        assert!(classify_dispatch("ok\n\n\nok\n\n\nok", 3).is_ok());
    }

    #[test]
    fn an_error_string_names_the_step_that_failed() {
        let e = classify_dispatch("ok\n\n\nBad workspace", 2).expect_err("step 2 failed");
        let message = e.to_string();
        assert!(message.contains("step 2"), "{message}");
        assert!(message.contains("Bad workspace"), "{message}");
    }

    #[test]
    fn a_first_step_failure_is_reported_as_step_one() {
        let e = classify_dispatch("Invalid dispatcher\n\n\nok", 2).expect_err("step 1 failed");
        assert!(e.to_string().contains("step 1"), "{e}");
    }

    #[test]
    fn a_lone_error_response_is_a_failure_not_a_success() {
        assert!(classify_dispatch("Invalid dispatcher", 1).is_err());
        assert!(classify_dispatch("Bad workspace", 1).is_err());
    }

    #[test]
    fn a_dropped_step_is_detected_by_the_result_count() {
        let e = classify_dispatch("ok", 2).expect_err("one result for two commands");
        assert!(e.to_string().contains("expected 2"), "{e}");
    }

    #[test]
    fn an_empty_response_to_a_dispatch_is_a_failure() {
        assert!(classify_dispatch("", 1).is_err());
    }

    // -----------------------------------------------------------------------------------------
    // The dispatch-verify-rollback outcome (T059), and the injected fault (T060).
    // -----------------------------------------------------------------------------------------

    #[test]
    fn a_failure_the_rollback_repaired_is_reported_as_rolled_back() {
        // FR-013b: the user asked for a change that did not happen, and their layout is intact.
        assert_eq!(
            classify("step 2 failed: Invalid dispatcher".to_owned(), None, None),
            DispatchOutcome::RolledBack {
                reason: "step 2 failed: Invalid dispatcher".to_owned()
            }
        );
    }

    #[test]
    fn a_failure_the_rollback_could_not_repair_carries_the_resulting_state() {
        // FR-013c: both messages matter — why it went wrong, and where things are now.
        let outcome = classify(
            "step 2 failed".to_owned(),
            Some("workspace 1 is on eDP-1 rather than HEADLESS-2".to_owned()),
            Some("workspace 2 is on eDP-1 and workspace 1 is on eDP-1".to_owned()),
        );
        let DispatchOutcome::RollbackFailed { reason, resulting } = outcome else {
            panic!("a double failure is never reported as a rollback that worked");
        };
        assert!(reason.contains("step 2 failed"), "{reason}");
        assert!(reason.contains("the rollback failed too"), "{reason}");
        assert_eq!(
            resulting,
            "workspace 2 is on eDP-1 and workspace 1 is on eDP-1"
        );
    }

    #[test]
    fn a_double_failure_with_nothing_readable_still_says_so() {
        // Losing the compositor mid-rollback must not silently degrade to the FR-013b message.
        let outcome = classify(
            "step 1 failed".to_owned(),
            Some("unreachable".to_owned()),
            None,
        );
        assert!(matches!(outcome, DispatchOutcome::RollbackFailed { .. }));
    }

    #[test]
    fn the_injected_fault_drops_its_step_once_and_then_stops() {
        // T060: the rollback batch that follows the sabotaged one must be allowed to succeed.
        let fault = Fault {
            step: 2,
            used: Arc::new(AtomicBool::new(false)),
        };
        assert!(
            !fault.used.swap(true, Ordering::Relaxed),
            "the first batch is sabotaged"
        );
        assert!(
            fault.used.swap(true, Ordering::Relaxed),
            "the second is not"
        );
    }

    #[test]
    fn the_fault_flag_is_shared_across_clones() {
        // The application clones its `Ipc` into the event loop callbacks; a per-clone flag would
        // fire once per clone and sabotage the rollback as well.
        let ipc = Ipc {
            socket: PathBuf::from("/nonexistent"),
            fault: Some(Fault {
                step: 1,
                used: Arc::new(AtomicBool::new(false)),
            }),
        };
        let clone = ipc.clone();
        let used = |ipc: &Ipc| {
            ipc.fault
                .as_ref()
                .is_some_and(|f| f.used.swap(true, Ordering::Relaxed))
        };
        assert!(!used(&ipc));
        assert!(used(&clone), "the clone sees the original's use");
    }

    #[test]
    fn without_the_environment_variable_there_is_no_fault_at_all() {
        // The hook costs one environment read and nothing else in normal use.
        assert!(
            Ipc::new(Path::new("/run/user/1000"), "abc").fault.is_none()
                || std::env::var(FAULT_INJECTION_VAR).is_ok()
        );
    }

    // T093 — the compositor-version substitution (research.md R42).

    #[test]
    fn the_version_gate_substitutes_only_when_it_is_given_something() {
        use std::ffi::OsStr;

        // Unset and empty are both "ask the compositor", which is what keeps the hook inert in
        // every ordinary run.
        assert_eq!(substitution(None), None);
        assert_eq!(substitution(Some(OsStr::new(""))), None);

        let substituted = substitution(Some(OsStr::new("0.52.1"))).expect("a value substitutes");
        assert_eq!(substituted.version, "0.52.1");
        assert_eq!(
            substituted.tag, None,
            "a substituted version has no tag to report"
        );

        // The value is passed through verbatim, unparsed: deciding what it means is
        // `CompositorVersion::supported`'s job, and the hook must be able to feed it nonsense.
        assert_eq!(
            substitution(Some(OsStr::new("next"))).map(|v| v.version),
            Some("next".to_owned())
        );
    }

    #[test]
    fn nothing_is_substituted_in_an_ordinary_run() {
        assert!(
            substituted_version().is_none() || std::env::var(COMPOSITOR_VERSION_VAR).is_ok(),
            "a version is only ever substituted by {COMPOSITOR_VERSION_VAR}"
        );
    }
}
