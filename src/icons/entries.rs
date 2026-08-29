//! The desktop-entry index, and the rule that decides which entry owns a window class.
//!
//! Only four keys are read per entry — `Icon`, `StartupWMClass`, `Name` and `NoDisplay` — so this
//! is a minimal INI reader rather than a dependency (research.md R21). The matching ladder is the
//! five ordered steps in `contracts/icon-lookup.md`, expressed as a pure function so it is
//! testable without a filesystem (FR-040).

use std::path::{Path, PathBuf};

/// The group every key below is read from. Anything in a later group — an action, a locale
/// section — is not part of the entry proper and is skipped.
const GROUP: &str = "[Desktop Entry]";

/// The four keys of one desktop entry, plus the id its filename gives it.
///
/// Everything else in the file is dropped at parse time rather than carried and ignored: the
/// index is built for 143 entries at start-up (research.md R21) and holding whole files would be
/// paying for `Exec`, `Comment` and every localised `Name` that no lookup ever reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// The basename without `.desktop` — `org.gnome.Nautilus` for `org.gnome.Nautilus.desktop`.
    pub id: String,
    /// The `Icon` key: a bare name to look up in the icon set, or an absolute path.
    pub icon: String,
    /// The toplevel class this entry claims, if it claims one.
    pub startup_wm_class: Option<String>,
    /// The unlocalised `Name`. Localised variants (`Name[de]`) are deliberately not read: the
    /// class a compositor reports is not localised either.
    pub name: Option<String>,
    /// `NoDisplay=true`, which ranks the entry behind every visible one.
    pub no_display: bool,
}

impl DesktopEntry {
    /// Read one entry's four keys out of its file contents.
    ///
    /// `None` when the file carries no `Icon`: an entry that names no icon can never be the
    /// answer to "which icon belongs to this window", so letting it into the index would only let
    /// it shadow an entry that could have answered.
    #[must_use]
    pub fn parse(id: &str, text: &str) -> Option<Self> {
        let mut entry = Self {
            id: id.to_owned(),
            icon: String::new(),
            startup_wm_class: None,
            name: None,
            no_display: false,
        };

        let mut inside = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                // The desktop-entry format has no nesting: the group runs until the next header,
                // so the first one after ours ends the part we read.
                if inside {
                    break;
                }
                inside = line == GROUP;
                continue;
            }
            if !inside || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "Icon" => value.clone_into(&mut entry.icon),
                "StartupWMClass" => entry.startup_wm_class = Some(value.to_owned()),
                "Name" => entry.name = Some(value.to_owned()),
                "NoDisplay" => entry.no_display = value.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }

        (!entry.icon.is_empty()).then_some(entry)
    }

    /// Which step of the ladder this entry answers `class` on, or `None` for no match at all.
    ///
    /// The five steps of `contracts/icon-lookup.md`, lowest number first. Kept as a number rather
    /// than an early return so the caller can rank a whole index in one pass and apply the
    /// `NoDisplay` rule across it.
    #[must_use]
    fn step_for(&self, class: &str) -> Option<u8> {
        if self.startup_wm_class.as_deref() == Some(class) {
            return Some(1);
        }
        if self
            .startup_wm_class
            .as_deref()
            .is_some_and(|claimed| claimed.eq_ignore_ascii_case(class))
        {
            return Some(2);
        }
        if self.id.eq_ignore_ascii_case(class) {
            return Some(3);
        }
        // Reverse-DNS ids are the common case for a Flatpak or a GNOME application, and their
        // last component is what the compositor reports: `org.gnome.Nautilus` ← `nautilus`.
        if self
            .id
            .rsplit('.')
            .next()
            .is_some_and(|tail| tail.eq_ignore_ascii_case(class))
        {
            return Some(4);
        }
        if self
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(class))
        {
            return Some(5);
        }
        None
    }
}

/// Every desktop entry the search path holds, indexed once at start-up.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Index {
    entries: Vec<DesktopEntry>,
}

impl Index {
    /// Build an index straight from entries, for tests and for callers that have already read
    /// them. The order given is the order ties are broken in.
    #[must_use]
    pub fn new(entries: Vec<DesktopEntry>) -> Self {
        Self { entries }
    }

    /// Scan `applications` under each root, in order, for `.desktop` files.
    ///
    /// The roots are `$XDG_DATA_HOME` then each `$XDG_DATA_DIRS` entry
    /// (`contracts/icon-lookup.md`). An id found in an earlier root wins, which is the
    /// freedesktop precedence rule: a user's own override in `~/.local/share` shadows the
    /// packaged entry of the same name rather than competing with it.
    ///
    /// Unreadable directories and unreadable files are skipped in silence. A missing
    /// `applications` directory is the normal state of most roots, and an entry we cannot read is
    /// one program without its own icon — FR-041's placeholder, not a failure worth reporting.
    #[must_use]
    pub fn scan(roots: &[PathBuf]) -> Self {
        let mut entries: Vec<DesktopEntry> = Vec::new();
        for root in roots {
            for (id, text) in read_directory(&root.join("applications")) {
                if entries.iter().any(|existing| existing.id == id) {
                    continue;
                }
                if let Some(entry) = DesktopEntry::parse(&id, &text) {
                    entries.push(entry);
                }
            }
        }
        Self { entries }
    }

