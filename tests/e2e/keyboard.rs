//! Real key events through `virtual-keyboard-unstable-v1`.
//!
//! The suite presses keys against a compositor that is running the user's documented bind lines,
//! which is what Principle V means by driving the real external interface — nothing here reaches
//! into the application.
//!
//! **The one thing that must not be forgotten**: Hyprland ignores injected keys unless the client
//! also announces its modifier state with `zwp_virtual_keyboard_v1.modifiers`. Key events alone
//! reach neither binds nor focused clients (research.md R14). Every helper below therefore keeps
//! the modifier mask and sends it around each key.

use std::io::{Seek, Write};
use std::os::fd::AsFd;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

/// evdev keycodes, as the virtual-keyboard protocol expects them (xkb keycode minus 8).
pub const KEY_ESC: u32 = 1;
pub const KEY_N: u32 = 49;
pub const KEY_TAB: u32 = 15;
pub const KEY_ENTER: u32 = 28;
pub const KEY_LEFTSHIFT: u32 = 42;
pub const KEY_LEFTALT: u32 = 56;
pub const KEY_LEFTMETA: u32 = 125;
pub const KEY_LEFT: u32 = 105;
pub const KEY_RIGHT: u32 = 106;
pub const KEY_UP: u32 = 103;
pub const KEY_DOWN: u32 = 108;
/// A key with no modifier of its own, for the sticky-mode bind (FR-022c).
pub const KEY_F12: u32 = 88;

/// xkb modifier masks, matching the depressed masks Hyprland reports.
pub const MOD_SHIFT: u32 = 1 << 0;
pub const MOD_CTRL: u32 = 1 << 2;
pub const MOD_ALT: u32 = 1 << 3;
pub const MOD_LOGO: u32 = 1 << 6;

/// A virtual keyboard attached to a nested compositor's seat.
pub struct Keyboard {
    connection: Connection,
    device: ZwpVirtualKeyboardV1,
    /// Modifiers currently held, so `modifiers` can be re-announced on every change.
    depressed: u32,
    /// Milliseconds, monotonically increasing, as the protocol requires.
    clock: u32,
}

impl Keyboard {
    /// Attach to the nested compositor listening on `display`, e.g. `wayland-2`.
    ///
    /// The socket is opened by path rather than through `WAYLAND_DISPLAY`, so the test process
    /// never has to mutate its own environment to reach the instance under test.
    ///
    /// # Panics
    /// If the compositor is unreachable or does not offer the virtual-keyboard protocol.
    #[must_use]
    pub fn attach(display: &str) -> Self {
        let runtime = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is set");
        let socket =
            std::os::unix::net::UnixStream::connect(std::path::Path::new(&runtime).join(display))
                .expect("the nested compositor's wayland socket");
        let connection =
            Connection::from_socket(socket).expect("the nested compositor is reachable");
        let (globals, mut queue) = registry_queue_init::<State>(&connection).expect("registry");
        let qh = queue.handle();

        let manager: ZwpVirtualKeyboardManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .expect("zwp_virtual_keyboard_manager_v1 (Hyprland >= 0.55)");
        let seat: WlSeat = globals.bind(&qh, 1..=9, ()).expect("wl_seat");
        let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());

        // A keymap must arrive before any key, or the device is created but ignored.
        let keymap = default_keymap();
        let mut file = tempfile();
        file.write_all(keymap.as_bytes()).expect("write the keymap");
        file.write_all(&[0]).expect("terminate the keymap");
        file.rewind().expect("rewind the keymap");
        keyboard.keymap(
            1,
            file.as_fd(),
            u32::try_from(keymap.len() + 1).expect("a keymap smaller than 4 GiB"),
        );

        let mut state = State;
        queue
            .roundtrip(&mut state)
            .expect("the compositor accepts the keymap");

