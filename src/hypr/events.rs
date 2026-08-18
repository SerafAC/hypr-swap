//! Hyprland's event socket (`.socket2.sock`): a persistent connection streaming `EVENT>>DATA`
//! lines, plus the backoff that reconnects it (FR-026a, FR-026d).
//!
//! Unknown event names are ignored, so a Hyprland release that adds events is non-breaking
//! (`contracts/compositor-ipc.md`).

use std::io::{ErrorKind, Read};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::model::MonitorName;

/// A compositor event this application acts on.
///
/// Several Hyprland events carry only a name where the application needs an id, or omit window
/// geometry entirely. Those variants exist so the event is still recognised; applying them is
/// [`crate::state::World`]'s job, which resolves what it can and asks for a rebuild otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The active workspace changed. `workspacev2` carries the id; `workspace` carries only a name.
    WorkspaceActivated { id: Option<i32>, name: String },
    /// Keyboard focus moved to another monitor, which activates that monitor's workspace.
    MonitorFocused {
        monitor: MonitorName,
        workspace_name: String,
    },
    /// A workspace came into existence. Carries no monitor binding, so the world rebuilds.
    WorkspaceCreated { name: String },
    /// A workspace ceased to exist.
    WorkspaceDestroyed { name: String },
    /// A workspace was rebound to another monitor.
    WorkspaceMoved { name: String, monitor: MonitorName },
    /// A window was mapped. Carries no geometry, so the world rebuilds.
    WindowOpened { address: String },
    /// A window went away.
    WindowClosed { address: String },
    /// A window changed workspace. Its geometry changes with it, so the world rebuilds.
    WindowMoved { address: String },
    /// A window's title changed. `windowtitlev2` carries the new title; `windowtitle` does not.
    WindowTitleChanged {
        address: String,
        title: Option<String>,
    },
    /// A monitor was connected or disconnected — this reshuffles workspace bindings, so the
    /// world rebuilds wholesale.
    MonitorsChanged,
}

/// Parse one `EVENT>>DATA` line, or `None` for an event this application does not act on.
#[must_use]
pub fn parse_line(line: &str) -> Option<Event> {
    let (name, data) = line.trim_end().split_once(">>")?;
    let fields: Vec<&str> = data.split(',').collect();
    // `fields` always has at least one element, so `fields[0]` below is safe for every name.
    let field = |index: usize| fields.get(index).copied().unwrap_or_default().to_owned();

    Some(match name {
        "workspace" => Event::WorkspaceActivated {
            id: None,
            name: field(0),
        },
        "workspacev2" => Event::WorkspaceActivated {
            id: fields[0].parse().ok(),
            name: field(1),
        },
        "focusedmon" => Event::MonitorFocused {
            monitor: field(0),
            workspace_name: field(1),
        },
        "createworkspace" => Event::WorkspaceCreated { name: field(0) },
        "createworkspacev2" => Event::WorkspaceCreated { name: field(1) },
        "destroyworkspace" => Event::WorkspaceDestroyed { name: field(0) },
        "destroyworkspacev2" => Event::WorkspaceDestroyed { name: field(1) },
        "moveworkspace" => Event::WorkspaceMoved {
            name: field(0),
            monitor: field(1),
        },
        "moveworkspacev2" => Event::WorkspaceMoved {
            name: field(1),
            monitor: field(2),
        },
        "openwindow" => Event::WindowOpened { address: field(0) },
        "closewindow" => Event::WindowClosed { address: field(0) },
        "movewindow" | "movewindowv2" => Event::WindowMoved { address: field(0) },
        "windowtitle" => Event::WindowTitleChanged {
            address: field(0),
            title: None,
        },
        // A title change is the most frequent event a desktop produces; carrying the new title
        // is what keeps it from costing a full state rebuild each time.
        "windowtitlev2" => Event::WindowTitleChanged {
            address: field(0),
            title: Some(data_after_first_comma(data)),
        },
        "monitoradded" | "monitorremoved" | "monitoraddedv2" | "monitorremovedv2" => {
            Event::MonitorsChanged
        }
        _ => return None,
    })
}

/// Window titles may contain commas, so everything after the first separator is the title.
fn data_after_first_comma(data: &str) -> String {
    data.split_once(',')
        .map(|(_, rest)| rest)
        .unwrap_or_default()
        .to_owned()
}

