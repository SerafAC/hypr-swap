//! The freedesktop icon-set lookup: which file on disk is a given icon name at a given size.
//!
//! Implemented directly rather than taken as a dependency (research.md R20): the search path, the
//! `index.theme` directory list with each directory's `Size`, `Scale`, `Type`, `MinSize`,
//! `MaxSize` and `Threshold`, and `Inherits` followed in order until the standard default set.
//!
//! Directory choice for a requested size is a pure function over that parsed metadata, so the
//! scoring rule is unit-testable on its own (FR-040).
//!
//! Note the vocabulary, which the spec keeps distinct: this is the *icon set* (FR-057), not the
//! overlay theme in [`crate::theme`]. The two are independent settings.

use std::path::{Path, PathBuf};

/// The set every inheritance chain ends at, by specification. It is the one set a freedesktop
/// desktop is required to have, which is what makes it a safe terminator.
pub const DEFAULT_SET: &str = "hicolor";

/// The file extensions this application can decode, in the order the specification prefers them
/// (FR-040a). Anything else in a set is invisible to the lookup, which FR-040a defines as
/// unresolvable rather than as an error.
const EXTENSIONS: [&str; 2] = ["png", "svg"];

/// How a directory's `Size` relates to the sizes it actually holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Exactly `Size`, and nothing else.
    Fixed,
    /// Anything between `MinSize` and `MaxSize` — a directory of vector icons.
    Scalable,
    /// `Size`, give or take `Threshold`. The specification's default when `Type` is absent.
    #[default]
    Threshold,
}

/// One directory listed in a set's `index.theme`, with the metadata that decides whether it can
/// answer a request for a given size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    /// Relative to the set's root — `48x48/apps`, `scalable/apps`.
    pub path: String,
    pub size: u32,
    /// The monitor scale the directory's icons are drawn for. `1` unless the set ships `@2x`
    /// variants.
    pub scale: u32,
    pub kind: Kind,
    pub min_size: u32,
    pub max_size: u32,
    pub threshold: u32,
}

impl Directory {
    /// The specification's defaults for a directory that declares only its `Size`.
    #[must_use]
    pub fn of_size(path: &str, size: u32) -> Self {
        Self {
            path: path.to_owned(),
            size,
            scale: 1,
            kind: Kind::Threshold,
            min_size: size,
            max_size: size,
            threshold: 2,
        }
    }

    /// Whether this directory holds the requested size outright — the specification's
    /// `DirectoryMatchesSize`.
    #[must_use]
    pub fn matches(&self, size: u32, scale: u32) -> bool {
        if self.scale != scale {
            return false;
        }
        match self.kind {
            Kind::Fixed => self.size == size,
            Kind::Scalable => self.min_size <= size && size <= self.max_size,
            Kind::Threshold => {
                self.size.saturating_sub(self.threshold) <= size
                    && size <= self.size + self.threshold
            }
        }
    }

    /// How far this directory is from the requested size — the specification's
    /// `DirectorySizeDistance`, used to pick the least-bad directory when none matches outright.
    ///
    /// Sizes are compared in device pixels, i.e. multiplied by their scale, so a `@2x` directory
    /// of 24-pixel icons is correctly seen as holding 48-pixel artwork.
    #[must_use]
    pub fn distance(&self, size: u32, scale: u32) -> u32 {
        let wanted = size * scale;
        let gap = |bound: u32| bound.abs_diff(wanted);
        match self.kind {
            Kind::Fixed => gap(self.size * self.scale),
            Kind::Scalable => {
                if wanted < self.min_size * self.scale {
                    gap(self.min_size * self.scale)
                } else if wanted > self.max_size * self.scale {
                    gap(self.max_size * self.scale)
                } else {
                    0
                }
            }
            Kind::Threshold => {
                let low = self.size.saturating_sub(self.threshold) * self.scale;
                let high = (self.size + self.threshold) * self.scale;
                if wanted < low {
                    gap(low)
                } else if wanted > high {
                    gap(high)
                } else {
                    0
                }
            }
        }
    }
}

