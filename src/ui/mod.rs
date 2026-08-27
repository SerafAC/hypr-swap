//! The Wayland client: connection, registry, seat and keyboard, layer surfaces, shm.
//!
//! This module and `main.rs` are the deliberately logic-free shell. Every decision the overlay
//! makes lives in `session`, `ordering`, `actions` and `ui::layout`, which are unit-tested
//! without a compositor; what remains here is protocol plumbing, covered by the nested-Hyprland
//! E2E suite (plan.md → Complexity Tracking).
//!
//! The shell never talks to Hyprland's IPC itself. It records what happened in [`App::outbox`]
//! and `main.rs` drains it, which is what keeps the Wayland event handlers free of I/O.

pub mod layout;
pub mod render;
pub mod shortcuts;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, EventQueue, Proxy, QueueHandle};

use crate::config::{Configuration, Placement, Presentation};
use crate::diag::{self, Condition};
use crate::model::MonitorName;
use crate::ordering;
use crate::session::{self, Session};
use crate::state::World;
use crate::ui::layout::Metrics;
use crate::ui::shortcuts::{HyprlandGlobalShortcutV1, HyprlandGlobalShortcutsManagerV1, Shortcut};

/// The initial shm pool size. `SlotPool` grows on demand, so this only avoids a resize for the
/// common single-overlay case.
const INITIAL_POOL_BYTES: usize = 4 * 1024 * 1024;

/// What the Wayland shell asks `main.rs` to do once it has handled an event.
///
/// Everything that needs compositor I/O leaves through here, so the event handlers stay pure
/// enough to reason about and `main.rs` keeps sole ownership of the IPC connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The switcher shortcut fired. Opens a session, or advances the highlight when one is
    /// already open (FR-003, FR-028).
    SwitcherPressed,
    /// The switcher shortcut was released. Only interesting before the overlay ever gained
    /// keyboard focus — the fast-tap path (FR-005).
    SwitcherReleased,
    /// The new-workspace shortcut fired (FR-020). Never opens an overlay.
    NewWorkspace,
    /// A session reached [`crate::session::Outcome::Committed`] or
    /// [`crate::session::Outcome::Cancelled`]. `main.rs` turns the outcome into dispatches.
    SessionEnded,
}

/// Why the client could not start. Every variant is fatal at start-up and exits 3
/// (`contracts/cli.md`).
#[derive(Debug)]
pub enum StartupError {
    /// No Wayland connection — no compositor, or `WAYLAND_DISPLAY` is wrong.
    NoConnection(wayland_client::ConnectError),
    /// The compositor answered but does not offer something this application cannot work without.
    MissingGlobal(&'static str, String),
    /// Shared memory could not be set up.
    Shm(String),
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConnection(e) => write!(f, "cannot connect to the Wayland display: {e}"),
            Self::MissingGlobal(global, e) => {
                write!(f, "the compositor does not offer {global}: {e}")
            }
            Self::Shm(e) => write!(f, "cannot set up shared memory: {e}"),
        }
    }
}

/// The Wayland connection and its event queue, handed to `main.rs` so it can drive them from
/// calloop alongside the Hyprland event socket.
pub struct Wayland {
    pub connection: Connection,
    pub queue: EventQueue<App>,
}

/// The whole client state. This is the type the Wayland queue dispatches into and the type
/// calloop carries as its loop data, which is why it holds the world and the configuration as
/// well as the protocol objects.
pub struct App {
    registry: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    /// Buffers for the overlay. Unused until a session opens.
    pool: SlotPool,
    compositor: CompositorState,
    layer_shell: LayerShell,
    /// Maps each overlay's device-pixel buffer onto its logical-pixel surface size, which is what
    /// keeps the overlay the same physical size on a scaled monitor as on an unscaled one.
    viewporter: WpViewporter,
    shortcuts_manager: HyprlandGlobalShortcutsManagerV1,
    /// Held for the lifetime of the connection: dropping a shortcut unregisters it.
    registered: Vec<HyprlandGlobalShortcutV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Needed to create surfaces from outside an event handler, when `main.rs` acts on a request.
    qh: QueueHandle<Self>,
    /// The most recent modifier state, for the Shift in Shift+Tab (FR-004a).
    modifiers: Modifiers,

