//! A nested Hyprland instance: lifecycle, generated configuration, headless outputs, IPC
//! assertions and teardown (research.md R14).
//!
//! The nested instance is an ordinary Wayland client of the developer's session with its own
//! `HYPRLAND_INSTANCE_SIGNATURE` and its own IPC sockets, so the suite is safe to run repeatedly
//! and never touches the workspaces the developer is actually using.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use hypr_swap::hypr::ipc::Ipc;
use hypr_swap::model::{Monitor, Window, Workspace};
use hypr_swap::ui::shortcuts::Shortcut;

/// How long any `wait_until` will keep asking before giving up.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// How often it asks.
const POLL: Duration = Duration::from_millis(50);
/// A nested compositor takes a moment to come up; well under the timeout in practice.
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// Only one nested instance at a time: they compete for `wayland-N` sockets and for the
/// developer's GPU, and a test that saw another test's compositor would be a mystery to debug.
fn instance_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What the generated `hyprland.conf` should contain beyond the defaults.
#[derive(Debug, Default, Clone)]
pub struct Setup {
    /// Extra `hyprland.conf` lines, appended verbatim.
    pub compositor_config: String,
    /// Contents of the application's own `config.toml`, or `None` to run with no file at all
    /// (FR-023).
    pub app_config: Option<String>,
    /// Which of the two shortcuts get bind lines. Empty exercises FR-022b.
    pub binds: Vec<Shortcut>,
}