/// The compositor connection has gone. Not an error the application exits on (FR-026a).
#[derive(Debug)]
pub struct Disconnected;

/// The persistent event-socket connection.
pub struct EventStream {
    socket: UnixStream,
    /// Bytes read but not yet terminated by a newline.
    partial: Vec<u8>,
}

impl EventStream {
    /// Open the event socket. Non-blocking, so the event loop owns the waiting.
    ///
    /// # Errors
    /// If the socket cannot be opened or put into non-blocking mode.
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        let socket = UnixStream::connect(path)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            partial: Vec::new(),
        })
    }

    /// Drain whatever the compositor has sent, returning the events it contained.
    ///
    /// # Errors
    /// [`Disconnected`] when the socket reaches end of file or fails — the compositor is gone,
    /// and the caller reconnects with [`Backoff`].
    pub fn drain(&mut self) -> Result<Vec<Event>, Disconnected> {
        let mut buffer = [0u8; 8192];
        loop {
            match self.socket.read(&mut buffer) {
                Ok(0) => return Err(Disconnected),
                Ok(n) => self.partial.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return Err(Disconnected),
            }
        }
        Ok(self.take_events())
    }

    fn take_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            if let Ok(line) = std::str::from_utf8(&line)
                && let Some(event) = parse_line(line)
            {
                events.push(event);
            }
        }
        events
    }
}

impl AsFd for EventStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

/// Exponential reconnect delay: 100 ms doubling to a 5 s cap, indefinitely, reset on success
/// (FR-026a, FR-026d).
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    next: Duration,
}

impl Backoff {
    pub const INITIAL: Duration = Duration::from_millis(100);
    pub const CAP: Duration = Duration::from_secs(5);

    #[must_use]
    pub fn new() -> Self {
        Self {
            next: Self::INITIAL,
        }
    }

    /// The delay before the next attempt, doubling for the attempt after that.
    pub fn take(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(Self::CAP);
        delay
    }

