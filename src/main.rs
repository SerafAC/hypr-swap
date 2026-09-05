//! `hypr-swap` — start-up, wiring, the event loop, and reconnection.
//!
//! The binary is the daemon: there are no subcommands, and every user-facing action arrives
//! through a shortcut the user bound in their own `hyprland.conf` (`contracts/cli.md`, FR-022).
//!
//! Losing the compositor mid-session is not fatal (FR-026a). The Wayland connection dies with the
//! compositor, so a lost connection is handled by tearing the whole client down and building a
//! fresh one after a backoff delay — which is also what gives FR-026b (re-registered shortcuts),
//! FR-026c (cleared history) and FR-026d (no overlay while disconnected) with no extra machinery.

use std::cell::Cell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::signals::{Signal, Signals};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, Mode, PostAction};
use calloop_wayland_source::WaylandSource;

use hypr_swap::actions;
use hypr_swap::config::{self, Configuration, LoadError};
use hypr_swap::diag::{self, Condition};
use hypr_swap::hypr::events::{Backoff, Disconnected, EventStream};
use hypr_swap::hypr::ipc::{DispatchOutcome, Ipc, IpcError};
use hypr_swap::icons;
use hypr_swap::model::Support;
use hypr_swap::session;
use hypr_swap::state::{Applied, World};
use hypr_swap::ui::shortcuts::Shortcut;
use hypr_swap::ui::{self, App, Request, StartupError};
use hypr_swap::{APP_ID, SUPPORTED_HYPRLAND, version};

/// Clean shutdown, `--version`, or `--help`.
const EXIT_OK: u8 = 0;
/// Invalid command line, or `--config` naming a file that does not exist.
const EXIT_USAGE: u8 = 2;
/// Cannot reach the compositor at start-up (FR-025), or a second instance (FR-025a).
const EXIT_NO_COMPOSITOR: u8 = 3;

/// Why the process is stopping (FR-113).
///
/// Every path out of [`execute`] produces one, so "no daemon run ends without a record of why"
/// (SC-042) is a property of the type rather than of remembering to report at each `return`. The
/// exit code travels with the cause for the same reason: the two cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stopping {
    /// A termination signal, named as the user's session manager sent it.
    Signal(&'static str),
    /// The compositor could not be reached at start-up (FR-025).
    NoCompositor,
    /// Another `hypr-swap` already holds the shortcuts (FR-025a).
    SecondInstance,
    /// The command line, or the `--config` path on it, could not be used (FR-033, FR-034).
    UsageError,
}

impl Stopping {
    /// The cause, spelled as `contracts/diagnostics.md` publishes it.
    fn cause(self) -> &'static str {
        match self {
            Self::Signal(name) => name,
            Self::NoCompositor => "cannot reach the compositor at start-up",
            Self::SecondInstance => "another hypr-swap is already running",
            Self::UsageError => "usage error",
        }
    }

    fn exit_code(self) -> u8 {
        match self {
            Self::Signal(_) => EXIT_OK,
            Self::NoCompositor | Self::SecondInstance => EXIT_NO_COMPOSITOR,
            Self::UsageError => EXIT_USAGE,
        }
    }
}

fn main() -> ExitCode {
    // `--version`, `--help` and `--environment` print an answer and exit; they are not a daemon
    // run, so there is no stopping to record (`contracts/diagnostics.md`).
    let Some(stopping) = execute() else {
        return ExitCode::from(EXIT_OK);
    };

    // FR-113: the last line the process writes, after whatever was reported at `Error` level
    // above it, and before the exit code is returned.
    diag::report(
        Condition::Stopping,
        "daemon",
        &format!("stopping: {}", stopping.cause()),
    );
    ExitCode::from(stopping.exit_code())
}

