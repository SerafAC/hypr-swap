//! Research spike R4 — does commit-on-release actually work?
//!
//! Maps an overlay layer surface with **exclusive** keyboard interactivity when a named global
//! shortcut fires, and logs every `wl_keyboard.modifiers` event with a timestamp. It answers the
//! two questions research.md R4 says must be settled before the switcher is wired up:
//!
//! (a) does Hyprland deliver `modifiers` to an exclusive-mode layer surface both on `enter` and
//!     on every later change, **including the release of a modifier that participates in the
//!     active bind**; and
//! (b) does `pressed` → first frame stay inside SC-001's 150 ms budget.
//!
//! It drives itself: a `virtual-keyboard-unstable-v1` device presses ALT, taps F12, and releases
//! ALT, so the spike needs no human at the keyboard and never disturbs a real session. Run it
//! inside a nested Hyprland whose config carries
//! `bind = ALT, F12, global, hypr-swap-spike:probe`.

use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
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
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use hypr_swap::ui::shortcuts::{HyprlandGlobalShortcutV1, HyprlandGlobalShortcutsManagerV1};

/// evdev keycodes, as the virtual-keyboard protocol expects them (xkb keycode minus 8).
const KEY_LEFTALT: u32 = 56;
const KEY_F12: u32 = 88;

const APP_ID: &str = "hypr-swap-spike";
const SHORTCUT_ID: &str = "probe";

const WIDTH: u32 = 400;
const HEIGHT: u32 = 200;

fn main() {
    let conn = Connection::connect_to_env().expect("a Wayland compositor to talk to");
    let (globals, mut queue) = registry_queue_init(&conn).expect("registry");
    let qh = queue.handle();

    let shortcuts: HyprlandGlobalShortcutsManagerV1 = globals
        .bind(&qh, 1..=1, Ignored)
        .expect("hyprland_global_shortcuts_manager_v1 (Hyprland >= 0.55)");
    shortcuts.register_shortcut(
        SHORTCUT_ID.to_owned(),
        APP_ID.to_owned(),
        "Spike probe".to_owned(),
        "Hold ALT and tap F12".to_owned(),
        &qh,
        Ignored,
    );

    let virtual_keyboard_manager: ZwpVirtualKeyboardManagerV1 = globals
        .bind(&qh, 1..=1, Ignored)
        .expect("zwp_virtual_keyboard_manager_v1");

    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let mut spike = Spike {
        pool: SlotPool::new((WIDTH * HEIGHT * 4) as usize, &shm).expect("shm pool"),
        shm,
        compositor: CompositorState::bind(&globals, &qh).expect("wl_compositor"),
        layer_shell: LayerShell::bind(&globals, &qh).expect("zwlr_layer_shell_v1"),
        registry: RegistryState::new(&globals),
        seat: SeatState::new(&globals, &qh),
        output: OutputState::new(&globals, &qh),
        keyboard: None,
        virtual_keyboard_manager,
        virtual_keyboard: None,
        layer: None,
        drawn: false,
        pressed_at: None,
        first_frame: None,
        modifier_events: Vec::new(),
        entered: false,
        shortcut_released: false,
        finished: false,
        started: Instant::now(),
    };

    // Let the seat, the shortcut registration and the keyboard settle before typing.
    for _ in 0..3 {
        queue.roundtrip(&mut spike).expect("roundtrip");
    }
    spike.arm_virtual_keyboard(&qh);
    queue.roundtrip(&mut spike).expect("roundtrip");

    println!("== R4 spike ==");
    // Give the compositor time to record the registration before typing, and leave a window in
    // which `hyprctl globalshortcuts` can be observed from outside.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(150));
        queue.roundtrip(&mut spike).expect("roundtrip");
    }
    println!("registered; injecting: ALT down, F12 down, F12 up, (400 ms), ALT up");

    // The gesture runs on its own thread so the main thread can stay parked in
    // `blocking_dispatch` — the only way to see modifier events at the instant they arrive.
    let keyboard = spike.virtual_keyboard.clone().expect("virtual keyboard");
    let injector_conn = conn.clone();
    let injector_qh = qh.clone();
    std::thread::spawn(move || {
        // `(delay, keycode, pressed, modifier mask after this step)`. The virtual-keyboard
        // protocol makes the client responsible for the modifier state, so ALT is announced
        // explicitly rather than inferred from the keycode.
        const MOD_ALT: u32 = 1 << 3;
        let script = [
            (0u64, KEY_LEFTALT, true, MOD_ALT),
            (30, KEY_F12, true, MOD_ALT),
            (60, KEY_F12, false, MOD_ALT),
            (460, KEY_LEFTALT, false, 0),
        ];
        let started = Instant::now();
        for (at, key, down, mods) in script {
            let now = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            if at > now {
                std::thread::sleep(Duration::from_millis(at - now));
            }
            let time = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
            if down {
                keyboard.modifiers(mods, 0, 0, 0);
                keyboard.key(time, key, 1);
            } else {
                keyboard.key(time, key, 0);
                keyboard.modifiers(mods, 0, 0, 0);
            }
            let _ = injector_conn.flush();
            println!(
                "  [{time:>4} ms] inject {} {key} (mods 0x{mods:x})",
                if down { "press  " } else { "release" }
            );
        }
        // Whatever the compositor did or did not send, this sync guarantees one last event so
        // the main thread leaves `blocking_dispatch` and reports rather than hanging.
        std::thread::sleep(Duration::from_millis(400));
        injector_conn.display().sync(&injector_qh, Ignored);
        let _ = injector_conn.flush();
    });

    while spike.started.elapsed() < Duration::from_secs(10) && !spike.finished {
        queue.blocking_dispatch(&mut spike).expect("dispatch");
    }

    spike.report();
}