/// One set's `index.theme`, parsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetIndex {
    pub directories: Vec<Directory>,
    /// The sets to fall back to, in the order they are listed.
    pub inherits: Vec<String>,
}

impl SetIndex {
    /// Parse an `index.theme`.
    ///
    /// Total, by design: every field has a specified default and a malformed value simply takes
    /// it, so a set with a damaged or unusual `index.theme` degrades to "holds nothing we can
    /// find" instead of taking the daemon down (FR-044's spirit, applied to the set rather than
    /// to one file). Only directories that are actually listed in `Directories` are read, which
    /// is what the specification says and also what stops a stray group from becoming one.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut groups = parse_ini(text);

        let header = groups.iter().find(|(name, _)| name == "Icon Theme");
        let listed: Vec<String> = header
            .and_then(|(_, keys)| lookup_key(keys, "Directories"))
            .into_iter()
            .chain(header.and_then(|(_, keys)| lookup_key(keys, "ScaledDirectories")))
            .flat_map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        let inherits = header
            .and_then(|(_, keys)| lookup_key(keys, "Inherits"))
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let directories = listed
            .into_iter()
            .filter_map(|path| {
                let keys = groups
                    .iter_mut()
                    .find(|(name, _)| *name == path)
                    .map(|(_, keys)| &*keys)?;
                let number = |key: &str| lookup_key(keys, key).and_then(|v| v.parse::<u32>().ok());
                // A directory with no readable `Size` has nothing to be scored against, so it is
                // dropped rather than admitted as size zero, which would match nothing anyway
                // and would sit at the top of every distance ranking.
                let size = number("Size")?;
                Some(Directory {
                    path,
                    size,
                    // Zero would divide the scoring arithmetic by nothing useful and no real set
                    // writes it; the specified default is 1.
                    scale: number("Scale").filter(|scale| *scale > 0).unwrap_or(1),
                    kind: match lookup_key(keys, "Type") {
                        Some(value) if value.eq_ignore_ascii_case("fixed") => Kind::Fixed,
                        Some(value) if value.eq_ignore_ascii_case("scalable") => Kind::Scalable,
                        // Including an unrecognised `Type`: the specification's default.
                        _ => Kind::Threshold,
                    },
                    min_size: number("MinSize").unwrap_or(size),
                    max_size: number("MaxSize").unwrap_or(size),
                    threshold: number("Threshold").unwrap_or(2),
                })
            })
            .collect();

        Self {
            directories,
            inherits,
        }
    }
}

/// One set in a resolved inheritance chain: its name, the roots that hold a copy of it, and its
/// directories.
///
/// A set can be installed in more than one root — a user's own `~/.icons/Papirus` extending the
/// packaged one — and the specification searches all of them for each directory, in search-path
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Level {
    name: String,
    roots: Vec<PathBuf>,
    directories: Vec<Directory>,
}

/// A resolved icon set: the requested set, everything it inherits, and the flat directories that
/// answer when no set does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconSet {
    levels: Vec<Level>,
    /// Unthemed directories such as `/usr/share/pixmaps`, holding bare `name.png` files.
    flat: Vec<PathBuf>,
}

