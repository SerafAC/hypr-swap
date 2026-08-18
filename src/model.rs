//! Compositor entities, as reported by Hyprland's `j/monitors`, `j/workspaces` and `j/clients`.
//!
//! Everything here is a projection of compositor state; this application owns none of it.
//! The deserialisers accept the compositor's JSON verbatim and ignore fields the feature does
//! not use, so a Hyprland release that adds fields is non-breaking.

use serde::Deserialize;

/// A monitor's connector name, e.g. `eDP-1` or `HEADLESS-2`. Identity for monitors everywhere.
pub type MonitorName = String;

/// An ordinary compositor workspace.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    pub monitor: MonitorName,
    /// `j/workspaces[].windows` — the FR-021 emptiness check reads this.
    #[serde(rename = "windows")]
    pub window_count: u32,
}

impl Workspace {
    /// Special and scratchpad workspaces carry a negative id. They are excluded from the overlay
    /// and never moved between monitors (FR-007).
    ///
    /// This is the single place that rule is expressed.
    #[must_use]
    pub fn is_special(&self) -> bool {
        self.id < 0
    }
}

/// A connected display.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawMonitor")]
pub struct Monitor {
    pub id: i32,
    pub name: MonitorName,
    /// Layout coordinates of the monitor's top-left corner, in pixels.
    pub position: (i32, i32),
    /// Monitor size in pixels (not logical units — divide by `scale` for those).
    pub size: (u32, u32),
    pub scale: f32,
    /// Every monitor has exactly one active workspace at all times.
    pub active_workspace: i32,
    /// At most one monitor is focused.
    pub focused: bool,
}

/// An application window.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawWindow")]
pub struct Window {
    pub address: String,
    pub title: String,
    /// Fallback label when `title` is empty.
    pub class: String,
    pub workspace: i32,
    /// Layout coordinates, in pixels — global, so miniature geometry subtracts the monitor origin.
    pub at: (i32, i32),
    pub size: (u32, u32),
    /// Painted above tiled windows in miniatures.
    pub floating: bool,
    pub mapped: bool,
}

impl Window {
    /// Unmapped windows are excluded from both presentations.
    #[must_use]
    pub fn is_listed(&self) -> bool {
        self.mapped
    }

    /// A window with zero width or height is skipped rather than drawn as a degenerate rectangle.
    #[must_use]
    pub fn has_area(&self) -> bool {
        self.size.0 > 0 && self.size.1 > 0
    }

    /// The label shown in both presentations: the title, or the class when the title is empty.
    #[must_use]
    pub fn label(&self) -> &str {
        if self.title.is_empty() {
            &self.class
        } else {
            &self.title
        }
    }
}

// ---------------------------------------------------------------------------
// Wire shapes. Private: the compositor's JSON layout is not part of any module's interface.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WorkspaceRef {
    id: i32,
}

#[derive(Deserialize)]
struct RawMonitor {
    id: i32,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f32,
    #[serde(rename = "activeWorkspace")]
    active_workspace: WorkspaceRef,
    focused: bool,
}

impl From<RawMonitor> for Monitor {
    fn from(raw: RawMonitor) -> Self {
        Self {
            id: raw.id,
            name: raw.name,
            position: (raw.x, raw.y),
            size: (raw.width, raw.height),
            scale: raw.scale,
            active_workspace: raw.active_workspace.id,
            focused: raw.focused,
        }
    }
}

#[derive(Deserialize)]
struct RawWindow {
    address: String,
    title: String,
    class: String,
    workspace: WorkspaceRef,
    at: [i32; 2],
    size: [i32; 2],
    floating: bool,
    mapped: bool,
}