    /// The open session, if any. At most one exists at a time (FR-028).
    session: Option<Session>,
    /// The mapped copies of the overlay: one on the focused monitor by default, one per
    /// connected monitor under `placement = "all"` (FR-017). All of them show the same session,
    /// so the highlight cannot differ between them. Empty on the fast-tap path, which never
    /// shows one (FR-005).
    overlays: Vec<Overlay>,

    /// User settings, read once at start-up.
    pub config: Configuration,
    /// The cached compositor view, kept current by `main.rs` from the event socket.
    pub world: World,
    /// Work for `main.rs`, drained after every dispatch.
    pub outbox: Vec<Request>,
}

/// One overlay still to be mapped: where it goes, how big it is, and whether it is the copy the
/// session reads the keyboard from.
struct Target {
    /// The output to ask for, or `None` to let the compositor pick the focused monitor.
    output: Option<wl_output::WlOutput>,
    monitor: MonitorName,
    metrics: Metrics,
    exclusive: bool,
}

/// One mapped layer surface showing the session.
struct Overlay {
    layer: LayerSurface,
    /// The monitor this copy is on, for the diagnostic when the compositor withdraws it.
    monitor: MonitorName,
    /// Whether this copy is the one holding exclusive keyboard focus. Exactly one is, even under
    /// `placement = "all"`: the session reads one keyboard, on the monitor the user is looking at
    /// (FR-002a, FR-017).
    exclusive: bool,
    /// Declares that this surface's buffer is in device pixels while the surface itself is the
    /// logical size the compositor configured. Without it the buffer would be taken as logical
    /// and the overlay would come out `scale` times too big on a scaled monitor (FR-019).
    viewport: WpViewport,
    /// Geometry for this monitor, fixed for the life of the session.
    metrics: Metrics,
    /// Held until the next frame replaces it; releasing it early would show a torn buffer.
    buffer: Option<Buffer>,
    /// The entry index at the top of the viewport, carried across navigation so scrolling
    /// depends on which way the user came from (FR-019).
    first_visible: usize,
    /// Set by the first `configure`; nothing may be attached before it arrives.
    configured: bool,
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // A viewport that outlives its surface makes every later request a protocol error, so it
        // goes first — before the `LayerSurface` field below it is dropped.
        self.viewport.destroy();
    }
}

/// Connect to the compositor and bind everything the application cannot work without.
///
/// A missing required global is fatal here rather than at first use, so a misconfigured system
/// fails at start-up with a clear message instead of when the user first presses the shortcut
/// (`contracts/compositor-ipc.md`).
///
/// # Errors
/// [`StartupError`] when the compositor cannot be reached, does not offer a required protocol,
/// or shared memory cannot be set up. Every variant is fatal at start-up and exits 3.
pub fn connect(config: Configuration, world: World) -> Result<(Wayland, App), StartupError> {
    let connection = Connection::connect_to_env().map_err(StartupError::NoConnection)?;
    let (globals, queue) = registry_queue_init(&connection)
        .map_err(|e| StartupError::MissingGlobal("wl_registry", e.to_string()))?;
    let qh = queue.handle();

    let shm = Shm::bind(&globals, &qh)
        .map_err(|e| StartupError::MissingGlobal("wl_shm", e.to_string()))?;
    let pool =
        SlotPool::new(INITIAL_POOL_BYTES, &shm).map_err(|e| StartupError::Shm(e.to_string()))?;
    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| StartupError::MissingGlobal("wl_compositor", e.to_string()))?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|e| StartupError::MissingGlobal("zwlr_layer_shell_v1", e.to_string()))?;
    // Required rather than optional: without it there is no way to paint at a scaled monitor's
    // real resolution and still ask for a correctly-sized surface, and Hyprland has offered it
    // for its whole supported range (contracts/compositor-ipc.md).
    let viewporter: WpViewporter = globals
        .bind(&qh, 1..=1, shortcuts::NoData)
        .map_err(|e| StartupError::MissingGlobal("wp_viewporter", e.to_string()))?;
    let shortcuts_manager: HyprlandGlobalShortcutsManagerV1 =
        globals.bind(&qh, 1..=1, shortcuts::NoData).map_err(|e| {
            StartupError::MissingGlobal("hyprland_global_shortcuts_manager_v1", e.to_string())
        })?;

    let mut app = App {
        registry: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        compositor,
        layer_shell,
        viewporter,
        shortcuts_manager,
        registered: Vec::new(),
        keyboard: None,
        qh: qh.clone(),
        modifiers: Modifiers::default(),
        session: None,
        overlays: Vec::new(),
        config,
        world,
        outbox: Vec::new(),
    };
    app.register_shortcuts(&qh);

    Ok((Wayland { connection, queue }, app))
}