/// Everything between the command line and the exit code. `None` is an option that printed its
/// answer.
fn execute() -> Option<Stopping> {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            diag::report(Condition::UsageError, "usage", &message);
            eprintln!("{}", usage());
            return Some(Stopping::UsageError);
        }
    };

    match options.print {
        Some(Print::Version) => {
            println!("{APP_ID} {}", version());
            return None;
        }
        Some(Print::Help) => {
            println!("{}", usage());
            return None;
        }
        // `--environment` needs the configuration it is going to report on, so it is answered
        // below rather than here.
        Some(Print::Environment) | None => {}
    }

    let configuration = match config::load(options.config.as_deref()) {
        Ok(configuration) => configuration,
        Err(e @ (LoadError::NotFound(_) | LoadError::Unreadable(..))) => {
            // FR-034: a file named explicitly and not found is an error, unlike the default
            // location where absence is normal.
            diag::report(Condition::InvalidConfigValue, "config", &e.to_string());
            return Some(Stopping::UsageError);
        }
    };

    if options.print == Some(Print::Environment) {
        print!("{}", environment_report(&options, &configuration));
        return None;
    }

    Some(run(&configuration))
}

/// Start up, then serve until a signal arrives, reconnecting across compositor restarts.
fn run(configuration: &Configuration) -> Stopping {
    let environment = match Environment::read() {
        Ok(environment) => environment,
        Err(missing) => {
            diag::report(
                Condition::CompositorUnreachableAtStartup,
                "compositor",
                &format!("cannot connect at start-up: no {missing} in environment"),
            );
            return Stopping::NoCompositor;
        }
    };

    let ipc = Ipc::new(&environment.runtime_dir, &environment.signature);
    if !ipc.socket_path().exists() {
        diag::report(
            Condition::CompositorUnreachableAtStartup,
            "compositor",
            &format!(
                "cannot connect at start-up: {} is absent",
                ipc.socket_path().display()
            ),
        );
        return Stopping::NoCompositor;
    }

    // FR-118: asked once, before anything else can fail obscurely because of it.
    report_compositor_version(&ipc);

    // FR-025a: refuse to run as a second instance competing for the same shortcut names.
    if let Some(name) = already_registered(&ipc) {
        diag::report(
            Condition::SecondInstance,
            "shortcut",
            &format!("{name} is already registered; another {APP_ID} is running"),
        );
        return Stopping::SecondInstance;
    }

    let mut backoff = Backoff::new();
    let mut first_attempt = true;

    loop {
        match serve(configuration, &environment, &ipc, first_attempt) {
            // A signal asked us to stop.
            Ok(Outcome::Terminated(signal)) => return Stopping::Signal(signal),
            Ok(Outcome::Disconnected) => {}
            Err(stopping) => return stopping,
        }
        first_attempt = false;

        // FR-026a/FR-026d: retry with increasing delay, indefinitely, never in a hot loop.
        let delay = backoff.take();
        diag::report(
            Condition::CompositorConnection,
            "compositor",
            &format!(
                "connection lost, reconnecting (next attempt in {}ms)",
                delay.as_millis()
            ),
        );
        if let Some(signal) = wait_or_terminate(delay) {
            return Stopping::Signal(signal);
        }
        // A successful `serve` resets the delay, so a compositor that restarts twice is retried
        // as briskly the second time as the first.
        if ipc.socket_path().exists() {
            backoff.reset();
        }
    }
}

/// Name the compositor's version when it is one this release does not claim to support (FR-118).
///
/// Never fatal, and never reported more than once: this runs at start-up only, and a reconnection
/// does not ask again. A compositor that cannot be asked at all says nothing here — that is the
/// `CompositorUnreachableAtStartup` path's business, and it is about to run.
fn report_compositor_version(ipc: &Ipc) {
    let Ok(version) = ipc.version() else {
        return;
    };
    let message = match version.supported() {
        Support::Supported => return,
        Support::TooOld {
            found: (major, minor, patch),
            ..
        } => format!(
            "Hyprland {major}.{minor}.{patch} is below the supported range \
             ({SUPPORTED_HYPRLAND}); continuing anyway"
        ),
        Support::Unknown { found } => format!(
            "Hyprland version {found:?} could not be read; supported range is \
             {SUPPORTED_HYPRLAND}"
        ),
    };
    diag::report(
        Condition::CompositorVersionUnsupported,
        "compositor",
        &message,
    );
}

