//! `foot` toplevels with known titles and geometry — the documented substitute for arbitrary
//! user applications (research.md R14).
//!
//! `foot` is used because it starts fast, takes its title from the command line, and is already
//! a dependency of the developer's session. The tests assert on the geometry the compositor
//! reports for these windows, never on their pixels.

use std::process::{Child, Command, Stdio};

use super::harness::Nested;

/// A spawned test window. Killed when dropped, so a panicking test leaves nothing behind.
pub struct Client {
    child: Child,
    pub title: String,
    pub address: String,
}

impl Client {
    /// The workspace this window is currently on.
    #[must_use]
    pub fn workspace(&self, nested: &Nested) -> Option<i32> {
        nested
            .clients()
            .into_iter()
            .find(|window| window.address == self.address)
            .map(|window| window.workspace)
    }

    /// Whether the compositor still knows about this window (FR-012: a swap loses nothing).
    #[must_use]
    pub fn is_open(&self, nested: &Nested) -> bool {
        nested
            .clients()
            .iter()
            .any(|window| window.address == self.address)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn a `foot` window with a known title on the currently active workspace, and wait until
/// the compositor reports it.
///
/// # Panics
/// If `foot` is not installed, or the window never appears.
#[must_use]
pub fn spawn(nested: &Nested, title: &str) -> Client {
    spawn_as(nested, None, title)
}

/// The same, with a chosen window class — the identity icon resolution is keyed on (FR-040,
/// research.md R21).
///
/// `foot`'s `--app-id` becomes the toplevel's `app_id`, which is what Hyprland reports as a
/// window's `class`, so this is how a test stands in for "a window of program X" without
/// installing program X. `None` leaves `foot`'s own class, for the tests that do not care.
///
/// # Panics
/// If `foot` is not installed, or the window never appears.
#[must_use]
pub fn spawn_as(nested: &Nested, class: Option<&str>, title: &str) -> Client {
    let mut command = Command::new("foot");
    nested.env(&mut command);
    if let Some(class) = class {
        command.arg("--app-id").arg(class);
    }
    let child = command
        .arg("--title")
        .arg(title)
        .arg("--")
        .arg("sh")
        .arg("-c")
        // Sleeps rather than exits, so the window lives for the whole scenario.
        .arg("exec sleep 3600")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("foot is installed");

    nested.wait_until(&format!("the window titled {title:?} appears"), || {
        nested
            .clients()
            .iter()
            .any(|window| window.title == title && window.mapped)
    });

    let address = nested
        .clients()
        .into_iter()
        .find(|window| window.title == title)
        .map(|window| window.address)
        .unwrap_or_default();

    Client {
        child,
        title: title.to_owned(),
        address,
    }
}

/// Spawn a window on a named workspace, switching there first and leaving the compositor on it.
#[must_use]
pub fn spawn_on(nested: &Nested, workspace: i32, title: &str) -> Client {
    spawn_as_on(nested, None, workspace, title)
}

/// The same, with a chosen window class (FR-040).
#[must_use]
pub fn spawn_as_on(nested: &Nested, class: Option<&str>, workspace: i32, title: &str) -> Client {
    nested.dispatch(&format!("workspace {workspace}"));
    nested.wait_until("the workspace is active", || {
        nested.active_workspace() == workspace
    });
    spawn_as(nested, class, title)
}

/// The titles of the mapped windows on a workspace, in the compositor's order — what the flat
/// list presentation shows (FR-014).
#[must_use]
pub fn titles_on(nested: &Nested, workspace: i32) -> Vec<String> {
    nested
        .clients()
        .into_iter()
        .filter(|window| window.workspace == workspace && window.mapped)
        .map(|window| window.title)
        .collect()
}

/// Every window the compositor knows about, as `(address, workspace)`, for the before/after
/// comparison SC-003 and FR-012 need.
#[must_use]
pub fn inventory(nested: &Nested) -> Vec<(String, i32)> {
    let mut inventory: Vec<(String, i32)> = nested
        .clients()
        .into_iter()
        .map(|window| (window.address, window.workspace))
        .collect();
    inventory.sort();
    inventory
}