impl App {
    /// Register both named shortcuts (FR-022), and again after every reconnect (FR-026b).
    ///
    /// The protocol reports a duplicate `app_id` + `id` as a fatal protocol error rather than a
    /// recoverable one, so a collision is detected before this point — see `main.rs`'s
    /// second-instance check (FR-025a).
    pub fn register_shortcuts(&mut self, qh: &QueueHandle<Self>) {
        self.registered.clear();
        for shortcut in Shortcut::ALL {
            let object = self.shortcuts_manager.register_shortcut(
                shortcut.id().to_owned(),
                crate::APP_ID.to_owned(),
                shortcut.description().to_owned(),
                shortcut.trigger_description().to_owned(),
                qh,
                shortcuts::ShortcutData(shortcut),
            );
            self.registered.push(object);
        }
    }

    /// Whether an overlay is currently mapped.
    #[must_use]
    pub fn has_overlay(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// The switcher shortcut fired (FR-001, FR-003, FR-028).
    ///
    /// A press with a session already open advances the highlight rather than opening a second
    /// overlay. That is not a special case bolted on for FR-028 — it is the only way the second
    /// Tab of an Alt-Tab-Tab gesture can reach this application at all, because the compositor
    /// consumes the bind's key before any client sees it (research.md R5).
    pub fn switcher_pressed(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.next();
            self.draw();
            return;
        }
        self.open_session();
    }