/// One connected lifetime: build the client, serve until the connection drops or a signal
/// arrives, then tear everything down.
fn serve(
    configuration: &Configuration,
    environment: &Environment,
    ipc: &Ipc,
    fatal_if_unavailable: bool,
) -> Result<Outcome, Stopping> {
    // FR-026b/FR-026c: the world is rebuilt from scratch and the history starts empty, because
    // activations missed while disconnected would leave a confidently wrong order.
    let world = match build_world(ipc) {
        Ok(world) => world,
        Err(e) if fatal_if_unavailable => {
            diag::report(
                Condition::CompositorUnreachableAtStartup,
                "compositor",
                &format!("cannot connect at start-up: {e}"),
            );
            return Err(Stopping::NoCompositor);
        }
        Err(_) => return Ok(Outcome::Disconnected),
    };

    // Cloned per connected lifetime rather than re-read: the file is read exactly once, at
    // start-up, and a reconnection must not pick up an edit the user made since (FR-060).
    let (wayland, mut app) = match ui::connect(configuration.clone(), world) {
        Ok(client) => client,
        Err(e) if fatal_if_unavailable => {
            diag::report(
                Condition::CompositorUnreachableAtStartup,
                "compositor",
                &format!("cannot connect at start-up: {e}"),
            );
            return Err(Stopping::NoCompositor);
        }
        Err(StartupError::MissingGlobal(global, _)) => {
            // A compositor that came back without a protocol we need is not something a retry
            // will fix, but it is also not a reason to lose the user's daemon; report and retry.
            diag::report(
                Condition::CompositorConnection,
                "compositor",
                &format!("reconnected without {global}, retrying"),
            );
            return Ok(Outcome::Disconnected);
        }
        Err(_) => return Ok(Outcome::Disconnected),
    };

    let events = match EventStream::connect(&hypr_swap::hypr::ipc::event_socket_path(
        &environment.runtime_dir,
        &environment.signature,
    )) {
        Ok(events) => events,
        Err(e) if fatal_if_unavailable => {
            diag::report(
                Condition::CompositorUnreachableAtStartup,
                "compositor",
                &format!("cannot connect at start-up: event socket: {e}"),
            );
            return Err(Stopping::NoCompositor);
        }
        Err(_) => return Ok(Outcome::Disconnected),
    };

    if fatal_if_unavailable {
        // FR-112: the world, the Wayland client and the event stream are all up, which is the
        // point at which the daemon can actually serve a shortcut. Reported here, and only on the
        // first connected lifetime — a reconnection has its own record below.
        diag::report(
            Condition::Started,
            "daemon",
            &format!("{APP_ID} {} started", version()),
        );
    } else {
        // FR-031: recovery is reported on stderr only, never as a notification.
        diag::report(
            Condition::CompositorConnection,
            "compositor",
            "reconnected, state rebuilt, shortcuts re-registered",
        );
    }

    event_loop(ipc, &mut app, wayland, events)
}