    /// The icon name a window class resolves to, or `None` when nothing claims it (FR-040).
    ///
    /// Pure: the whole ladder is decided from the parsed index, which is what lets every step
    /// below be a unit test rather than a filesystem fixture.
    ///
    /// Two keys rank a candidate, in this order:
    ///
    /// 1. **Visible before hidden.** `NoDisplay=true` entries are indexed but rank behind every
    ///    visible one, however good their match — "a real launcher beats a hidden one"
    ///    (`contracts/icon-lookup.md`). A hidden entry claiming the class outright still loses to
    ///    a visible entry matched only by name, because the hidden one is by definition not the
    ///    launcher the user sees this program as.
    /// 2. **The lower step.** Within each group, the first step of the ladder that hits wins, and
    ///    within a step the entry the search path reached first.
    #[must_use]
    pub fn icon_for(&self, class: &str) -> Option<&str> {
        self.entries
            .iter()
            .filter_map(|entry| entry.step_for(class).map(|step| (entry, step)))
            .min_by_key(|(entry, step)| (u8::from(entry.no_display), *step))
            .map(|(entry, _)| entry.icon.as_str())
    }

    /// How many entries were indexed — the one thing about the index worth reporting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Every `(id, contents)` pair in one `applications` directory, sorted by filename.
///
/// Sorted so the index is the same on every run: `read_dir` order is the filesystem's and varies
/// between machines, which would make a tie between two entries in one directory resolve
/// differently on different systems.
fn read_directory(directory: &Path) -> Vec<(String, String)> {
    let Ok(reader) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = reader
        .flatten()
        .map(|found| found.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
        .collect();
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_owned();
            let text = std::fs::read_to_string(&path).ok()?;
            Some((id, text))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DesktopEntry, Index};

    /// One entry, spelled the way a real `.desktop` file spells it.
    fn entry(id: &str, class: Option<&str>, name: Option<&str>, icon: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.to_owned(),
            icon: icon.to_owned(),
            startup_wm_class: class.map(ToOwned::to_owned),
            name: name.map(ToOwned::to_owned),
            no_display: false,
        }
    }

    fn hidden(mut entry: DesktopEntry) -> DesktopEntry {
        entry.no_display = true;
        entry
    }

    // --- T026: the minimal INI reader (research.md R21) ----------------------

    #[test]
    fn parsing_reads_exactly_the_four_keys_it_needs() {
        let parsed = DesktopEntry::parse(
            "foot",
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Foot\n\
             Comment=A terminal\n\
             Exec=/usr/bin/foot\n\
             Icon=foot\n\
             StartupWMClass=foot\n\
             NoDisplay=false\n",
        )
        .expect("an entry with an Icon is indexed");

        assert_eq!(parsed.id, "foot");
        assert_eq!(parsed.icon, "foot");
        assert_eq!(parsed.startup_wm_class.as_deref(), Some("foot"));
        assert_eq!(parsed.name.as_deref(), Some("Foot"));
        assert!(!parsed.no_display);
    }

    #[test]
    fn an_entry_without_an_icon_is_not_indexed_at_all() {
        // It could never answer "which icon belongs to this window", so admitting it would only
        // let it shadow an entry that could.
        assert_eq!(
            DesktopEntry::parse("nothing", "[Desktop Entry]\nName=Nothing\nExec=/bin/true\n"),
            None
        );
        assert_eq!(
            DesktopEntry::parse("blank", "[Desktop Entry]\nIcon=\nName=Blank\n"),
            None
        );
    }

    #[test]
    fn keys_outside_the_desktop_entry_group_are_not_read() {
        // Actions carry their own `Icon` and `Name`, and reading them would give the entry the
        // icon of one of its right-click menu items.
        let parsed = DesktopEntry::parse(
            "browser",
            "[Desktop Entry]\n\
             Name=Browser\n\
             Icon=browser\n\
             \n\
             [Desktop Action new-private-window]\n\
             Name=Private Window\n\
             Icon=browser-private\n",
        )
        .expect("the entry itself has an Icon");

        assert_eq!(parsed.icon, "browser");
        assert_eq!(parsed.name.as_deref(), Some("Browser"));
    }

    #[test]
    fn a_file_whose_first_group_is_not_the_desktop_entry_group_is_still_read() {
        let parsed = DesktopEntry::parse(
            "late",
            "[Some Other Group]\nIcon=wrong\n\n[Desktop Entry]\nIcon=right\n",
        )
        .expect("the Desktop Entry group is found wherever it sits");
        assert_eq!(parsed.icon, "right");
    }

    #[test]
    fn localised_names_and_comments_are_ignored() {
        let parsed = DesktopEntry::parse(
            "files",
            "[Desktop Entry]\n\
             # a comment\n\
             Name=Files\n\
             Name[de]=Dateien\n\
             Icon=org.gnome.Nautilus\n",
        )
        .expect("indexed");
        // The compositor reports an unlocalised class, so a localised name could only ever
        // produce a wrong match.
        assert_eq!(parsed.name.as_deref(), Some("Files"));
    }