    /// The switcher shortcut was released — only meaningful on the fast-tap path (FR-005).
    pub fn switcher_released(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.shortcut_released();
            self.settle();
        }
    }

    /// Take the finished session so `main.rs` can act on its outcome.
    pub fn take_session(&mut self) -> Option<Session> {
        // An open session is not the caller's to take: it is still collecting input.
        if self.session.as_ref().is_some_and(Session::is_open) {
            return None;
        }
        self.close_overlay();
        self.session.take()
    }

    /// Close any open overlay without committing (FR-026a, and the `SIGTERM` path).
    pub fn abandon_session(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.connection_lost();
        }
        self.close_overlay();
        self.session = None;
    }

    /// Open a session on a snapshot of the world and map its overlay on every monitor the
    /// placement setting asks for.
    fn open_session(&mut self) {
        let (entries, highlight) = ordering::entries(&self.world, self.config.order);
        let Some(monitor) = self.world.focused_monitor() else {
            // No focused monitor means no compositor state worth showing; the event socket will
            // report the disconnection along its own path.
            return;
        };
        let origin = monitor.name.clone();

        let Some(session) = Session::open(entries, highlight, origin) else {
            // Nothing to switch between. Taking the user's keyboard to show an empty box would
            // be worse than doing nothing.
            return;
        };

        let targets = self.targets(&session.origin_monitor, session.entries.len());
        self.session = Some(session);
        for target in targets {
            self.map_overlay(target);
        }
    }

    /// Geometry for one monitor.
    ///
    /// The one place the two presentations diverge (FR-016, US3-AS4): a different shape of
    /// overlay is asked for, and everything after this — navigation, commit, cancel — is the
    /// same code either way.
    fn metrics_for(&self, monitor_size: (u32, u32), scale: f32, entry_count: usize) -> Metrics {
        match self.config.presentation {
            Presentation::List => layout::list_metrics(monitor_size, scale, entry_count),
            Presentation::Grid => layout::grid_metrics(monitor_size, scale, entry_count),
        }
    }

    /// Which monitors get a copy of the overlay, and which copy takes the keyboard (FR-017).
    ///
    /// `placement = "active"` names no output at all: a layer surface with no output goes to the
    /// monitor holding the focused workspace, which is exactly the setting's definition, and is
    /// left to the compositor rather than second-guessed. `placement = "all"` has to name each
    /// output explicitly, because that is the only way to ask for a surface on a monitor that is
    /// not the focused one.
    ///
    /// Only the focused monitor's copy is exclusive. One session, one keyboard, one highlight —
    /// the other copies are there to be looked at (US5-AS2/AS3). A set that would leave nobody
    /// holding the keyboard is discarded for the single active-monitor overlay: an unreadable
    /// gesture would be worse than ignoring the setting, and it is what a compositor that does
    /// not name its outputs would otherwise produce.
    fn targets(&self, origin: &MonitorName, entry_count: usize) -> Vec<Target> {
        if self.config.placement == Placement::AllMonitors {
            let all: Vec<Target> = self
                .world
                .monitors
                .iter()
                .filter_map(|monitor| {
                    Some(Target {
                        output: Some(self.output_named(&monitor.name)?),
                        monitor: monitor.name.clone(),
                        metrics: self.metrics_for(monitor.size, monitor.scale, entry_count),
                        exclusive: monitor.name == *origin,
                    })
                })
                .collect();
            if all.iter().any(|target| target.exclusive) {
                return all;
            }
        }

        self.world
            .focused_monitor()
            .map(|monitor| Target {
                output: None,
                monitor: monitor.name.clone(),
                metrics: self.metrics_for(monitor.size, monitor.scale, entry_count),
                exclusive: true,
            })
            .into_iter()
            .collect()
    }

    /// The `wl_output` the compositor calls `name`, if it has told us its name yet.
    ///
    /// Output names are the only thing tying the Wayland side of the client to the monitor names
    /// Hyprland's IPC reports, which is what lets a per-monitor surface be sized from the
    /// monitor's own resolution and scale.
    fn output_named(&self, name: &str) -> Option<wl_output::WlOutput> {
        self.output_state.outputs().find(|output| {
            self.output_state
                .info(output)
                .and_then(|info| info.name)
                .is_some_and(|reported| reported == name)
        })
    }

    /// Map one overlay layer surface (FR-002a, FR-017, FR-018).
    ///
    /// The **overlay** layer is what puts it above a fullscreen client, and **exclusive**
    /// keyboard interactivity is what lets it observe the modifier release that commits
    /// (research.md R4, R6). The exclusive zone stays at zero: this is a transient overlay, not
    /// a panel, and must not make the compositor reserve space for it.
    ///
    /// A copy that is not holding the keyboard asks for `None` interactivity rather than
    /// `OnDemand`: a passive copy must never take focus away from the one the session is reading.
    fn map_overlay(&mut self, target: Target) {
        let surface = self.compositor.create_surface(&self.qh);
        // Created before the surface is handed to the layer shell, which takes ownership of it.
        let viewport = self
            .viewporter
            .get_viewport(&surface, &self.qh, shortcuts::NoData);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some(crate::APP_ID),
            target.output.as_ref(),
        );
        layer.set_keyboard_interactivity(if target.exclusive {
            KeyboardInteractivity::Exclusive
        } else {
            KeyboardInteractivity::None
        });
        // Logical pixels: `set_size` is in the compositor's coordinate space, not the buffer's.
        // The scale is applied to the buffer instead, by the viewport set in `draw`.
        let (surface_width, surface_height) = target.metrics.surface_size();
        layer.set_size(surface_width, surface_height);
        // Anchored to nothing, so the compositor centres it.
        layer.set_anchor(Anchor::empty());
        layer.set_exclusive_zone(0);
        layer.commit();

        self.overlays.push(Overlay {
            layer,
            monitor: target.monitor,
            exclusive: target.exclusive,
            viewport,
            metrics: target.metrics,
            buffer: None,
            first_visible: 0,
            configured: false,
        });
    }

    fn close_overlay(&mut self) {
        // Dropping the layer surfaces destroys them, which is also what returns keyboard focus to
        // whatever held it before (FR-002a) — the compositor does that for us.
        self.overlays.clear();
    }

    /// Whatever just happened to the session, do what it now implies: repaint while it is open,
    /// hand it to `main.rs` once it is not.
    fn settle(&mut self) {
        match self.session.as_ref() {
            Some(session) if session.is_open() => self.draw(),
            Some(_) => {
                self.close_overlay();
                self.outbox.push(Request::SessionEnded);
            }
            None => {}
        }
    }

    /// Paint the current highlight onto every mapped copy.
    ///
    /// One session drives them all, so `placement = "all"` cannot show two different highlights
    /// (FR-017, US5-AS3).
    fn draw(&mut self) {
        for index in 0..self.overlays.len() {
            self.draw_overlay(index);
        }
    }

    /// Paint one copy and commit its surface.
    fn draw_overlay(&mut self, index: usize) {
        let (Some(session), Some(overlay)) = (self.session.as_ref(), self.overlays.get_mut(index))
        else {
            return;
        };
        if !overlay.configured || !session.is_visible() {
            return;
        }

        overlay.first_visible = layout::first_visible_entry(
            &overlay.metrics,
            session.entries.len(),
            session.highlight,
            overlay.first_visible,
        );

        let metrics = overlay.metrics;
        let Ok(stride) = render::stride_for(metrics.width) else {
            return;
        };
        // Device pixels throughout: the buffer is allocated, painted and damaged in them.
        let (Ok(width), Ok(height)) = (i32::try_from(metrics.width), i32::try_from(metrics.height))
        else {
            return;
        };
        // Logical pixels: what that buffer is displayed at. Setting this is what stops a HiDPI
        // monitor's overlay from being drawn `scale` times larger than everything around it.
        let (surface_width, surface_height) = metrics.surface_size();
        let (Ok(destination_width), Ok(destination_height)) =
            (i32::try_from(surface_width), i32::try_from(surface_height))
        else {
            return;
        };

        let (buffer, canvas) =
            match self
                .pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            {
                Ok(pair) => pair,
                Err(e) => {
                    diag::report(
                        Condition::OverlayFocusRefused,
                        "overlay",
                        &format!("cannot allocate a buffer: {e}"),
                    );
                    return;
                }
            };

        if let Err(e) = render::overlay(
            canvas,
            &metrics,
            &session.entries,
            overlay.first_visible,
            session.highlight,
        ) {
            diag::report(
                Condition::OverlayFocusRefused,
                "overlay",
                &format!("cannot paint the overlay: {e}"),
            );
            return;
        }

        overlay
            .viewport
            .set_destination(destination_width, destination_height);

        let wl_surface = overlay.layer.wl_surface();
        if buffer.attach_to(wl_surface).is_ok() {
            wl_surface.damage_buffer(0, 0, width, height);
            overlay.layer.commit();
            overlay.buffer = Some(buffer);
        }
    }

    /// Which copy of the overlay `surface` is, if it is one of them.
    fn overlay_index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.overlays
            .iter()
            .position(|overlay| overlay.layer.wl_surface() == surface)
    }

    /// Whether `surface` is one of this session's overlays.
    fn is_overlay(&self, surface: &wl_surface::WlSurface) -> bool {
        self.overlay_index(surface).is_some()
    }

    fn on_shortcut(&mut self, shortcut: Shortcut, pressed: bool) {
        let request = match (shortcut, pressed) {
            (Shortcut::Switcher, true) => Request::SwitcherPressed,
            (Shortcut::Switcher, false) => Request::SwitcherReleased,
            (Shortcut::NewWorkspace, true) => Request::NewWorkspace,
            // The new-workspace shortcut's release is ignored (contracts/shortcuts.md).
            (Shortcut::NewWorkspace, false) => return,
        };
        self.outbox.push(request);
    }
}