/// The single event loop, over the Wayland connection, the Hyprland event socket, and signals.
/// No polling: the process consumes no CPU while the overlay is closed.
fn event_loop(
    ipc: &Ipc,
    app: &mut App,
    wayland: ui::Wayland,
    events: EventStream,
) -> Result<Outcome, Stopping> {
    let mut event_loop: EventLoop<'static, App> =
        EventLoop::try_new().map_err(|_| Stopping::NoCompositor)?;
    let handle = event_loop.handle();
    let stop = event_loop.get_signal();

    // Shared with the callbacks because the loop data is `App` — the Wayland queue's own state
    // type — and these two facts belong to the loop rather than to the client.
    let outcome = Rc::new(Cell::new(Outcome::Disconnected));

    // SIGTERM/SIGINT close any overlay without committing and exit 0 (contracts/cli.md). The
    // overlay closes because every surface is dropped with `app` when this function returns.
    let signals =
        Signals::new(&[Signal::SIGTERM, Signal::SIGINT]).map_err(|_| Stopping::NoCompositor)?;
    handle
        .insert_source(signals, {
            let outcome = Rc::clone(&outcome);
            let stop = stop.clone();
            // FR-113: which signal it was, so the stopping record names the cause rather than
            // saying only that one arrived.
            move |event, (), _: &mut App| {
                outcome.set(Outcome::Terminated(signal_name(event.signal())));
                stop.stop();
            }
        })
        .map_err(|_| Stopping::NoCompositor)?;

    // The Hyprland event socket keeps the world current (FR-026) and is the only thing that
    // feeds the activation history (FR-008c).
    handle
        .insert_source(Generic::new(events, Interest::READ, Mode::Level), {
            let ipc = ipc.clone();
            let stop = stop.clone();
            move |_, stream, app: &mut App| {
                // SAFETY: `drain` only reads from the socket. `NoIoDrop` exists to stop the
                // wrapped file being dropped and its descriptor closed, which never happens
                // here — the stream outlives the source.
                let stream = unsafe { stream.get_mut() };
                match stream.drain() {
                    Ok(events) => {
                        let mut rebuild = false;
                        for event in &events {
                            rebuild |= app.world.apply(event) == Applied::ByRebuilding;
                        }
                        if rebuild {
                            // A failed rebuild means the compositor is going away; the socket
                            // will report it on the next read.
                            if refresh(&ipc, &mut app.world).is_err() {
                                stop.stop();
                            } else {
                                // The path a newly-opened window arrives on, and so the one place
                                // an icon can be resolved before any overlay could draw it
                                // (FR-043, research.md R27). Classes already cached cost a hash
                                // lookup each, which is why this can sit on a hot event.
                                app.ensure_icons();
                            }
                        }
                    }
                    Err(Disconnected) => stop.stop(),
                }
                Ok(PostAction::Continue)
            }
        })
        .map_err(|_| Stopping::NoCompositor)?;

    let connection = wayland.connection.clone();
    WaylandSource::new(wayland.connection.clone(), wayland.queue)
        .insert(handle.clone())
        .map_err(|_| Stopping::NoCompositor)?;

    let ipc = ipc.clone();
    let run = event_loop.run(None, app, move |app| {
        // Everything the Wayland shell asked for, now that it is safe to do I/O.
        for request in std::mem::take(&mut app.outbox) {
            handle_request(&ipc, app, &request);
        }
    });

    // A protocol error is how the compositor refuses a request. The one this application can
    // provoke is a duplicate shortcut registration, which the user has to act on (FR-030).
    if let Some(error) = connection.protocol_error() {
        if error.object_interface.contains("global_shortcut") {
            diag::report(
                Condition::ShortcutRegistrationFailed,
                "shortcut",
                &format!(
                    "the compositor refused a registration: {} (object {})",
                    error.message, error.object_interface
                ),
            );
        } else {
            diag::report(
                Condition::CompositorConnection,
                "compositor",
                &format!(
                    "protocol error on {}: {}",
                    error.object_interface, error.message
                ),
            );
        }
    }

    match run {
        Ok(()) => Ok(outcome.get()),
        // A loop that cannot run is not something a reconnect fixes at start-up, but mid-session
        // it is indistinguishable from the compositor going away, so treat it as a disconnect.
        Err(_) => Ok(Outcome::Disconnected),
    }
}

/// Act on one thing the Wayland shell recorded.
///
/// This is the only place a session's outcome turns into compositor traffic, which is what keeps
/// the Wayland event handlers free of I/O.
fn handle_request(ipc: &Ipc, app: &mut App, request: &Request) {
    match request {
        Request::SwitcherPressed => app.switcher_pressed(),
        Request::SwitcherReleased => app.switcher_released(),
        Request::SessionEnded => commit_session(ipc, app),
        Request::NewWorkspace => new_workspace(ipc, app),
    }
}

/// Switch to a new empty workspace on the focused monitor (FR-020, FR-021).
///
/// No overlay, no session, and nothing to do on the shortcut's `released` event — the Wayland
/// shell already drops that one (`contracts/shortcuts.md`). Everything this shortcut decides
/// lives in `actions::new_workspace_plan`; the only thing left here is the dispatch.
fn new_workspace(ipc: &Ipc, app: &App) {
    // `None` is FR-021's no-op: the focused monitor is already showing an empty workspace, so
    // there is deliberately no dispatch and no diagnostic.
    let Some(plan) = actions::new_workspace_plan(&app.world) else {
        return;
    };

    // One command cannot half-apply, so there is no half-applied state to verify against or roll
    // back from, and `contracts/diagnostics.md` names no condition for this shortcut: a dispatch
    // the compositor refuses leaves the session exactly as it was, which is what the user sees.
    let _ = ipc.dispatch(&plan.commands);
}

