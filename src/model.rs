//! Compositor entities, as reported by Hyprland's `j/monitors`, `j/workspaces` and `j/clients`.
//!
//! Everything here is a projection of compositor state; this application owns none of it.
//! The deserialisers accept the compositor's JSON verbatim and ignore fields the feature does
//! not use, so a Hyprland release that adds fields is non-breaking.

use serde::Deserialize;

use crate::SUPPORTED_HYPRLAND;

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

// --- The compositor's own version (FR-118, research.md R42) -----------------

/// Hyprland's `j/version` response, of which exactly two fields are read.
///
/// The rest of that response — the commit, the build's library versions, the ABI hash — is
/// deliberately ignored: the daemon asks this question once, at start-up, to answer FR-118, and
/// anything more would be state this application has no use for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CompositorVersion {
    /// e.g. `0.56.2`.
    pub version: String,
    /// e.g. `v0.56.2`. Carried for the `--environment` report only, and absent on builds that
    /// were not made from a tag.
    #[serde(default)]
    pub tag: Option<String>,
}

/// How a reported version stands against [`SUPPORTED_HYPRLAND`].
///
/// Three outcomes rather than a boolean because the two failures are reported differently: one
/// names a version that is too old, the other names a string that could not be read at all
/// (`contracts/diagnostics.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support<'a> {
    /// At or above the minimum. There is no maximum — a newer compositor is assumed to work.
    Supported,
    TooOld {
        found: (u32, u32, u32),
        minimum: (u32, u32),
    },
    /// The version string is not a version this application can compare. Borrowed rather than
    /// owned so the record quotes exactly what the compositor said, without copying it.
    Unknown { found: &'a str },
}

impl CompositorVersion {
    /// `MAJOR.MINOR[.PATCH]`, with an optional `v` prefix and any trailing suffix ignored.
    ///
    /// Pure and total: `None` for anything that does not begin with two dot-separated numbers,
    /// which is what makes the "could not be read" branch of FR-118 a decision rather than a
    /// guess. The leniency is deliberate — `0.56.2`, `v0.56.2`, `0.56.2-dirty` and `0.55` are all
    /// versions a Hyprland build really reports, and none of them should cost the user a warning.
    #[must_use]
    pub fn parse(text: &str) -> Option<(u32, u32, u32)> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        // The suffix begins at the first character that is neither a digit nor a separator, so
        // `-dirty`, `rc1` and `+build` all fall off together.
        let end = text
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(text.len());
        let mut parts = text[..end].split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        // A two-component version is a real form; a fourth component is more trailing suffix.
        let patch = match parts.next() {
            Some(patch) => patch.parse().ok()?,
            None => 0,
        };
        Some((major, minor, patch))
    }

    /// Whether this compositor is inside the supported range (FR-118).
    ///
    /// The patch level is parsed but not compared: [`SUPPORTED_HYPRLAND`] is a `MAJOR.MINOR`
    /// minimum, because that is the granularity at which Hyprland's protocol surface moves.
    #[must_use]
    pub fn supported(&self) -> Support<'_> {
        let minimum = SUPPORTED_HYPRLAND.minimum;
        match Self::parse(&self.version) {
            None => Support::Unknown {
                found: &self.version,
            },
            Some((major, minor, _patch)) if (major, minor) >= minimum => Support::Supported,
            Some(found) => Support::TooOld { found, minimum },
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

    // T094 — the compositor version (FR-118, research.md R42).

    #[test]
    fn the_version_response_reads_only_the_two_fields_it_needs() {
        // The real `j/version` reply, abbreviated but shaped exactly as the running compositor
        // sends it: every other field is ignored, as it is everywhere else in this module.
        let json = r#"{"branch":"v0.56.2","commit":"efb5099","version":"0.56.2",
                       "dirty":false,"tag":"v0.56.2","commits":"7661",
                       "buildAquamarine":"0.15.0"}"#;
        let parsed: CompositorVersion = serde_json::from_str(json).expect("deserialises");
        assert_eq!(parsed.version, "0.56.2");
        assert_eq!(parsed.tag.as_deref(), Some("v0.56.2"));

        // A build made from no tag reports none, and that is not a parse failure.
        let untagged: CompositorVersion =
            serde_json::from_str(r#"{"version":"0.57.0"}"#).expect("deserialises");
        assert_eq!(untagged.tag, None);
    }

    #[test]
    fn a_version_parses_in_every_form_a_hyprland_build_reports() {
        // The plain form, the `v` prefix, a two-component version, and the suffixes a build from
        // a working tree or a pre-release carries.
        assert_eq!(CompositorVersion::parse("0.56.2"), Some((0, 56, 2)));
        assert_eq!(CompositorVersion::parse("v0.56.2"), Some((0, 56, 2)));
        assert_eq!(CompositorVersion::parse("0.55"), Some((0, 55, 0)));
        assert_eq!(CompositorVersion::parse("v0.55"), Some((0, 55, 0)));
        assert_eq!(CompositorVersion::parse("0.56.2-dirty"), Some((0, 56, 2)));
        assert_eq!(CompositorVersion::parse("0.57.0-rc1"), Some((0, 57, 0)));
        assert_eq!(CompositorVersion::parse("1.0.0+build.7"), Some((1, 0, 0)));
        // A fourth component is more trailing suffix, not a different version.
        assert_eq!(CompositorVersion::parse("0.56.2.1"), Some((0, 56, 2)));
        // Whitespace around the value is the compositor's formatting, not part of the version.
        assert_eq!(CompositorVersion::parse("  0.56.2\n"), Some((0, 56, 2)));
    }

    #[test]
    fn anything_that_is_not_two_numbers_is_not_a_version() {
        // `None` is what puts FR-118's "could not be read" branch on stderr, so each of these is
        // a way a real build could answer that this application must not guess about.
        for text in ["", "next", "v", "0", "0.", ".5", "v.", "unknown", "0.x.2"] {
            assert_eq!(
                CompositorVersion::parse(text),
                None,
                "{text:?} is not a version"
            );
        }
    }

    #[test]
    fn support_is_decided_against_the_one_published_range() {
        let at = |version: &str| CompositorVersion {
            version: version.to_owned(),
            tag: None,
        };
        let (major, minor) = SUPPORTED_HYPRLAND.minimum;

        // Exactly the minimum is supported, and so is everything above it — there is no maximum.
        assert_eq!(at("0.55").supported(), Support::Supported);
        assert_eq!(at("0.55.0").supported(), Support::Supported);
        assert_eq!(at("v0.56.2").supported(), Support::Supported);
        assert_eq!(at("1.0.0").supported(), Support::Supported);

        // Below it, the record names the version found and the minimum it was measured against.
        assert_eq!(
            at("0.52.1").supported(),
            Support::TooOld {
                found: (0, 52, 1),
                minimum: (major, minor),
            }
        );
        assert_eq!(
            at("0.54.9").supported(),
            Support::TooOld {
                found: (0, 54, 9),
                minimum: (major, minor),
            }
        );

        // The patch level is not compared: the range is a MAJOR.MINOR minimum, so a hypothetical
        // `0.55` with any patch is inside it.
        assert_eq!(at("0.55.99").supported(), Support::Supported);

        // And an unreadable version quotes back exactly what the compositor said.
        assert_eq!(at("next").supported(), Support::Unknown { found: "next" });
    }
}