impl IconSet {
    /// Resolve `requested` and its inheritance chain against the search path.
    ///
    /// `themed` are the roots holding sets (`…/icons`, `~/.icons`) and `flat` the unthemed
    /// directories (`/usr/share/pixmaps`), both in search-path order
    /// (`contracts/icon-lookup.md`). The chain always ends at [`DEFAULT_SET`], appended whether or
    /// not anything inherits it, so a set with no `Inherits` still falls back where the
    /// specification says it should.
    ///
    /// Nothing is reported here: a set that is not installed is a question for the caller, which
    /// is where FR-057's fallback and its diagnostic live.
    #[must_use]
    pub fn load(requested: &str, themed: &[PathBuf], flat: &[PathBuf]) -> Self {
        let mut levels: Vec<Level> = Vec::new();
        let mut queue: Vec<String> = vec![requested.to_owned()];
        let mut seen: Vec<String> = Vec::new();

        while let Some(name) = queue.first().cloned() {
            queue.remove(0);
            if name.is_empty() || seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());

            let roots: Vec<PathBuf> = themed
                .iter()
                .map(|root| root.join(&name))
                .filter(|path| path.is_dir())
                .collect();
            if roots.is_empty() {
                continue;
            }

            // One `index.theme` per set, taken from the first root that has one: an extending
            // copy in `~/.icons` adds files, not a second directory list.
            let index = roots
                .iter()
                .find_map(|root| std::fs::read_to_string(root.join("index.theme")).ok())
                .map(|text| SetIndex::parse(&text))
                .unwrap_or_default();

            // Inheritance is followed in the order listed, and depth-first: everything the parent
            // itself inherits is tried before the grandparent's siblings, which is what makes a
            // chain of themes behave like the single ordered fallback list users expect.
            let mut rest = index.inherits.clone();
            rest.append(&mut queue);
            queue = rest;

            levels.push(Level {
                name,
                roots,
                directories: index.directories,
            });
        }

        // The terminator, appended rather than assumed: a chain that already passed through it
        // does not visit it twice.
        if !seen.iter().any(|name| name == DEFAULT_SET) {
            let roots: Vec<PathBuf> = themed
                .iter()
                .map(|root| root.join(DEFAULT_SET))
                .filter(|path| path.is_dir())
                .collect();
            if !roots.is_empty() {
                let index = roots
                    .iter()
                    .find_map(|root| std::fs::read_to_string(root.join("index.theme")).ok())
                    .map(|text| SetIndex::parse(&text))
                    .unwrap_or_default();
                levels.push(Level {
                    name: DEFAULT_SET.to_owned(),
                    roots,
                    directories: index.directories,
                });
            }
        }

        Self {
            levels,
            flat: flat.to_vec(),
        }
    }

    /// The chain this set resolved to, outermost first — the evidence `Inherits` was followed.
    #[must_use]
    pub fn chain(&self) -> Vec<&str> {
        self.levels
            .iter()
            .map(|level| level.name.as_str())
            .collect()
    }

    /// Whether any set in the chain was actually found on disk. `false` is SC-016's "no icon set
    /// installed at all", which is a normal state and not a failure.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty() && self.flat.is_empty()
    }

    /// The file holding `name` at `size` device pixels, or `None` when nothing in the chain has
    /// it (FR-041).
    ///
    /// An absolute path is honoured as written — a desktop entry is allowed to give one instead
    /// of a name, and looking it up in a set would only fail.
    ///
    /// Otherwise the specification's two passes are run per set, in chain order: first any
    /// directory that holds the requested size outright, then the directory closest to it. Both
    /// passes finish within one set before its parent is tried, so a parent's exactly-sized icon
    /// never beats the child set's own artwork — which is the whole point of inheriting.
    ///
    /// The requested size is already in device pixels, so the scale asked for is always 1: a
    /// scaled monitor asks for a *larger* icon rather than for the same icon at a higher scale,
    /// which is what FR-039's "drawn at the monitor's device resolution" means here.
    #[must_use]
    pub fn lookup(&self, name: &str, size: u32) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        let direct = Path::new(name);
        if direct.is_absolute() {
            return direct.is_file().then(|| direct.to_path_buf());
        }

        for level in &self.levels {
            if let Some(found) = level_lookup(level, name, size) {
                return Some(found);
            }
        }
        // The unthemed fallback: a bare `name.png` in `/usr/share/pixmaps`, which belongs to no
        // set and answers only when every set has failed.
        self.flat.iter().find_map(|root| existing_file(root, name))
    }
}