        Self {
            connection,
            device: keyboard,
            depressed: 0,
            clock: 1,
        }
    }

    /// Press a key, announcing any modifier it is (`ALT` and friends) before the key event.
    pub fn press(&mut self, key: u32) {
        if let Some(mask) = modifier_mask(key) {
            self.depressed |= mask;
            self.announce_modifiers();
        }
        self.key(key, true);
    }

    /// Release a key, announcing the modifier state after the key event.
    pub fn release(&mut self, key: u32) {
        self.key(key, false);
        if let Some(mask) = modifier_mask(key) {
            self.depressed &= !mask;
            self.announce_modifiers();
        }
    }

    /// Press and release, the ordinary "tap" a user makes.
    pub fn tap(&mut self, key: u32) {
        self.press(key);
        self.settle();
        self.release(key);
        self.settle();
    }

    /// How long a key stays down in a tap.
    ///
    /// This matters more than it looks. A bind's `released` event fires on the bind's *key*, not
    /// its modifier (research.md R4), and the application treats a release that arrives before the
    /// overlay has keyboard focus as a fast tap. Injecting press and release with no interval
    /// makes the compositor deliver both bind events in one batch, so the overlay never gets the
    /// round trip it needs to be focused — a race no human keyboard can produce. A human tap is
    /// tens of milliseconds; the R4 spike measured focus arriving at 6 ms, comfortably inside it.
    /// Use [`Self::tap_fast`] to exercise the fast-tap path deliberately.
    const KEY_HOLD: Duration = Duration::from_millis(60);

    /// Hold `modifier`, tap `key` `times`, then release the modifier — the hold-and-release
    /// gesture the whole switcher is built around.
    ///
    /// `between` is the pause after each tap, which is what gives the highlight time to move
    /// before the next one.
    pub fn hold_with_taps(&mut self, modifier: u32, key: u32, times: usize, between: Duration) {
        self.press(modifier);
        self.settle();
        for _ in 0..times {
            self.tap_while_held(key);
            std::thread::sleep(between);
        }
        self.release(modifier);
        self.settle();
    }

    /// Hold a modifier without releasing it, so a test can inspect the open overlay.
    pub fn hold(&mut self, modifier: u32) {
        self.press(modifier);
        self.settle();
    }

    /// Tap `key` while a modifier is already held, at human speed.
    pub fn tap_while_held(&mut self, key: u32) {
        self.press(key);
        self.flush();
        std::thread::sleep(Self::KEY_HOLD);
        self.release(key);
        self.settle();
    }

    /// Press and release inside one batch, faster than any overlay could map — the gesture
    /// FR-005's fast-tap path exists for.
    pub fn tap_fast(&mut self, key: u32) {
        self.press(key);
        self.release(key);
        self.flush();
    }

    /// Send the pending requests and give the compositor a moment to act on them.
    pub fn settle(&mut self) {
        self.flush();
        std::thread::sleep(Duration::from_millis(60));
    }

    pub fn flush(&mut self) {
        let _ = self.connection.flush();
    }

    fn key(&mut self, key: u32, pressed: bool) {
        let time = self.tick();
        self.device.key(time, key, u32::from(pressed));
        self.flush();
    }

    fn announce_modifiers(&mut self) {
        self.device.modifiers(self.depressed, 0, 0, 0);
        self.flush();
    }

    fn tick(&mut self) -> u32 {
        self.clock = self.clock.wrapping_add(10).max(1);
        self.clock
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        // Leaving a modifier stuck down would poison the next test.
        if self.depressed != 0 {
            self.depressed = 0;
            self.announce_modifiers();
        }
        self.device.destroy();
        self.flush();
    }
}

/// Which modifier bit a keycode carries, if any.
fn modifier_mask(key: u32) -> Option<u32> {
    match key {
        KEY_LEFTSHIFT => Some(MOD_SHIFT),
        KEY_LEFTALT => Some(MOD_ALT),
        KEY_LEFTMETA => Some(MOD_LOGO),
        _ => None,
    }
}

fn default_keymap() -> String {
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
    .expect("a default US keymap");
    keymap.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1)
}

/// An anonymous file to pass the keymap through. Unlinked immediately; the descriptor keeps it
/// alive until the compositor has read it.
fn tempfile() -> std::fs::File {
    let path = std::env::temp_dir().join(format!(
        "hypr-swap-e2e-keymap-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("create the keymap file");
    let _ = std::fs::remove_file(&path);
    file
}

/// The injector consumes no events; this exists only to satisfy the dispatch machinery.
struct State;

macro_rules! ignore_events {
    ($($proxy:ty),* $(,)?) => {
        $(impl Dispatch<$proxy, ()> for State {
            fn event(
                _: &mut Self,
                _: &$proxy,
                _: <$proxy as wayland_client::Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        })*
    };
}

ignore_events!(ZwpVirtualKeyboardManagerV1, ZwpVirtualKeyboardV1, WlSeat);

impl Dispatch<wayland_client::protocol::wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: <wayland_client::protocol::wl_registry::WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