// --- Shortcut protocol ------------------------------------------------------

impl Dispatch2<HyprlandGlobalShortcutsManagerV1, App> for shortcuts::NoData {
    fn event(
        &self,
        _: &mut App,
        _: &HyprlandGlobalShortcutsManagerV1,
        _: <HyprlandGlobalShortcutsManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        // The manager has no events.
    }
}

impl Dispatch2<HyprlandGlobalShortcutV1, App> for shortcuts::ShortcutData {
    fn event(
        &self,
        app: &mut App,
        _: &HyprlandGlobalShortcutV1,
        event: <HyprlandGlobalShortcutV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        use shortcuts::protocol::hyprland_global_shortcut_v1::Event;
        match event {
            Event::Pressed { .. } => app.on_shortcut(self.0, true),
            Event::Released { .. } => app.on_shortcut(self.0, false),
        }
    }
}

// --- Viewporter -------------------------------------------------------------

impl Dispatch2<WpViewporter, App> for shortcuts::NoData {
    fn event(
        &self,
        _: &mut App,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        // Neither the viewporter nor a viewport has any events.
    }
}

impl Dispatch2<WpViewport, App> for shortcuts::NoData {
    fn event(
        &self,
        _: &mut App,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

// --- Seat and keyboard ------------------------------------------------------

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(e) => diag::report(
                    Condition::OverlayFocusRefused,
                    "seat",
                    &format!("no keyboard available: {e}"),
                ),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    /// Exclusive keyboard focus arrived.
    ///
    /// The modifiers held at this instant are what a later release is compared against, but they
    /// do not arrive here — the compositor sends them in the `modifiers` event that immediately
    /// follows (verified in the R4 spike), so [`Self::update_modifiers`] does the recording.
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.is_overlay(surface) {
            // The surface has focus, so the first frame can go up.
            self.draw();
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Losing focus is not a cancellation: the overlay is torn down by whatever committed or
        // cancelled it, and this event is the consequence rather than the cause.
    }

    /// The fixed in-overlay key map (FR-004a, contracts/shortcuts.md).
    ///
    /// The table itself is [`crate::session::action_for`], so it is unit-tested; what happens
    /// here is only the lookup and the repaint. Any key not in the table is ignored, including
    /// every key the compositor has bound — those never arrive at all.
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let shift = self.modifiers.shift;
        let Some(action) = session::action_for(event.keysym.raw(), shift) else {
            return;
        };
        if let Some(session) = self.session.as_mut() {
            session.apply(action);
            self.settle();
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
        // Navigation happens on press; a key release carries nothing the session needs. The
        // *modifier* release does, and arrives as a `modifiers` event instead.
    }

    fn repeat_key(
        &mut self,
        connection: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        // Holding Tab down should walk the list, exactly as holding it in any other switcher does.
        self.press_key(connection, qh, keyboard, serial, event);
    }

    /// The commit trigger (FR-002, FR-005, research.md R4).
    ///
    /// The first `modifiers` after focus records what the user is holding; every later one is
    /// checked against it, and any of those modifiers going away commits the highlighted entry.
    /// Comparing against the initially-held set rather than against zero is what makes the
    /// gesture behave when an unrelated modifier is also down.
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        raw: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.focus_state == crate::session::FocusState::AwaitingFocus {
            session.focused(raw.depressed);
        } else {
            session.modifiers_changed(raw.depressed);
        }
        self.settle();
    }
}