#[allow(clippy::struct_excessive_bools)]
struct Spike {
    registry: RegistryState,
    seat: SeatState,
    output: OutputState,
    shm: Shm,
    pool: SlotPool,
    compositor: CompositorState,
    layer_shell: LayerShell,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    virtual_keyboard_manager: ZwpVirtualKeyboardManagerV1,
    virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    layer: Option<LayerSurface>,
    drawn: bool,
    pressed_at: Option<Instant>,
    first_frame: Option<Duration>,
    /// `(milliseconds since start, depressed mask, note)`
    modifier_events: Vec<(u128, u32, &'static str)>,
    entered: bool,
    shortcut_released: bool,
    /// Set by the trailing sync callback: the gesture is over, report and exit.
    finished: bool,
    started: Instant,
}

impl Spike {
    fn arm_virtual_keyboard(&mut self, qh: &QueueHandle<Self>) {
        let seat = self.seat.seats().next().expect("a seat");
        let keyboard = self
            .virtual_keyboard_manager
            .create_virtual_keyboard(&seat, qh, Ignored);

        // The compositor needs a keymap before it will interpret injected keycodes.
        let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
        let keymap = xkbcommon::xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("a default keymap");
        let text = keymap.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1);

        let file = tempfile_with(text.as_bytes());
        keyboard.keymap(1, file.as_fd(), u32::try_from(text.len() + 1).unwrap());
        self.virtual_keyboard = Some(keyboard);
        std::mem::forget(file);
    }

    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let Some(layer) = &self.layer else { return };
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                i32::try_from(WIDTH).unwrap(),
                i32::try_from(HEIGHT).unwrap(),
                i32::try_from(WIDTH * 4).unwrap(),
                wayland_client::protocol::wl_shm::Format::Argb8888,
            )
            .expect("buffer");
        for pixel in canvas.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0x40, 0x20, 0x10, 0xE0]);
        }
        let surface = layer.wl_surface();
        surface.damage_buffer(
            0,
            0,
            i32::try_from(WIDTH).unwrap(),
            i32::try_from(HEIGHT).unwrap(),
        );
        buffer.attach_to(surface).expect("attach");
        layer.commit();

        if self.first_frame.is_none()
            && let Some(pressed) = self.pressed_at
        {
            self.first_frame = Some(pressed.elapsed());
        }
        self.drawn = true;
    }

    fn open_overlay(&mut self, qh: &QueueHandle<Self>) {
        if self.layer.is_some() {
            return;
        }
        let surface = self.compositor.create_surface(qh);
        let layer =
            self.layer_shell
                .create_layer_surface(qh, surface, Layer::Overlay, Some(APP_ID), None);
        layer.set_size(WIDTH, HEIGHT);
        layer.set_anchor(Anchor::empty());
        layer.set_exclusive_zone(0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        self.layer = Some(layer);
    }

    fn note(&mut self, mask: u32, note: &'static str) {
        self.modifier_events
            .push((self.started.elapsed().as_millis(), mask, note));
        println!(
            "  [{:>4} ms] modifiers depressed=0x{mask:04x} ({note})",
            self.started.elapsed().as_millis()
        );
    }

    fn report(&self) {
        println!("\n== findings ==");

        let on_enter = self
            .modifier_events
            .iter()
            .any(|(_, _, note)| *note == "on enter");
        let alt_on_enter = self
            .modifier_events
            .iter()
            .find(|(_, _, note)| *note == "on enter")
            .is_some_and(|(_, mask, _)| mask & 0x8 != 0);
        let release_seen = self
            .modifier_events
            .iter()
            .skip_while(|(_, _, note)| *note == "on enter")
            .any(|(_, mask, _)| mask & 0x8 == 0);

        println!(
            "(a1) modifiers delivered on enter:            {}",
            yes_no(on_enter)
        );
        println!(
            "(a2) ALT visible in the enter mask:           {}",
            yes_no(alt_on_enter)
        );
        println!(
            "(a3) modifiers delivered on ALT release:      {}",
            yes_no(release_seen)
        );
        match self.first_frame {
            Some(latency) => println!(
                "(b)  pressed -> first frame:                  {} ms  (budget 150 ms) {}",
                latency.as_millis(),
                yes_no(latency <= Duration::from_millis(150))
            ),
            None => println!("(b)  pressed -> first frame:                  never drawn  NO"),
        }
        println!(
            "\nverdict: {}",
            if on_enter && alt_on_enter && release_seen {
                "PASS — commit-on-release via exclusive layer-shell focus is viable"
            } else {
                "FAIL — fall back to keyboard-shortcuts-inhibit-unstable-v1 (research.md R4)"
            }
        );
    }
}