/// Turn a finished session into workspace changes (FR-005, FR-009, FR-011, FR-027).
///
/// The selection is resolved against the world as it is **now**, not as it was when the overlay
/// opened: a workspace destroyed in the meantime cancels the commit, while one whose monitor has
/// gone degrades to plain activation on the focused monitor. Both are FR-027, and they differ
/// because losing the target is losing the user's intent, whereas losing the monitor is not.
fn commit_session(ipc: &Ipc, app: &mut App) {
    let Some(session) = app.take_session() else {
        return;
    };
    let session::Outcome::Committed(selected) = session.outcome else {
        // Cancelled: no dispatch, and the activation history stays untouched because nothing was
        // activated (FR-006, US1-AS5).
        return;
    };

    if session.target(&app.world).is_none() {
        diag::report(
            Condition::SelectionTargetVanished,
            "selection",
            &format!("workspace {selected} no longer exists; nothing was activated"),
        );
        return;
    }

    // `None` is the FR-011 no-op: the selection is already on screen.
    let Some(plan) = actions::plan(&app.world, &session.origin_monitor, selected) else {
        return;
    };

    // A cross-monitor selection moves two workspaces and can half-apply, so it is dispatched,
    // read back and undone on mismatch (FR-013a). A same-monitor activation goes down the same
    // path: one plan type, one verification, one place to report from.
    let (subject, attempt) = if plan.is_swap() {
        (
            "swap",
            format!(
                "swapping workspace {selected} onto {}",
                session.origin_monitor
            ),
        )
    } else {
        ("activation", format!("activating workspace {selected}"))
    };

    match ipc.dispatch_verified(&plan) {
        DispatchOutcome::Verified => {}
        // FR-013b: the user asked for a change that did not happen. Their layout is intact, but
        // saying nothing would leave them believing the gesture worked.
        DispatchOutcome::RolledBack { reason } => diag::report(
            Condition::SwapRolledBack,
            subject,
            &format!("{attempt} failed, rolled back to the previous layout ({reason})"),
        ),
        // FR-013c: the layout has changed in a way nobody asked for, so the report says where
        // things actually are rather than what was attempted.
        DispatchOutcome::RollbackFailed { reason, resulting } => diag::report(
            Condition::RollbackFailed,
            subject,
            &format!("{attempt} failed; rollback failed; {resulting} ({reason})"),
        ),
    }
}

/// Where the loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// A signal asked the process to stop, named as [`signal_name`] spells it.
    Terminated(&'static str),
    /// The compositor went away; the caller reconnects.
    Disconnected,
}

/// The name a stopping record uses for a signal (`contracts/diagnostics.md`).
///
/// Only the two signals this application installs a handler for can reach it; anything else keeps
/// its default disposition and never gets here.
fn signal_name(signal: Signal) -> &'static str {
    match signal {
        Signal::SIGINT => "SIGINT",
        _ => "SIGTERM",
    }
}

/// Re-read the cached compositor view from the three `j/*` queries (FR-026b).
fn refresh(ipc: &Ipc, world: &mut World) -> Result<(), IpcError> {
    world.rebuild(ipc.monitors()?, ipc.workspaces()?, ipc.clients()?);
    Ok(())
}

fn build_world(ipc: &Ipc) -> Result<World, IpcError> {
    let mut world = World::default();
    refresh(ipc, &mut world)?;
    Ok(world)
}

/// Whether either of this application's shortcut names is already registered by someone else.
fn already_registered(ipc: &Ipc) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Registered {
        name: String,
    }
    let response = ipc.request("j/globalshortcuts").ok()?;
    let registered: Vec<Registered> = serde_json::from_str(&response).ok()?;
    Shortcut::ALL
        .into_iter()
        .map(Shortcut::qualified_name)
        .find(|name| registered.iter().any(|entry| entry.name == *name))
}