// --- Surfaces ---------------------------------------------------------------

impl LayerShellHandler for App {
    /// The compositor withdrew the surface.
    ///
    /// This is the observable form of "exclusive keyboard focus was refused" (FR-002a): a
    /// compositor that will not give a layer surface exclusive interactivity closes it rather
    /// than mapping it without. Either way the session cannot do its job, so it is abandoned
    /// with a report rather than left holding a dead surface.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        let Some(index) = self.overlay_index(layer.wl_surface()) else {
            return;
        };
        if !self.overlays[index].exclusive {
            // A passive `placement = "all"` copy going away — an unplugged monitor, say — costs
            // the user nothing: the session is still on screen and still reading the keyboard
            // where it matters, so it keeps running with one fewer copy.
            self.overlays.remove(index);
            return;
        }
        let monitor = self.overlays[index].monitor.clone();
        diag::report(
            Condition::OverlayFocusRefused,
            "overlay",
            &format!(
                "the compositor closed the overlay surface on {monitor}; the session was abandoned"
            ),
        );
        self.abandon_session();
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self.overlay_index(layer.wl_surface()) else {
            return;
        };
        let entry_count = self.session.as_ref().map_or(0, |s| s.entries.len());
        if let Some(overlay) = self.overlays.get_mut(index) {
            // A compositor may hand back a size of its own choosing. Honour it rather than
            // painting outside the surface it agreed to — refitting the row count, never the
            // row size (FR-019). The size is in logical pixels, which is what `refit` takes.
            let (width, height) = configure.new_size;
            if width != 0 && height != 0 {
                overlay.metrics = layout::refit(overlay.metrics, width, height, entry_count);
            }
            overlay.configured = true;
        }
        // Only this copy has news; the others are already showing the same highlight.
        self.draw_overlay(index);
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(App);
delegate_dispatch2!(App);