/// The specification's `LookupIcon` for one set: an exactly-matching directory first, then the
/// closest one.
fn level_lookup(level: &Level, name: &str, size: u32) -> Option<PathBuf> {
    for directory in &level.directories {
        if !directory.matches(size, 1) {
            continue;
        }
        for root in &level.roots {
            if let Some(found) = existing_file(&root.join(&directory.path), name) {
                return Some(found);
            }
        }
    }

    let mut closest: Option<(u32, PathBuf)> = None;
    for directory in &level.directories {
        let distance = directory.distance(size, 1);
        if closest.as_ref().is_some_and(|(best, _)| distance >= *best) {
            continue;
        }
        for root in &level.roots {
            if let Some(found) = existing_file(&root.join(&directory.path), name) {
                closest = Some((distance, found));
                break;
            }
        }
    }
    closest.map(|(_, path)| path)
}

/// `directory/name.ext` for the first extension this application can decode.
fn existing_file(directory: &Path, name: &str) -> Option<PathBuf> {
    EXTENSIONS.iter().find_map(|extension| {
        let candidate = directory.join(format!("{name}.{extension}"));
        candidate.is_file().then_some(candidate)
    })
}

/// The whole INI file as `(group, [(key, value)])`, in file order.
///
/// The same minimal reader shape as `entries.rs`, kept separate because what the two formats do
/// with a group differs: a desktop entry reads one known group, and an `index.theme` reads a group
/// per directory whose names it only learns from the header.
fn parse_ini(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            groups.push((name.trim().to_owned(), Vec::new()));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Some((_, keys)) = groups.last_mut() {
            keys.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    groups
}

/// The first value for `key`, matching the specification's "first occurrence wins".
fn lookup_key<'a>(keys: &'a [(String, String)], key: &str) -> Option<&'a str> {
    keys.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SET, Directory, IconSet, Kind, SetIndex};
    use std::path::PathBuf;

    // --- T031: directory scoring, filesystem-free (research.md R20) ----------

    #[test]
    fn a_fixed_directory_matches_only_its_own_size() {
        let fixed = Directory {
            kind: Kind::Fixed,
            ..Directory::of_size("48x48/apps", 48)
        };
        assert!(fixed.matches(48, 1));
        assert!(!fixed.matches(47, 1), "fixed means fixed");
        assert!(!fixed.matches(49, 1));
        assert_eq!(fixed.distance(48, 1), 0);
        assert_eq!(fixed.distance(20, 1), 28, "distance is the plain gap");
        assert_eq!(fixed.distance(64, 1), 16);
    }

    #[test]
    fn a_scalable_directory_matches_its_whole_range_and_is_the_ideal_answer_inside_it() {
        let scalable = Directory {
            kind: Kind::Scalable,
            min_size: 8,
            max_size: 512,
            ..Directory::of_size("scalable/apps", 48)
        };
        assert!(scalable.matches(8, 1));
        assert!(scalable.matches(20, 1));
        assert!(scalable.matches(512, 1));
        assert!(!scalable.matches(7, 1));
        assert!(!scalable.matches(513, 1));

        assert_eq!(
            scalable.distance(20, 1),
            0,
            "anywhere in range is a zero cost"
        );
        assert_eq!(scalable.distance(4, 1), 4, "below the minimum, by how much");
        assert_eq!(
            scalable.distance(600, 1),
            88,
            "above the maximum, by how much"
        );
    }

    #[test]
    fn a_threshold_directory_matches_its_size_give_or_take() {
        // The specification's default, and what a directory declaring only `Size` gets.
        let threshold = Directory::of_size("32x32/apps", 32);
        assert_eq!(threshold.kind, Kind::Threshold);
        assert_eq!(threshold.threshold, 2);
        assert!(threshold.matches(30, 1));
        assert!(threshold.matches(34, 1));
        assert!(!threshold.matches(29, 1));
        assert!(!threshold.matches(35, 1));
        assert_eq!(threshold.distance(32, 1), 0);
        assert_eq!(
            threshold.distance(20, 1),
            10,
            "measured from the near bound"
        );
        assert_eq!(threshold.distance(40, 1), 6);
    }

    #[test]
    fn a_threshold_smaller_than_its_size_cannot_underflow() {
        // `Size=1, Threshold=4` is nonsense but nothing stops a set shipping it, and the
        // subtraction underflowing would be a panic in a debug build.
        let odd = Directory {
            threshold: 4,
            ..Directory::of_size("tiny", 1)
        };
        assert!(odd.matches(0, 1));
        assert_eq!(odd.distance(0, 1), 0);
    }

    #[test]
    fn a_directory_for_a_different_scale_never_matches() {
        let doubled = Directory {
            scale: 2,
            ..Directory::of_size("24x24@2x/apps", 24)
        };
        assert!(!doubled.matches(24, 1));
        assert!(doubled.matches(24, 2));
        // Distance still compares device pixels, so 24 at scale 2 is seen as 48-pixel artwork.
        assert_eq!(doubled.distance(48, 1), 0);
    }

    // --- T029: index.theme parsing ------------------------------------------

    #[test]
    fn parsing_reads_the_directory_list_and_each_directorys_metadata() {
        let index = SetIndex::parse(
            "[Icon Theme]\n\
             Name=Example\n\
             Inherits=Adwaita,hicolor\n\
             Directories=48x48/apps,scalable/apps\n\
             \n\
             [48x48/apps]\n\
             Size=48\n\
             Type=Fixed\n\
             \n\
             [scalable/apps]\n\
             Size=48\n\
             MinSize=8\n\
             MaxSize=512\n\
             Type=Scalable\n",
        );

        assert_eq!(index.inherits, vec!["Adwaita", "hicolor"]);
        assert_eq!(index.directories.len(), 2);
        assert_eq!(index.directories[0].path, "48x48/apps");
        assert_eq!(index.directories[0].kind, Kind::Fixed);
        assert_eq!(index.directories[1].kind, Kind::Scalable);
        assert_eq!(index.directories[1].min_size, 8);
        assert_eq!(index.directories[1].max_size, 512);
    }

    #[test]
    fn an_absent_type_min_max_and_scale_take_the_specified_defaults() {
        let index =
            SetIndex::parse("[Icon Theme]\nDirectories=32x32/apps\n\n[32x32/apps]\nSize=32\n");
        let directory = &index.directories[0];
        assert_eq!(directory.kind, Kind::Threshold);
        assert_eq!(directory.scale, 1);
        assert_eq!(directory.min_size, 32);
        assert_eq!(directory.max_size, 32);
        assert_eq!(directory.threshold, 2);
    }

    #[test]
    fn scaled_directories_are_read_alongside_the_plain_ones() {
        let index = SetIndex::parse(
            "[Icon Theme]\n\
             Directories=32x32/apps\n\
             ScaledDirectories=32x32@2x/apps\n\
             \n\
             [32x32/apps]\nSize=32\n\
             \n\
             [32x32@2x/apps]\nSize=32\nScale=2\n",
        );
        assert_eq!(index.directories.len(), 2);
        assert_eq!(index.directories[1].scale, 2);
    }

    #[test]
    fn a_malformed_index_degrades_instead_of_panicking() {
        // Each of these is a real shape of damage: no header, a listed directory with no group,
        // a group with an unreadable size, values that are not numbers, and a truncated file.
        for text in [
            "",
            "not an ini file at all",
            "[Icon Theme]\nDirectories=48x48/apps\n",
            "[Icon Theme]\nDirectories=48x48/apps\n\n[48x48/apps]\nType=Fixed\n",
            "[Icon Theme]\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=huge\n",
            "[Icon Theme]\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nMinSize=\nScale=0\n",
            "[Icon Theme",
        ] {
            let index = SetIndex::parse(text);
            for directory in &index.directories {
                // Whatever survived parsing has to be safe to score, which is the only thing the
                // rest of the module asks of it.
                assert!(directory.scale > 0, "a zero scale would poison the scoring");
                let _ = directory.matches(20, 1);
                let _ = directory.distance(20, 1);
            }
        }
    }

    #[test]
    fn a_group_that_is_not_listed_in_directories_is_not_a_directory() {
        let index = SetIndex::parse(
            "[Icon Theme]\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n\n[16x16/apps]\nSize=16\n",
        );
        assert_eq!(index.directories.len(), 1);
        assert_eq!(index.directories[0].path, "48x48/apps");
    }

    // --- T029: the chain, against a staged root ------------------------------

    /// A temporary set root, so the inheritance tests exercise the real `is_dir` checks that
    /// decide whether a set is installed.
    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "hypr-swap-iconset-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("a staged icon root");
            Self(path)
        }

        fn set(&self, name: &str, index: &str) {
            let directory = self.0.join(name);
            std::fs::create_dir_all(&directory).expect("a staged set");
            std::fs::write(directory.join("index.theme"), index).expect("a staged index.theme");
        }

        fn icon(&self, set: &str, directory: &str, file: &str) -> PathBuf {
            let path = self.0.join(set).join(directory);
            std::fs::create_dir_all(&path).expect("a staged icon directory");
            let path = path.join(file);
            // One byte is enough: nothing here decodes, it only looks the file up.
            std::fs::write(&path, b"x").expect("a staged icon");
            path
        }

        fn roots(&self) -> Vec<PathBuf> {
            vec![self.0.clone()]
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const INDEX_48: &str =
        "[Icon Theme]\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n";

    #[test]
    fn inheritance_is_followed_in_order_and_terminates_at_the_default_set() {
        let root = Root::new();
        root.set(
            "Child",
            "[Icon Theme]\nInherits=Parent\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
        );
        root.set("Parent", INDEX_48);
        root.set(DEFAULT_SET, INDEX_48);

        let set = IconSet::load("Child", &root.roots(), &[]);
        assert_eq!(set.chain(), vec!["Child", "Parent", DEFAULT_SET]);
    }

    #[test]
    fn the_default_set_is_appended_even_when_nothing_inherits_it() {
        let root = Root::new();
        root.set("Lonely", INDEX_48);
        root.set(DEFAULT_SET, INDEX_48);

        let set = IconSet::load("Lonely", &root.roots(), &[]);
        assert_eq!(set.chain(), vec!["Lonely", DEFAULT_SET]);
    }

    #[test]
    fn an_inheritance_cycle_terminates() {
        let root = Root::new();
        root.set("A", "[Icon Theme]\nInherits=B\nDirectories=\n");
        root.set("B", "[Icon Theme]\nInherits=A\nDirectories=\n");

        let set = IconSet::load("A", &root.roots(), &[]);
        assert_eq!(set.chain(), vec!["A", "B"], "each set is visited once");
    }

    #[test]
    fn a_set_that_is_not_installed_contributes_nothing() {
        let root = Root::new();
        root.set(DEFAULT_SET, INDEX_48);

        let set = IconSet::load("NoSuchSet", &root.roots(), &[]);
        assert_eq!(
            set.chain(),
            vec![DEFAULT_SET],
            "the chain still terminates where it should; reporting is the caller's job"
        );
    }

    #[test]
    fn nothing_installed_at_all_is_an_empty_set_rather_than_a_failure() {
        // SC-016: the overlay still opens, every name readable, no error raised.
        let root = Root::new();
        let set = IconSet::load("Anything", &root.roots(), &[]);
        assert!(set.is_empty());
        assert_eq!(set.lookup("firefox", 20), None);
    }

    // --- T029: lookup --------------------------------------------------------

    #[test]
    fn an_exactly_sized_directory_answers_before_a_closer_ranked_one() {
        let root = Root::new();
        root.set(
            "Set",
            "[Icon Theme]\n\
             Directories=16x16/apps,scalable/apps\n\
             \n\
             [16x16/apps]\nSize=16\nType=Fixed\n\
             \n\
             [scalable/apps]\nSize=48\nMinSize=8\nMaxSize=512\nType=Scalable\n",
        );
        root.icon("Set", "16x16/apps", "app.png");
        let scalable = root.icon("Set", "scalable/apps", "app.svg");

        assert_eq!(
            IconSet::load("Set", &root.roots(), &[]).lookup("app", 20),
            Some(scalable),
            "20 is inside the scalable range and outside the fixed directory's"
        );
    }

    #[test]
    fn the_closest_directory_answers_when_none_matches_outright() {
        let root = Root::new();
        root.set(
            "Set",
            "[Icon Theme]\n\
             Directories=16x16/apps,48x48/apps\n\
             \n\
             [16x16/apps]\nSize=16\nType=Fixed\n\
             \n\
             [48x48/apps]\nSize=48\nType=Fixed\n",
        );
        root.icon("Set", "16x16/apps", "app.png");
        let bigger = root.icon("Set", "48x48/apps", "app.png");

        // 40 is 24 from 16 and 8 from 48, so the larger directory wins — and a downscaled icon
        // is what FR-039 wants over an upscaled one.
        assert_eq!(
            IconSet::load("Set", &root.roots(), &[]).lookup("app", 40),
            Some(bigger)
        );
    }

    #[test]
    fn a_child_sets_own_artwork_beats_an_exactly_sized_parent_icon() {
        let root = Root::new();
        root.set(
            "Child",
            "[Icon Theme]\nInherits=Parent\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n",
        );
        root.set(
            "Parent",
            "[Icon Theme]\nDirectories=20x20/apps\n\n[20x20/apps]\nSize=20\nType=Fixed\n",
        );
        let childs = root.icon("Child", "48x48/apps", "app.png");
        root.icon("Parent", "20x20/apps", "app.png");

        assert_eq!(
            IconSet::load("Child", &root.roots(), &[]).lookup("app", 20),
            Some(childs),
            "inheriting is a fallback, not a size competition"
        );
    }

    #[test]
    fn a_parent_answers_what_the_child_does_not_have() {
        let root = Root::new();
        root.set(
            "Child",
            "[Icon Theme]\nInherits=Parent\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
        );
        root.set("Parent", INDEX_48);
        let parents = root.icon("Parent", "48x48/apps", "only-here.png");

        assert_eq!(
            IconSet::load("Child", &root.roots(), &[]).lookup("only-here", 48),
            Some(parents)
        );
    }

    #[test]
    fn png_is_preferred_to_svg_within_one_directory() {
        let root = Root::new();
        root.set("Set", INDEX_48);
        let raster = root.icon("Set", "48x48/apps", "app.png");
        root.icon("Set", "48x48/apps", "app.svg");

        assert_eq!(
            IconSet::load("Set", &root.roots(), &[]).lookup("app", 48),
            Some(raster),
            "the specification's extension order, and the cheaper decode"
        );
    }

    #[test]
    fn a_format_this_application_cannot_decode_is_invisible_to_the_lookup() {
        // FR-040a classes it as unresolvable, which means the placeholder rather than an error.
        let root = Root::new();
        root.set("Set", INDEX_48);
        root.icon("Set", "48x48/apps", "app.xpm");

        assert_eq!(
            IconSet::load("Set", &root.roots(), &[]).lookup("app", 48),
            None
        );
    }

    #[test]
    fn an_unthemed_directory_answers_when_no_set_does() {
        let root = Root::new();
        root.set("Set", INDEX_48);
        let flat = Root::new();
        let loose = flat.0.join("loose.png");
        std::fs::write(&loose, b"x").expect("a staged pixmap");

        let set = IconSet::load("Set", &root.roots(), std::slice::from_ref(&flat.0));
        assert_eq!(set.lookup("loose", 48), Some(loose));
    }

    #[test]
    fn an_absolute_icon_path_is_used_as_written() {
        // A desktop entry may give a path instead of a name; looking one up in a set would only
        // ever fail.
        let flat = Root::new();
        let file = flat.0.join("absolute.png");
        std::fs::write(&file, b"x").expect("a staged icon");

        let set = IconSet::load("Set", &[], &[]);
        assert_eq!(set.lookup(&file.display().to_string(), 20), Some(file));
        assert_eq!(set.lookup("/nowhere/at/all.png", 20), None);
        assert_eq!(set.lookup("", 20), None);
    }
}