/// Wait out the backoff delay, naming the termination signal if one arrived instead.
///
/// This runs a small event loop rather than sleeping so that `SIGTERM` during a reconnect still
/// exits 0 (`contracts/cli.md`) instead of killing the process by default disposition — and, with
/// FR-113, so that it still leaves a stopping record on the way out.
fn wait_or_terminate(delay: Duration) -> Option<&'static str> {
    let Ok(mut event_loop) = EventLoop::<'static, Option<&'static str>>::try_new() else {
        std::thread::sleep(delay);
        return None;
    };
    let handle = event_loop.handle();
    let stop = event_loop.get_signal();

    let Ok(signals) = Signals::new(&[Signal::SIGTERM, Signal::SIGINT]) else {
        std::thread::sleep(delay);
        return None;
    };
    let inserted = handle.insert_source(signals, {
        let stop = stop.clone();
        move |event, (), terminated: &mut Option<&'static str>| {
            *terminated = Some(signal_name(event.signal()));
            stop.stop();
        }
    });
    let timer = handle.insert_source(
        Timer::from_duration(delay),
        move |_, (), _: &mut Option<&'static str>| {
            stop.stop();
            TimeoutAction::Drop
        },
    );
    if inserted.is_err() || timer.is_err() {
        std::thread::sleep(delay);
        return None;
    }

    let mut terminated = None;
    let _ = event_loop.run(delay, &mut terminated, |_| {});
    terminated
}

/// The environment the compositor sockets and the Wayland connection are located from.
struct Environment {
    runtime_dir: PathBuf,
    signature: String,
}

impl Environment {
    /// Reads the three required variables, naming the first that is missing
    /// (`contracts/cli.md` → Environment).
    fn read() -> Result<Self, &'static str> {
        let runtime_dir = non_empty("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR")?;
        let signature =
            non_empty("HYPRLAND_INSTANCE_SIGNATURE").ok_or("HYPRLAND_INSTANCE_SIGNATURE")?;
        non_empty("WAYLAND_DISPLAY").ok_or("WAYLAND_DISPLAY")?;
        Ok(Self {
            runtime_dir: PathBuf::from(runtime_dir),
            signature,
        })
    }
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// An option that answers a question and exits instead of starting the daemon
/// (`contracts/cli.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Print {
    Version,
    Help,
    /// The facts a bug report needs (FR-116).
    Environment,
}

impl Print {
    /// The flag as the user wrote it, for the message that refuses two of them together.
    fn flag(self) -> &'static str {
        match self {
            Self::Version => "--version",
            Self::Help => "--help",
            Self::Environment => "--environment",
        }
    }
}

/// Parsed command line.
struct Options {
    config: Option<PathBuf>,
    /// The one print-and-exit option given, if any. At most one: two of them together is a usage
    /// error (`contracts/cli.md`), because there is no sensible order to answer them in.
    print: Option<Print>,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut config = None;
        let mut print: Option<Print> = None;
        let mut args = args.peekable();

        // Nothing is printed while parsing: an option is *recorded* here and answered by the
        // caller, so that a second one can still be refused after the first has been seen.
        let once = |wanted: Print, print: &mut Option<Print>| {
            if let Some(already) = *print {
                return Err(format!(
                    "{} and {} cannot be given together",
                    already.flag(),
                    wanted.flag()
                ));
            }
            *print = Some(wanted);
            Ok(())
        };

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--version" => once(Print::Version, &mut print)?,
                "--help" | "-h" => once(Print::Help, &mut print)?,
                "--environment" => once(Print::Environment, &mut print)?,
                "--config" => {
                    let path = args.next().ok_or("--config needs a path")?;
                    config = Some(PathBuf::from(path));
                }
                other if other.starts_with("--config=") => {
                    config = Some(PathBuf::from(&other["--config=".len()..]));
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(Self { config, print })
    }
}