impl From<RawWindow> for Window {
    fn from(raw: RawWindow) -> Self {
        Self {
            address: raw.address,
            title: raw.title,
            class: raw.class,
            workspace: raw.workspace.id,
            at: (raw.at[0], raw.at[1]),
            // The compositor reports sizes as signed; a negative one is nonsense and would only
            // produce a degenerate rectangle, so it collapses to zero and `has_area` rejects it.
            size: (
                u32::try_from(raw.size[0]).unwrap_or(0),
                u32::try_from(raw.size[1]).unwrap_or(0),
            ),
            floating: raw.floating,
            mapped: raw.mapped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
        std::fs::read_to_string(format!("{path}{name}"))
            .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn monitors() -> Vec<Monitor> {
        serde_json::from_str(&fixture("monitors.json")).expect("monitors.json deserialises")
    }

    fn workspaces() -> Vec<Workspace> {
        serde_json::from_str(&fixture("workspaces.json")).expect("workspaces.json deserialises")
    }

    fn windows() -> Vec<Window> {
        serde_json::from_str(&fixture("clients.json")).expect("clients.json deserialises")
    }

    #[test]
    fn monitors_deserialise_position_size_and_active_workspace() {
        let mons = monitors();
        assert_eq!(mons.len(), 2);

        let edp = &mons[0];
        assert_eq!(edp.name, "eDP-1");
        assert_eq!(edp.id, 0);
        assert_eq!(edp.position, (0, 0));
        assert_eq!(edp.size, (2880, 1800));
        assert!((edp.scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(edp.active_workspace, 1);
        assert!(edp.focused);

        let headless = &mons[1];
        assert_eq!(headless.name, "HEADLESS-2");
        assert_eq!(headless.position, (2880, 0));
        assert_eq!(headless.size, (1920, 1080));
        assert_eq!(headless.active_workspace, 2);
        assert!(!headless.focused);
    }

    #[test]
    fn exactly_one_monitor_is_focused() {
        assert_eq!(monitors().iter().filter(|m| m.focused).count(), 1);
    }

    #[test]
    fn workspaces_deserialise_with_window_count_and_monitor_binding() {
        let all = workspaces();
        assert_eq!(all.len(), 4);

        let first = &all[0];
        assert_eq!(first.id, 1);
        assert_eq!(first.name, "1");
        assert_eq!(first.monitor, "eDP-1");
        assert_eq!(first.window_count, 2);

        let named = all.iter().find(|w| w.id == 4).expect("workspace 4 present");
        assert_eq!(
            named.name, "mail",
            "named workspaces report their name, not their number"
        );
        assert_eq!(
            named.window_count, 0,
            "an empty workspace is still a workspace (FR-007)"
        );
    }

    #[test]
    fn negative_ids_are_special_workspaces() {
        let all = workspaces();
        let special: Vec<_> = all.iter().filter(|w| w.is_special()).collect();
        assert_eq!(special.len(), 1);
        assert_eq!(special[0].id, -99);
        assert_eq!(special[0].name, "special:scratchpad");

        for ordinary in all.iter().filter(|w| !w.is_special()) {
            assert!(
                ordinary.id > 0,
                "ordinary workspace {} should be positive",
                ordinary.id
            );
        }
    }

    #[test]
    fn windows_deserialise_geometry_workspace_and_flags() {
        let all = windows();
        assert_eq!(all.len(), 5);

        let editor = &all[0];
        assert_eq!(editor.address, "0x55a0");
        assert_eq!(editor.title, "editor");
        assert_eq!(editor.class, "foot");
        assert_eq!(editor.workspace, 1);
        assert_eq!(editor.at, (0, 0));
        assert_eq!(editor.size, (1440, 1800));
        assert!(!editor.floating);
        assert!(editor.mapped);

        assert!(all[1].floating, "the notes window is floating");
        assert_eq!(
            all[2].at,
            (2880, 0),
            "geometry is global, not monitor-relative"
        );
    }

    #[test]
    fn unmapped_windows_are_excluded_from_both_presentations() {
        let all = windows();
        let ghost = all
            .iter()
            .find(|w| w.address == "0x55a4")
            .expect("unmapped window present");
        assert!(!ghost.is_listed());
        assert!(
            ghost.has_area(),
            "it has a size; it is excluded for being unmapped, not for that"
        );
    }

    #[test]
    fn zero_size_windows_are_skipped_in_miniatures_but_are_mapped() {
        let all = windows();
        let degenerate = all
            .iter()
            .find(|w| w.address == "0x55a5")
            .expect("zero-size window present");
        assert!(
            degenerate.is_listed(),
            "a mapped zero-size window is still a real window"
        );
        assert!(
            !degenerate.has_area(),
            "it must not be drawn as a degenerate rectangle"
        );
    }

    #[test]
    fn label_falls_back_to_class_when_the_title_is_empty() {
        let all = windows();
        assert_eq!(all[0].label(), "editor");
        let degenerate = all
            .iter()
            .find(|w| w.title.is_empty())
            .expect("empty-title window");
        assert_eq!(degenerate.label(), "degenerate");
    }

    #[test]
    fn unknown_fields_are_ignored_so_new_hyprland_releases_do_not_break_parsing() {
        let json = r#"[{"id":9,"name":"9","monitor":"eDP-1","monitorID":0,"windows":3,
                        "somethingHyprlandAddedLater":true}]"#;
        let parsed: Vec<Workspace> = serde_json::from_str(json).expect("tolerates unknown fields");
        assert_eq!(parsed[0].id, 9);
        assert_eq!(parsed[0].window_count, 3);
    }

    #[test]
    fn a_negative_reported_size_collapses_to_zero_area() {
        let json = r#"[{"address":"0x1","mapped":true,"at":[0,0],"size":[-1,10],
                        "workspace":{"id":1,"name":"1"},"floating":false,
                        "class":"c","title":"t"}]"#;
        let parsed: Vec<Window> = serde_json::from_str(json).expect("deserialises");
        assert_eq!(parsed[0].size, (0, 10));
        assert!(!parsed[0].has_area());
    }
}
