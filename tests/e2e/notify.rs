//! A `notify-send` stub on `PATH`, recording every desktop notification the daemon raises.
//!
//! `diag.rs` delivers notifications by spawning `notify-send` (`contracts/diagnostics.md`), so
//! the externally visible fact "a notification was raised" is "that binary was executed with
//! these arguments". Putting a recording stub first on the daemon's `PATH` observes exactly that
//! without a session bus, a notification daemon, or anything else that would make the assertion
//! depend on the developer's desktop.

use std::path::PathBuf;

/// A directory holding the stub, plus the file it appends to.
pub struct NotifyLog {
    directory: PathBuf,
    log: PathBuf,
}

impl NotifyLog {
    /// Create the stub. `PATH` for the daemon comes from [`Self::path`].
    ///
    /// # Panics
    /// If the stub cannot be written or made executable.
    #[must_use]
    pub fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "hypr-swap-e2e-notify-{}-{}",
            std::process::id(),
            thread_serial()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the notify stub directory");
        let log = directory.join("raised.log");

        let script = directory.join("notify-send");
        std::fs::write(
            &script,
            // The path is quoted: a scratch directory's name is not guaranteed to be free of
            // characters the shell would otherwise read as syntax.
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\n",
                log.display()
            ),
        )
        .expect("write the notify stub");
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("make the notify stub executable");

        Self { directory, log }
    }

    /// The `PATH` a daemon is given so it finds the stub and nothing else that could deliver a
    /// notification.
    #[must_use]
    pub fn path(&self) -> String {
        self.directory.display().to_string()
    }

    /// Every notification raised so far, one per line, as the arguments the stub was given.
    #[must_use]
    pub fn raised(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Poll until at least `count` notifications have been recorded, or give up.
    ///
    /// `notify-send` is spawned **detached** and never waited on (`contracts/diagnostics.md`), so
    /// the process that raised the notification can be gone before the stub has written its line.
    #[must_use]
    pub fn wait_for(&self, count: usize) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let raised = self.raised();
            if raised.len() >= count || std::time::Instant::now() >= deadline {
                return raised;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// A `PATH` with no `notify-send` on it at all — FR-032's "no notification service".
    ///
    /// The directory exists and is empty, so the spawn fails the way it does on a system without
    /// the binary installed rather than because the path itself is nonsense.
    #[must_use]
    pub fn empty_path() -> String {
        let directory = std::env::temp_dir().join(format!(
            "hypr-swap-e2e-no-notify-{}-{}",
            std::process::id(),
            thread_serial()
        ));
        std::fs::create_dir_all(&directory).expect("create the empty PATH directory");
        directory.display().to_string()
    }
}

impl Drop for NotifyLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// A number unique to this process, so two stubs never share a directory.
fn thread_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    SERIAL.fetch_add(1, Ordering::Relaxed)
}
