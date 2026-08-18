//! Hyprland's request socket (`.socket.sock`): queries, dispatches and batches.
//!
//! One connection per request — write the request, read the response to EOF — which is what
//! `hyprctl` itself does and what the compositor expects. The wire format is a line-oriented
//! text protocol (`contracts/compositor-ipc.md`, research.md R2), so encoding and response
//! classification are plain string work and are unit-tested as such.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::model::{Monitor, Window, Workspace};

/// Environment variable that makes a nominated batch step fail.
///
/// The one documented substitution reserved for the E2E rollback tests (research.md R14): a
/// genuine dispatcher failure cannot be provoked from outside the compositor, so it is injected.
/// Unset — which is always the case in normal use — this costs one environment read at start-up.
pub const FAULT_INJECTION_VAR: &str = "HYPR_SWAP_E2E_FAIL_BATCH_STEP";

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

/// A connection factory for the request socket. Holds no socket of its own: each request opens
/// and closes one, so a compositor restart cannot leave a stale handle behind.
#[derive(Debug, Clone)]
pub struct Ipc {
    socket: PathBuf,
    fail_step: Option<usize>,
}

impl Ipc {
    /// Locate the request socket for a compositor instance.
    #[must_use]
    pub fn new(runtime_dir: &Path, signature: &str) -> Self {
        Self {
            socket: request_socket_path(runtime_dir, signature),
            fail_step: injected_fault(),
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
        let (sent, injected) = match self.fail_step {
            Some(step) if step >= 1 && step <= commands.len() => {
                let mut kept = commands.to_vec();
                let dropped = kept.remove(step - 1);
                (kept, Some((step, dropped)))
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

fn injected_fault() -> Option<usize> {
    std::env::var(FAULT_INJECTION_VAR).ok()?.parse().ok()
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
}