    #[test]
    fn no_display_is_read_case_insensitively_and_defaults_to_false() {
        let hidden =
            DesktopEntry::parse("h", "[Desktop Entry]\nIcon=h\nNoDisplay=True\n").expect("indexed");
        assert!(hidden.no_display);

        let shown = DesktopEntry::parse("s", "[Desktop Entry]\nIcon=s\n").expect("indexed");
        assert!(!shown.no_display, "absent NoDisplay means visible");
    }

    #[test]
    fn a_file_that_is_not_a_desktop_entry_at_all_yields_nothing() {
        assert_eq!(DesktopEntry::parse("junk", ""), None);
        assert_eq!(DesktopEntry::parse("junk", "not an ini file"), None);
    }

    // --- T028: the matching ladder, one test per step (FR-040) ---------------

    #[test]
    fn step_one_is_startup_wm_class_matched_exactly() {
        let index = Index::new(vec![entry("foot", Some("foot"), None, "foot-icon")]);
        assert_eq!(index.icon_for("foot"), Some("foot-icon"));
    }

    #[test]
    fn step_two_is_startup_wm_class_matched_case_insensitively() {
        let index = Index::new(vec![entry("foot", Some("Foot"), None, "foot-icon")]);
        assert_eq!(index.icon_for("foot"), Some("foot-icon"));
    }

    #[test]
    fn step_three_is_the_entry_id_matched_case_insensitively() {
        let index = Index::new(vec![entry("firefox", None, None, "firefox-icon")]);
        assert_eq!(index.icon_for("Firefox"), Some("firefox-icon"));
    }

    #[test]
    fn step_four_is_the_last_component_of_a_reverse_dns_id() {
        // The case R21 calls out: GNOME and Flatpak entries are named this way, and the
        // compositor reports only the tail.
        let index = Index::new(vec![entry(
            "org.gnome.Nautilus",
            None,
            None,
            "org.gnome.Nautilus",
        )]);
        assert_eq!(index.icon_for("nautilus"), Some("org.gnome.Nautilus"));
    }

    #[test]
    fn step_five_is_the_name_matched_case_insensitively() {
        let index = Index::new(vec![entry("term", None, Some("Foot"), "term-icon")]);
        assert_eq!(index.icon_for("foot"), Some("term-icon"));
    }

    #[test]
    fn no_match_yields_none_which_is_the_placeholder() {
        // FR-041: a normal outcome, not a failure. The store turns this into the placeholder.
        let index = Index::new(vec![entry("foot", Some("foot"), Some("Foot"), "foot-icon")]);
        assert_eq!(index.icon_for("nobody"), None);
        assert_eq!(Index::default().icon_for("foot"), None);
    }

    #[test]
    fn an_earlier_step_wins_over_a_later_one() {
        let index = Index::new(vec![
            // Would match at step 5 by name.
            entry("impostor", None, Some("foot"), "wrong"),
            // Matches at step 1 by the class it explicitly claims.
            entry("foot", Some("foot"), None, "right"),
        ]);
        assert_eq!(
            index.icon_for("foot"),
            Some("right"),
            "the ladder is ordered by rule, not by index position"
        );
    }

    #[test]
    fn an_exact_class_match_beats_a_case_insensitive_one() {
        let index = Index::new(vec![
            entry("loose", Some("Foot"), None, "loose"),
            entry("exact", Some("foot"), None, "exact"),
        ]);
        assert_eq!(index.icon_for("foot"), Some("exact"));
    }

    #[test]
    fn within_one_step_the_first_entry_in_search_order_wins() {
        let index = Index::new(vec![
            entry("first", Some("foot"), None, "first"),
            entry("second", Some("foot"), None, "second"),
        ]);
        assert_eq!(
            index.icon_for("foot"),
            Some("first"),
            "search-path order breaks a tie, so ~/.local/share shadows /usr/share"
        );
    }

    #[test]
    fn a_hidden_entry_ranks_behind_every_visible_one() {
        // "A real launcher beats a hidden one": the hidden entry here has the *better* match —
        // step 1 against the visible entry's step 5 — and still loses.
        let index = Index::new(vec![
            hidden(entry("hidden", Some("foot"), None, "hidden")),
            entry("shown", None, Some("Foot"), "shown"),
        ]);
        assert_eq!(index.icon_for("foot"), Some("shown"));
    }

    #[test]
    fn a_hidden_entry_still_answers_when_nothing_visible_matches() {
        let index = Index::new(vec![hidden(entry("hidden", Some("foot"), None, "hidden"))]);
        assert_eq!(
            index.icon_for("foot"),
            Some("hidden"),
            "ranked last is indexed, not excluded"
        );
    }

    #[test]
    fn hidden_entries_rank_among_themselves_by_the_same_ladder() {
        let index = Index::new(vec![
            hidden(entry("byname", None, Some("foot"), "byname")),
            hidden(entry("byclass", Some("foot"), None, "byclass")),
        ]);
        assert_eq!(index.icon_for("foot"), Some("byclass"));
    }
}