/// The `--environment` block: the facts a bug report needs, and nothing else (FR-116, FR-071).
///
/// Six `key: value` lines on **stdout**, in the order `contracts/cli.md` lists them, with an
/// explicit word wherever a value is unavailable so a pasted report has no silent gaps. Nothing
/// is read that the daemon does not already read: the configuration file's own text never
/// appears, no window title ever does, and no path outside the configuration file and the icon
/// set is printed.
fn environment_report(options: &Options, configuration: &Configuration) -> String {
    let hyprland = Environment::read()
        .ok()
        .map(|environment| Ipc::new(&environment.runtime_dir, &environment.signature))
        .and_then(|ipc| ipc.version().ok())
        .map_or_else(
            || UNAVAILABLE.to_owned(),
            |version| {
                // The tag is what a packager or a bug reporter recognises; a build made from no
                // tag says so rather than leaving the brackets to look like a formatting fault.
                let tag = version.tag.as_deref().unwrap_or("no tag");
                format!("{} ({tag})", version.version)
            },
        );

    let config = options
        .config
        .clone()
        .or_else(config::default_path)
        .map_or_else(
            || UNAVAILABLE.to_owned(),
            |path| {
                let presence = if path.exists() { "present" } else { "absent" };
                format!("{} ({presence})", path.display())
            },
        );

    let differences = config::differences(configuration);
    let settings = if differences.is_empty() {
        "defaults".to_owned()
    } else {
        differences.join(", ")
    };

    // The set actually resolved, not the one configured — those differ exactly when something is
    // wrong, which is the case a report is being written about. `icons = false` resolves nothing.
    let icon_set = if configuration.icons {
        icons::resolved_set(configuration.icon_set.as_deref())
    } else {
        "none".to_owned()
    };

    let notify_send = if on_path("notify-send") {
        "present"
    } else {
        "absent"
    };

    let mut report = String::new();
    for (key, value) in [
        ("hypr-swap", version()),
        ("hyprland", &hyprland),
        ("config", &config),
        ("settings", &settings),
        ("icon-set", &icon_set),
        ("notify-send", notify_send),
    ] {
        // The values line up in a column, because the block is read as a whole in an issue.
        let _ = writeln!(report, "{:<13} {value}", format!("{key}:"));
    }
    report
}

/// The word every unavailable value in the report is printed with, rather than an empty one.
const UNAVAILABLE: &str = "unavailable";

/// Whether a program is on `PATH` — the same question `diag::report` answers by spawning it.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join(program).is_file())
    })
}

/// Usage text, including the bind lines so a user who has the binary has the instructions
/// (FR-033). The lines come from `Shortcut::suggested_bind`, which is also the text
/// `docs/user/binds.md` is asserted to contain, so the two cannot drift (Principle III).
fn usage() -> String {
    let binds = Shortcut::ALL
        .into_iter()
        .map(|shortcut| {
            format!(
                "    {}\n        {}",
                shortcut.suggested_bind(),
                shortcut.description()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\
{APP_ID} {version} — Alt-Tab style workspace switcher for Hyprland

USAGE:
    {APP_ID} [--config <path>] [--environment] [--version] [--help]

OPTIONS:
    --config <path>   Use this configuration file instead of
                      $XDG_CONFIG_HOME/hypr-swap/config.toml
    --environment     Print the facts a bug report needs, and exit
    --version         Print the version and exit
    --help            Print this help and exit

The binary is the daemon. Start it once per session, typically with
    exec-once = {APP_ID}

BIND THESE IN hyprland.conf (any combination works; these are suggestions):
{binds}

Use `bind`, not `binde`: a repeating bind fires continuously while held, which
reads as continuous navigation. Either line may be left out. Bound to a bare key
with no modifier, the overlay stays open and Enter commits. Full notes, including
the fixed in-overlay keys, are in docs/user/binds.md.

CONFIGURATION (all keys optional; defaults shown):
    presentation = \"list\"      # \"list\" | \"grid\"
    placement    = \"active\"    # \"active\" | \"all\"
    order        = \"mru\"       # \"mru\" | \"compositor\" | \"monitor\"

Diagnostics go to standard error. Exit codes: 0 success, 2 usage, 3 no compositor.",
        version = version()
    )
}