impl Setup {
    /// The ordinary case: both documented bind lines, no application configuration file.
    #[must_use]
    pub fn documented() -> Self {
        Self {
            binds: Shortcut::ALL.to_vec(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_app_config(mut self, toml: &str) -> Self {
        self.app_config = Some(toml.to_owned());
        self
    }

    #[must_use]
    pub fn with_compositor_config(mut self, lines: &str) -> Self {
        self.compositor_config.clear();
        self.compositor_config.push_str(lines);
        self
    }

    #[must_use]
    pub fn with_binds(mut self, binds: &[Shortcut]) -> Self {
        self.binds = binds.to_vec();
        self
    }
}

/// One mapped `hypr-swap` layer surface, as the compositor reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySurface {
    pub monitor: String,
    /// `wlr-layer-shell` stacking level: 3 is the overlay layer, which is what puts it above a
    /// fullscreen client (FR-018).
    pub level: u32,
    pub position: (i32, i32),
    pub size: (u32, u32),
}

/// The `wlr-layer-shell` level the overlay must occupy (FR-018).
pub const OVERLAY_LEVEL: u32 = 3;

/// What every monitor is showing, and which one has keyboard focus — everything a swap changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// `(monitor, active workspace)`, sorted by monitor so two layouts compare directly.
    pub monitors: Vec<(String, i32)>,
    pub focused: Option<String>,
}

impl Layout {
    /// The workspace a named monitor is showing.
    #[must_use]
    pub fn on(&self, monitor: &str) -> Option<i32> {
        self.monitors
            .iter()
            .find(|(name, _)| name == monitor)
            .map(|(_, workspace)| *workspace)
    }
}

/// A background watcher recording every distinct layout the compositor passed through.
pub struct Sampler {
    running: Arc<AtomicBool>,
    seen: Arc<Mutex<Vec<Layout>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Sampler {
    /// Stop watching and return the layouts seen, in order and without repeats.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn stop(mut self) -> Vec<Layout> {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.clone()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A running nested compositor. Killed when dropped, so a panicking test cannot leak one.
pub struct Nested {
    child: Child,
    directory: PathBuf,
    pub wayland_display: String,
    pub signature: String,
    pub ipc: Ipc,
    /// A stable view of this instance's sockets, so a daemon can outlive a restart of it — see
    /// [`Nested::stable_env`].
    stable: Stable,
    _lock: MutexGuard<'static, ()>,
}

/// Fixed socket locations that follow whichever nested compositor is currently running.
///
/// A real Hyprland picks a fresh `HYPRLAND_INSTANCE_SIGNATURE` and a fresh `wayland-N` socket
/// every time it starts, and the daemon reads both once, at start-up. A daemon pointed straight
/// at them could therefore never reconnect to a *restarted* compositor — not because FR-026b does
/// not work, but because it would be looking in the wrong place. These three names are symlinks
/// the harness re-points across a restart, standing in for the one thing a user's session keeps
/// stable across a compositor crash: where its sockets live. Everything the daemon then does
/// through them — Wayland, IPC, shortcut registration — is the real interface, unaltered.
struct Stable {
    runtime: PathBuf,
    signature: String,
    display: String,
}

impl Nested {
    /// Start a nested Hyprland carrying the documented bind lines.
    #[must_use]
    pub fn start() -> Self {
        Self::start_with(&Setup::documented())
    }

    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn start_with(setup: &Setup) -> Self {
        let lock = instance_lock();
        let directory = scratch_directory();
        let config = directory.join("hyprland.conf");
        std::fs::write(&config, generate_config(setup)).expect("write the nested configuration");

        // The application's own configuration, at the location it looks in — `XDG_CONFIG_HOME` is
        // pointed at this directory by `env`, so the daemon finds it exactly as a user's would.
        // Absent `app_config` leaves no file at all, which is the FR-023 default-everything case.
        if let Some(toml) = &setup.app_config {
            let app_config = directory.join("config").join(hypr_swap::APP_ID);
            std::fs::create_dir_all(&app_config).expect("create the application config directory");
            std::fs::write(app_config.join("config.toml"), toml)
                .expect("write the application configuration");
        }

        let runtime = runtime_dir();
        let (child, instance) = spawn_compositor(&directory, &config, &runtime);
        let Instance {
            signature,
            wayland_display,
        } = instance;

        let ipc = Ipc::new(&runtime, &signature);
        let stable = Stable {
            runtime: directory.join("runtime"),
            signature: "hypr-swap-e2e-stable".to_owned(),
            display: "wayland-stable".to_owned(),
        };
        let nested = Self {
            child,
            directory,
            wayland_display,
            signature,
            ipc,
            stable,
            _lock: lock,
        };
        nested.link_stable();
        // The compositor answers its socket a moment before it has finished setting up outputs.
        nested.wait_until("the nested compositor reports a monitor", || {
            !nested.monitors().is_empty()
        });
        nested
    }

    /// Environment a child process needs in order to talk to this instance rather than the host.
    pub fn env(&self, command: &mut Command) {
        command
            .env("WAYLAND_DISPLAY", &self.wayland_display)
            .env("HYPRLAND_INSTANCE_SIGNATURE", &self.signature)
            .env("XDG_CONFIG_HOME", self.directory.join("config"));
    }

    /// The same, but through the stable socket names — the environment a daemon that has to
    /// survive a restart of this compositor is given.
    pub fn stable_env(&self, command: &mut Command) {
        command
            .env("XDG_RUNTIME_DIR", &self.stable.runtime)
            .env("WAYLAND_DISPLAY", &self.stable.display)
            .env("HYPRLAND_INSTANCE_SIGNATURE", &self.stable.signature)
            .env("XDG_CONFIG_HOME", self.directory.join("config"));
    }

    /// Point the stable names at whichever instance is running now.
    #[allow(clippy::missing_panics_doc)]
    fn link_stable(&self) {
        let runtime = runtime_dir();
        std::fs::create_dir_all(self.stable.runtime.join("hypr"))
            .expect("create the stable runtime directory");
        // A runtime directory is expected to be private to its owner; libwayland complains
        // otherwise.
        let _ = std::fs::set_permissions(
            &self.stable.runtime,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        );
        relink(
            &runtime.join("hypr").join(&self.signature),
            &self
                .stable
                .runtime
                .join("hypr")
                .join(&self.stable.signature),
        );
        relink(
            &runtime.join(&self.wayland_display),
            &self.stable.runtime.join(&self.stable.display),
        );
    }

    /// Take the Hyprland IPC sockets away from a daemon using the stable names, without touching
    /// the compositor itself.
    ///
    /// This is the one disconnection that can be staged while the compositor is still there to
    /// press keys against, which is what FR-026d needs: the daemon tears its whole client down —
    /// surfaces, shortcuts and all — and the user's bind then has nothing to deliver to.
    #[allow(clippy::missing_panics_doc)]
    pub fn sever_ipc(&self) {
        relink(
            &self.directory.join("no-such-instance"),
            &self
                .stable
                .runtime
                .join("hypr")
                .join(&self.stable.signature),
        );
    }

    /// Undo [`Self::sever_ipc`].
    pub fn restore_ipc(&self) {
        self.link_stable();
    }

    /// Kill this compositor and start a fresh one in its place, re-pointing the stable names at
    /// it — the crash-and-restart FR-026a/FR-026b exist for.
    ///
    /// Everything the old instance held is gone: its workspaces, its windows, and the `Client`
    /// handles a test is holding for them.
    #[allow(clippy::missing_panics_doc)]
    pub fn restart(&mut self) {
        self.kill();
        let runtime = runtime_dir();
        let (child, instance) = spawn_compositor(
            &self.directory,
            &self.directory.join("hyprland.conf"),
            &runtime,
        );
        self.child = child;
        self.signature = instance.signature;
        self.wayland_display = instance.wayland_display;
        self.ipc = Ipc::new(&runtime, &self.signature);
        self.link_stable();
        self.wait_until("the restarted compositor reports a monitor", || {
            !self.monitors().is_empty()
        });
    }

    /// Add a headless output — the documented substitute for a second physical monitor.
    /// Returns its connector name.
    #[allow(clippy::missing_panics_doc)]
    pub fn add_headless_output(&self) -> String {
        let before: Vec<String> = self.monitors().into_iter().map(|m| m.name).collect();
        self.hyprctl(&["output", "create", "headless"]);
        wait_for_value(TIMEOUT, || {
            self.monitors()
                .into_iter()
                .map(|m| m.name)
                .find(|name| !before.contains(name))
        })
        .unwrap_or_else(|| panic!("no headless output appeared"))
    }

    /// Run `hyprctl` against this instance. Used only for setup that has no IPC equivalent;
    /// assertions go through [`Self::ipc`].
    #[allow(clippy::missing_panics_doc)]
    pub fn hyprctl(&self, args: &[&str]) -> String {
        let mut command = Command::new("hyprctl");
        self.env(&mut command);
        let output = command.args(args).output().expect("hyprctl runs");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Dispatch a compositor command directly, e.g. to set up workspaces before a scenario.
    pub fn dispatch(&self, command: &str) {
        let _ = self.ipc.dispatch(&[command.to_owned()]);
    }

    #[must_use]
    pub fn monitors(&self) -> Vec<Monitor> {
        self.ipc.monitors().unwrap_or_default()
    }

    #[must_use]
    pub fn workspaces(&self) -> Vec<Workspace> {
        self.ipc.workspaces().unwrap_or_default()
    }

    #[must_use]
    pub fn clients(&self) -> Vec<Window> {
        self.ipc.clients().unwrap_or_default()
    }

    /// The workspace id currently shown on the focused monitor.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn active_workspace(&self) -> i32 {
        self.monitors()
            .into_iter()
            .find(|monitor| monitor.focused)
            .map_or(0, |monitor| monitor.active_workspace)
    }

    /// The workspace shown on a named monitor.
    #[must_use]
    pub fn active_workspace_on(&self, monitor: &str) -> Option<i32> {
        self.monitors()
            .into_iter()
            .find(|candidate| candidate.name == monitor)
            .map(|candidate| candidate.active_workspace)
    }

    /// The monitor a workspace is currently bound to.
    #[must_use]
    pub fn monitor_of(&self, workspace: i32) -> Option<String> {
        self.workspaces()
            .into_iter()
            .find(|candidate| candidate.id == workspace)
            .map(|candidate| candidate.monitor)
    }

    /// Whether a `hypr-swap` layer surface is mapped, and on which monitors (FR-017, FR-018).
    #[must_use]
    pub fn overlay_monitors(&self) -> Vec<String> {
        let mut found: Vec<String> = self
            .overlay_surfaces()
            .into_iter()
            .map(|surface| surface.monitor)
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// Every mapped `hypr-swap` layer surface, with the geometry and stacking level the
    /// compositor reports for it.
    ///
    /// The overlay's pixels cannot be inspected from a test — screenshot comparison is rejected
    /// in research.md R14 as brittle across fonts and scaling — so its *geometry* is what the
    /// presentation scenarios assert on, against the same `ui::layout` arithmetic the application
    /// used to ask for it (FR-018, FR-019).
    #[must_use]
    pub fn overlay_surfaces(&self) -> Vec<OverlaySurface> {
        let layers = self.hyprctl(&["-j", "layers"]);
        let parsed: serde_json::Value = serde_json::from_str(&layers).unwrap_or_default();
        let mut found = Vec::new();
        let Some(monitors) = parsed.as_object() else {
            return found;
        };

        for (monitor, entry) in monitors {
            let Some(levels) = entry.get("levels").and_then(serde_json::Value::as_object) else {
                continue;
            };
            for (level, surfaces) in levels {
                let Some(surfaces) = surfaces.as_array() else {
                    continue;
                };
                for surface in surfaces {
                    if surface.get("namespace").and_then(serde_json::Value::as_str)
                        != Some(hypr_swap::APP_ID)
                    {
                        continue;
                    }
                    let number = |key: &str| {
                        surface
                            .get(key)
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or_default()
                    };
                    found.push(OverlaySurface {
                        monitor: monitor.clone(),
                        level: level.parse().unwrap_or_default(),
                        position: (
                            i32::try_from(number("x")).unwrap_or_default(),
                            i32::try_from(number("y")).unwrap_or_default(),
                        ),
                        size: (
                            u32::try_from(number("w")).unwrap_or_default(),
                            u32::try_from(number("h")).unwrap_or_default(),
                        ),
                    });
                }
            }
        }
        found
    }

    /// The names the compositor has been told about, i.e. what `hyprctl globalshortcuts` shows.
    #[must_use]
    pub fn registered_shortcuts(&self) -> Vec<String> {
        #[derive(serde::Deserialize)]
        struct Registered {
            name: String,
        }
        let response = self.ipc.request("j/globalshortcuts").unwrap_or_default();
        let mut names: Vec<String> = serde_json::from_str::<Vec<Registered>>(&response)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        names.sort();
        names
    }

    /// Poll until `condition` holds, or fail the test with `what` in the message.
    ///
    /// Compositor state changes are asynchronous, so every assertion that follows an action goes
    /// through here rather than through a fixed sleep.
    #[allow(clippy::missing_panics_doc, clippy::unused_self)]
    pub fn wait_until(&self, what: &str, mut condition: impl FnMut() -> bool) {
        assert!(
            wait_for_value(TIMEOUT, || condition().then_some(())).is_some(),
            "timed out after {TIMEOUT:?} waiting until {what}"
        );
    }

    /// Which workspace each monitor is showing, and which monitor has focus.
    ///
    /// One value that captures everything a swap changes, so a test can compare whole states
    /// rather than a handful of fields (SC-010).
    #[must_use]
    pub fn layout(&self) -> Layout {
        let mut monitors: Vec<(String, i32)> = self
            .monitors()
            .into_iter()
            .map(|monitor| (monitor.name, monitor.active_workspace))
            .collect();
        monitors.sort();
        Layout {
            monitors,
            focused: self
                .monitors()
                .into_iter()
                .find(|monitor| monitor.focused)
                .map(|monitor| monitor.name),
        }
    }

    /// Watch the layout continuously in the background until the returned sampler is stopped.
    ///
    /// SC-010 says no half-swapped state is ever observable, which is a claim about the states
    /// that exist *during* the change — the only way to test it from outside is to keep looking.
    #[must_use]
    pub fn sample_layout(&self) -> Sampler {
        let ipc = self.ipc.clone();
        let running = Arc::new(AtomicBool::new(true));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let thread = std::thread::spawn({
            let running = Arc::clone(&running);
            let seen = Arc::clone(&seen);
            move || {
                while running.load(Ordering::Relaxed) {
                    let mut monitors: Vec<(String, i32)> = ipc
                        .monitors()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|monitor| (monitor.name, monitor.active_workspace))
                        .collect();
                    monitors.sort();
                    let focused = ipc
                        .monitors()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|monitor| monitor.focused)
                        .map(|monitor| monitor.name);
                    let layout = Layout { monitors, focused };
                    let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
                    if seen.last() != Some(&layout) {
                        seen.push(layout);
                    }
                }
            }
        });
        Sampler {
            running,
            seen,
            thread: Some(thread),
        }
    }

    /// Start the application under test against this instance.
    #[must_use]
    pub fn start_daemon(&self) -> Daemon {
        self.start_daemon_with(&[])
    }

    #[must_use]
    pub fn start_daemon_with(&self, args: &[&str]) -> Daemon {
        self.start_daemon_with_env(args, &[])
    }

    /// Start the daemon with extra environment — the fault-injection hook and nothing else
    /// (research.md R14).
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn start_daemon_with_env(&self, args: &[&str], environment: &[(&str, &str)]) -> Daemon {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hypr-swap"));
        self.env(&mut command);
        for (name, value) in environment {
            command.env(name, value);
        }
        let child = command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the application under test is built");

        let daemon = Daemon { child };
        // The shortcuts appearing is the compositor's own confirmation that the daemon is up.
        self.wait_until("the daemon registers its shortcuts", || {
            !self.registered_shortcuts().is_empty()
        });
        daemon
    }

    /// Start the application under test against the stable socket names, so it can outlive a
    /// [`Self::restart`] or a [`Self::sever_ipc`].
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn start_daemon_stable(&self, environment: &[(&str, &str)]) -> Daemon {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hypr-swap"));
        self.stable_env(&mut command);
        for (name, value) in environment {
            command.env(name, value);
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the application under test is built");
        let daemon = Daemon { child };
        self.wait_until("the daemon registers its shortcuts", || {
            !self.registered_shortcuts().is_empty()
        });
        daemon
    }

    /// Kill the compositor from underneath the application, for the reconnection tests. Abrupt
    /// on purpose: this is the crash FR-026a exists for.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Everything the nested compositor logged, for a failing test's diagnosis.
    #[must_use]
    pub fn log(&self) -> String {
        read_log(&self.directory)
    }
}

impl Drop for Nested {
    fn drop(&mut self) {
        // SIGTERM first: a compositor killed outright leaves its wayland socket on disk for the
        // next instance to reuse, which is exactly the confusion this harness avoids.
        terminate(&self.child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(POLL),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The application under test. Killed when dropped.
pub struct Daemon {
    child: Child,
}

impl Daemon {
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the process is still running — a lost connection must never end it (FR-025).
    #[allow(clippy::missing_panics_doc)]
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Total CPU time the daemon has used, in clock ticks, from `/proc/<pid>/stat`.
    ///
    /// The only externally visible difference between "waiting out a backoff delay" and "retrying
    /// in a hot loop" is this number, which is what FR-026d's "MUST NOT consume resources by
    /// retrying without delay" comes down to.
    #[must_use]
    pub fn cpu_ticks(&self) -> u64 {
        let stat =
            std::fs::read_to_string(format!("/proc/{}/stat", self.child.id())).unwrap_or_default();
        // Fields 14 and 15 (1-based) are utime and stime; the comm field may contain spaces and
        // is bracketed, so counting starts after its closing parenthesis.
        let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
            return 0;
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let at = |index: usize| fields.get(index).and_then(|f| f.parse::<u64>().ok());
        at(11).unwrap_or_default() + at(12).unwrap_or_default()
    }

    /// Stop the daemon the way a session manager would, and return its exit code.
    #[allow(clippy::missing_panics_doc)]
    pub fn terminate(mut self) -> Option<i32> {
        terminate(&self.child);
        self.child.wait().ok().and_then(|status| status.code())
    }

    /// Everything the daemon wrote to stderr, which is its complete diagnostic record
    /// (`contracts/diagnostics.md`).
    #[allow(clippy::missing_panics_doc)]
    pub fn stderr(mut self) -> String {
        use std::io::Read;
        terminate(&self.child);
        let _ = self.child.wait();
        let mut text = String::new();
        if let Some(mut stream) = self.child.stderr.take() {
            let _ = stream.read_to_string(&mut text);
        }
        text
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Send `SIGTERM`, which is what the process contract promises to handle (`contracts/cli.md`).
fn terminate(child: &Child) {
    // `Command` offers no signal API, so this goes through `kill(1)` rather than a libc
    // dependency the application itself does not need.
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The `hyprland.conf` the nested instance runs, carrying the exact bind lines the application
/// documents (FR-022b) so the suite proves the documentation rather than a private arrangement.
fn generate_config(setup: &Setup) -> String {
    let mut config = String::from(
        "# Generated by the hypr-swap E2E harness.\n\
         monitor = WAYLAND-1, 1920x1080@60, 0x0, 1\n\
         misc {\n\
         \x20   disable_hyprland_logo = true\n\
         \x20   disable_splash_rendering = true\n\
         \x20   force_default_wallpaper = 0\n\
         \x20   disable_autoreload = true\n\
         }\n\
         animations { enabled = false }\n\
         decoration { blur { enabled = false } }\n\
         debug { disable_logs = false }\n",
    );
    for shortcut in &setup.binds {
        let _ = writeln!(config, "{}", shortcut.suggested_bind());
    }
    config.push_str(&setup.compositor_config);
    config.push('\n');
    config
}

/// Start one nested Hyprland against `config` and wait until it has registered its instance.
///
/// Each instance writes its pid and its wayland socket to `hyprland.lock`, so the instance is
/// identified by the pid this harness itself spawned. Diffing socket names instead is unreliable:
/// a compositor that was killed leaves its socket behind for the next one to reuse
/// (research.md R14).
fn spawn_compositor(directory: &Path, config: &Path, runtime: &Path) -> (Child, Instance) {
    // The nested instance must not inherit the host's signature, or `hyprctl` inside it would
    // address the developer's session.
    let child = Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .current_dir(directory)
        .stdout(log_file(directory))
        .stderr(log_file(directory))
        .spawn()
        .expect("Hyprland is installed and on PATH");

    let pid = child.id();
    let instance =
        wait_for_value(START_TIMEOUT, || find_instance(runtime, pid)).unwrap_or_else(|| {
            panic!(
                "the nested compositor never registered an instance; log:\n{}",
                read_log(directory)
            )
        });
    (child, instance)
}

/// Replace `link` with a symlink to `target`, creating its parent if need be.
fn relink(target: &Path, link: &Path) {
    if let Some(parent) = link.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link).expect("create the stable symlink");
}

fn scratch_directory() -> PathBuf {
    let unique = format!(
        "hypr-swap-e2e-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos() ^ next_serial()
    );
    let directory = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&directory).expect("create the scratch directory");
    directory
}

fn next_serial() -> u128 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    u128::from(SERIAL.fetch_add(1, Ordering::Relaxed))
}

fn log_file(directory: &Path) -> Stdio {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("hyprland.log"))
        .map_or_else(|_| Stdio::null(), Stdio::from)
}

fn runtime_dir() -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is set"))
}

/// A compositor instance as it identifies itself in `$XDG_RUNTIME_DIR/hypr/<signature>/`.
struct Instance {
    signature: String,
    wayland_display: String,
}

/// Find the instance belonging to `pid`, once it has opened its IPC socket.
///
/// `hyprland.lock` holds the pid on its first line and the wayland socket name on its second.
fn find_instance(runtime: &Path, pid: u32) -> Option<Instance> {
    for entry in std::fs::read_dir(runtime.join("hypr")).ok()?.flatten() {
        let directory = entry.path();
        if !directory.join(".socket.sock").exists() {
            continue;
        }
        let Ok(lock) = std::fs::read_to_string(directory.join("hyprland.lock")) else {
            continue;
        };
        let mut lines = lock.lines();
        let owner: u32 = lines.next().and_then(|line| line.trim().parse().ok())?;
        if owner != pid {
            continue;
        }
        let wayland_display = lines.next()?.trim().to_owned();
        if wayland_display.is_empty() {
            continue;
        }
        return Some(Instance {
            signature: entry.file_name().into_string().ok()?,
            wayland_display,
        });
    }
    None
}

fn read_log(directory: &Path) -> String {
    std::fs::read_to_string(directory.join("hyprland.log")).unwrap_or_default()
}

/// Poll `probe` until it yields a value or the deadline passes.
fn wait_for_value<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(POLL);
    }
}