    /// Called after a successful connection, so a compositor that restarts twice is retried as
    /// briskly the second time as the first.
    pub fn reset(&mut self) {
        self.next = Self::INITIAL;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_activation_without_an_id() {
        assert_eq!(
            parse_line("workspace>>3"),
            Some(Event::WorkspaceActivated {
                id: None,
                name: "3".to_owned()
            })
        );
    }

    #[test]
    fn workspace_activation_with_an_id() {
        assert_eq!(
            parse_line("workspacev2>>3,3"),
            Some(Event::WorkspaceActivated {
                id: Some(3),
                name: "3".to_owned()
            })
        );
        assert_eq!(
            parse_line("workspacev2>>7,mail"),
            Some(Event::WorkspaceActivated {
                id: Some(7),
                name: "mail".to_owned()
            })
        );
    }

    #[test]
    fn a_special_workspace_activation_keeps_its_negative_id() {
        assert_eq!(
            parse_line("workspacev2>>-99,special:scratchpad"),
            Some(Event::WorkspaceActivated {
                id: Some(-99),
                name: "special:scratchpad".to_owned()
            })
        );
    }

    #[test]
    fn focused_monitor_carries_the_monitor_and_its_workspace() {
        assert_eq!(
            parse_line("focusedmon>>HEADLESS-2,4"),
            Some(Event::MonitorFocused {
                monitor: "HEADLESS-2".to_owned(),
                workspace_name: "4".to_owned()
            })
        );
    }

    #[test]
    fn workspace_lifecycle_events() {
        assert_eq!(
            parse_line("createworkspace>>5"),
            Some(Event::WorkspaceCreated {
                name: "5".to_owned()
            })
        );
        assert_eq!(
            parse_line("createworkspacev2>>5,5"),
            Some(Event::WorkspaceCreated {
                name: "5".to_owned()
            })
        );
        assert_eq!(
            parse_line("destroyworkspace>>5"),
            Some(Event::WorkspaceDestroyed {
                name: "5".to_owned()
            })
        );
        assert_eq!(
            parse_line("destroyworkspacev2>>5,5"),
            Some(Event::WorkspaceDestroyed {
                name: "5".to_owned()
            })
        );
    }

    #[test]
    fn workspace_moves_carry_the_destination_monitor() {
        assert_eq!(
            parse_line("moveworkspace>>4,HEADLESS-2"),
            Some(Event::WorkspaceMoved {
                name: "4".to_owned(),
                monitor: "HEADLESS-2".to_owned()
            })
        );
        assert_eq!(
            parse_line("moveworkspacev2>>4,4,HEADLESS-2"),
            Some(Event::WorkspaceMoved {
                name: "4".to_owned(),
                monitor: "HEADLESS-2".to_owned()
            })
        );
    }

    #[test]
    fn window_lifecycle_events() {
        assert_eq!(
            parse_line("openwindow>>55a0,3,foot,editor"),
            Some(Event::WindowOpened {
                address: "55a0".to_owned()
            })
        );
        assert_eq!(
            parse_line("closewindow>>55a0"),
            Some(Event::WindowClosed {
                address: "55a0".to_owned()
            })
        );
        assert_eq!(
            parse_line("movewindow>>55a0,3"),
            Some(Event::WindowMoved {
                address: "55a0".to_owned()
            })
        );
        assert_eq!(
            parse_line("movewindowv2>>55a0,3,3"),
            Some(Event::WindowMoved {
                address: "55a0".to_owned()
            })
        );
    }

    #[test]
    fn title_changes_carry_the_new_title_only_in_the_v2_form() {
        assert_eq!(
            parse_line("windowtitle>>55a0"),
            Some(Event::WindowTitleChanged {
                address: "55a0".to_owned(),
                title: None
            })
        );
        assert_eq!(
            parse_line("windowtitlev2>>55a0,vim: main.rs"),
            Some(Event::WindowTitleChanged {
                address: "55a0".to_owned(),
                title: Some("vim: main.rs".to_owned())
            })
        );
    }

    #[test]
    fn a_title_containing_commas_survives_intact() {
        assert_eq!(
            parse_line("windowtitlev2>>55a0,one, two, three"),
            Some(Event::WindowTitleChanged {
                address: "55a0".to_owned(),
                title: Some("one, two, three".to_owned())
            })
        );
    }

    #[test]
    fn monitor_events_all_mean_rebuild() {
        for line in [
            "monitoradded>>HEADLESS-2",
            "monitorremoved>>HEADLESS-2",
            "monitoraddedv2>>1,HEADLESS-2,x",
        ] {
            assert_eq!(parse_line(line), Some(Event::MonitorsChanged), "{line}");
        }
    }

    #[test]
    fn unknown_event_names_are_ignored() {
        // A Hyprland release that adds events must not break this application.
        assert_eq!(parse_line("activelayout>>keyboard,us"), None);
        assert_eq!(parse_line("submap>>resize"), None);
        assert_eq!(parse_line("somethingnew>>whatever,fields"), None);
    }

    #[test]
    fn malformed_lines_are_ignored() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("no separator here"), None);
        assert_eq!(
            parse_line(">>"),
            None,
            "an empty event name matches nothing"
        );
    }

    #[test]
    fn a_trailing_newline_is_stripped() {
        assert_eq!(
            parse_line("workspace>>3\n"),
            Some(Event::WorkspaceActivated {
                id: None,
                name: "3".to_owned()
            })
        );
    }

    #[test]
    fn backoff_doubles_from_100ms_to_a_5s_cap() {
        let mut backoff = Backoff::new();
        let observed: Vec<u128> = (0..9).map(|_| backoff.take().as_millis()).collect();
        assert_eq!(
            observed,
            vec![100, 200, 400, 800, 1600, 3200, 5000, 5000, 5000]
        );
    }

    #[test]
    fn backoff_resets_after_a_successful_connection() {
        let mut backoff = Backoff::new();
        for _ in 0..5 {
            backoff.take();
        }
        backoff.reset();
        assert_eq!(backoff.take(), Backoff::INITIAL);
    }

    #[test]
    fn backoff_never_retries_without_delay() {
        // FR-026d: a compositor that is gone for good must not cost CPU.
        let mut backoff = Backoff::new();
        for _ in 0..100 {
            let delay = backoff.take();
            assert!(delay >= Backoff::INITIAL, "{delay:?}");
            assert!(delay <= Backoff::CAP, "{delay:?}");
        }
    }
}