fn yes_no(condition: bool) -> &'static str {
    if condition { "YES" } else { "NO" }
}

use std::os::fd::AsFd;

fn tempfile_with(bytes: &[u8]) -> std::fs::File {
    use std::io::{Seek, Write};
    let path = std::env::temp_dir().join(format!("hypr-swap-spike-keymap-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("keymap file");
    file.write_all(bytes).expect("write keymap");
    file.write_all(&[0]).expect("terminate keymap");
    file.rewind().expect("rewind");
    let _ = std::fs::remove_file(&path);
    file
}

// --- Wayland plumbing -------------------------------------------------------

/// Wayland user data for objects whose events this spike either ignores or handles inline.
///
/// SCTK 0.21 routes dispatch through the user data, so one type covers every object here.
#[derive(Debug, Clone, Copy)]
struct Ignored;

impl Dispatch2<HyprlandGlobalShortcutsManagerV1, Spike> for Ignored {
    fn event(
        &self,
        _: &mut Spike,
        _: &HyprlandGlobalShortcutsManagerV1,
        _: <HyprlandGlobalShortcutsManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Spike>,
    ) {
    }
}

impl Dispatch2<ZwpVirtualKeyboardManagerV1, Spike> for Ignored {
    fn event(
        &self,
        _: &mut Spike,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Spike>,
    ) {
    }
}

impl Dispatch2<ZwpVirtualKeyboardV1, Spike> for Ignored {
    fn event(
        &self,
        _: &mut Spike,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Spike>,
    ) {
    }
}

impl Dispatch2<wayland_client::protocol::wl_callback::WlCallback, Spike> for Ignored {
    fn event(
        &self,
        state: &mut Spike,
        _: &wayland_client::protocol::wl_callback::WlCallback,
        _: <wayland_client::protocol::wl_callback::WlCallback as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Spike>,
    ) {
        state.finished = true;
    }
}

impl Dispatch2<HyprlandGlobalShortcutV1, Spike> for Ignored {
    fn event(
        &self,
        state: &mut Spike,
        _: &HyprlandGlobalShortcutV1,
        event: <HyprlandGlobalShortcutV1 as Proxy>::Event,
        _: &Connection,
        qh: &QueueHandle<Spike>,
    ) {
        use hypr_swap::ui::shortcuts::protocol::hyprland_global_shortcut_v1::Event;
        match event {
            Event::Pressed { .. } => {
                println!(
                    "  [{:>4} ms] shortcut PRESSED",
                    state.started.elapsed().as_millis()
                );
                state.pressed_at = Some(Instant::now());
                state.open_overlay(qh);
            }
            Event::Released { .. } => {
                println!(
                    "  [{:>4} ms] shortcut RELEASED (overlay had focus: {})",
                    state.started.elapsed().as_millis(),
                    state.entered
                );
                state.shortcut_released = true;
            }
            _ => {}
        }
    }
}

impl LayerShellHandler for Spike {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.layer = None;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.draw(qh);
    }
}

impl KeyboardHandler for Spike {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.entered = true;
        println!(
            "  [{:>4} ms] keyboard ENTER (exclusive focus granted)",
            self.started.elapsed().as_millis()
        );
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.entered = false;
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!(
            "  [{:>4} ms] key press raw={} keysym={:?}",
            self.started.elapsed().as_millis(),
            event.raw_code,
            event.keysym.name()
        );
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        raw: RawModifiers,
        _: u32,
    ) {
        let note = if self.modifier_events.is_empty() {
            "on enter"
        } else {
            "on change"
        };
        self.note(raw.depressed, note);
    }
}

impl SeatHandler for Spike {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
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
            self.keyboard = self.seat.get_keyboard(qh, &seat, None).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl CompositorHandler for Spike {
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

impl OutputHandler for Spike {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Spike {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Spike {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Spike);
delegate_dispatch2!(Spike);
