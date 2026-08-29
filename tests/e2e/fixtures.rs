//! A synthetic `XDG_DATA_HOME`: desktop entries and icon sets staged into a temporary directory
//! so no icon assertion depends on what the developer happens to have installed (research.md R22).
//!
//! The layout is the one `contracts/icon-lookup.md` specifies, and each file is there to force
//! exactly one outcome:
//!
//! | Fixture | Forces | Covers |
//! |---|---|---|
//! | `fixture-alpha` | a vector icon resolves | FR-040a |
//! | `fixture-beta` | a raster icon resolves | FR-040a |
//! | `fixture-broken` | decoding fails, reported once | FR-044 |
//! | [`UNKNOWN_CLASS`] | no desktop entry matches at all | FR-041 |
//! | [`Fixtures::empty`] | no icon set installed at all | SC-016 |
//! | [`SECOND_SET`] | `icon_set` selects a different set | FR-057 |
//!
//! The window classes below are what a test spawns its clients with, so a class and the
//! `StartupWMClass` that claims it are written down once, here, rather than in each test.

use std::path::{Path, PathBuf};

/// The icon set the fixtures' entries resolve within.
pub const SET: &str = "FixtureSet";

/// A second installed set, so `icon_set` has something to be switched *to* (FR-057).
pub const SECOND_SET: &str = "FixtureSetTwo";

/// The window class whose icon is a vector file.
pub const ALPHA_CLASS: &str = "fixturealpha";
/// The window class whose icon is a raster file.
pub const BETA_CLASS: &str = "fixturebeta";
/// The window class whose icon file is truncated and cannot be decoded (FR-044).
pub const BROKEN_CLASS: &str = "fixturebroken";
/// A class no desktop entry claims, so it can only ever be the placeholder (FR-041).
pub const UNKNOWN_CLASS: &str = "fixturenobody";

/// A 48 x 48 opaque RGBA PNG of a single colour.
///
/// Written out as bytes rather than generated, so the fixture is the same on every machine and
/// the raster path is exercised against a file this repository controls end to end. The colour is
/// `#339966`, distinctive enough that a decoder test can recognise it (research.md R19).
const VALID_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x30, 0x08, 0x06, 0x00, 0x00, 0x00, 0x57, 0x02, 0xf9,
    0x87, 0x00, 0x00, 0x00, 0x44, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0xed, 0xcf, 0x41, 0x11, 0x00,
    0x00, 0x04, 0x00, 0x30, 0x9d, 0x74, 0xd2, 0x49, 0x5a, 0x2a, 0xf8, 0xba, 0xdb, 0x63, 0x01, 0x16,
    0xd9, 0x35, 0x9f, 0x85, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xc0, 0xd5, 0x02, 0x89, 0x2a, 0xba, 0x1e, 0x0c, 0x43, 0x0d,
    0x6c, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The same picture as a vector, for the `resvg` path (research.md R18).
///
/// Deliberately free of `<text>`: this project's `resvg` build has no font stack, so an SVG that
/// needed one would rasterise to nothing (research.md R18, `Cargo.toml`).
const VALID_SVG: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48">"##,
    r##"<rect width="48" height="48" fill="#339966"/></svg>"##,
    "\n",
);

/// A staged `XDG_DATA_HOME`, removed when it is dropped.
///
/// Nothing in here is written to the developer's own data directories, and nothing survives the
/// test: FR-043b forbids an on-disk icon cache, so a test that left files behind could not tell
/// the difference between the application caching and itself.
pub struct Fixtures {
    root: PathBuf,
}

impl Fixtures {
    /// Stage the full set: three desktop entries, [`SET`] with its three icon files, and an empty
    /// [`SECOND_SET`] for the `icon_set` switching test to point at.
    ///
    /// # Panics
    /// If the temporary directory cannot be written, which no test can recover from.
    #[must_use]
    pub fn stage() -> Self {
        let fixtures = Self::empty();

        for (id, class, icon) in [
            ("fixture-alpha", ALPHA_CLASS, "fixture-alpha"),
            ("fixture-beta", BETA_CLASS, "fixture-beta"),
            ("fixture-broken", BROKEN_CLASS, "fixture-broken"),
        ] {
            fixtures.write(
                &PathBuf::from("applications").join(format!("{id}.desktop")),
                format!(
                    "[Desktop Entry]\n\
                     Type=Application\n\
                     Name={id}\n\
                     Exec=/bin/true\n\
                     StartupWMClass={class}\n\
                     Icon={icon}\n"
                )
                .as_bytes(),
            );
        }

        // Two directories, one of each `Type`, so the size-scoring rule has both a fixed-size and
        // a scalable candidate to choose between (research.md R20).
        fixtures.write_index(SET);
        fixtures.write_index(SECOND_SET);

        fixtures.write(
            &icon_file(SET, "scalable/apps", "fixture-alpha.svg"),
            VALID_SVG.as_bytes(),
        );
        fixtures.write(&icon_file(SET, "48x48/apps", "fixture-beta.png"), VALID_PNG);
        // Truncated on purpose: the header says PNG, the data stops early, so decoding gets far
        // enough to be a real failure rather than an unrecognised extension (FR-044).
        fixtures.write(
            &icon_file(SET, "48x48/apps", "fixture-broken.png"),
            &VALID_PNG[..VALID_PNG.len() / 3],
        );

        // The second set carries the same names with a different picture, so a test can tell which
        // set an icon came from (FR-057).
        fixtures.write(
            &icon_file(SECOND_SET, "48x48/apps", "fixture-beta.png"),
            VALID_PNG,
        );
        fixtures.write(
            &icon_file(SECOND_SET, "scalable/apps", "fixture-alpha.svg"),
            VALID_SVG.as_bytes(),
        );

        fixtures
    }

    /// A staged root with no desktop entries and no icon sets at all — SC-016's "the overlay still
    /// opens with every name readable and no error raised".
    ///
    /// # Panics
    /// If the temporary directory cannot be created.
    #[must_use]
    pub fn empty() -> Self {
        let unique = format!("hypr-swap-icons-{}-{}", std::process::id(), next_serial());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("applications")).expect("create the fixture root");
        std::fs::create_dir_all(root.join("icons")).expect("create the fixture icon root");
        Self { root }
    }

    /// The directory to hand the daemon as `XDG_DATA_HOME`.
    #[must_use]
    pub fn data_home(&self) -> &Path {
        &self.root
    }

    /// The environment a daemon must be started with to see these fixtures and nothing else.
    ///
    /// `XDG_DATA_DIRS` is emptied as well as `XDG_DATA_HOME` being pointed here: leaving the
    /// system directories in place would let `/usr/share/applications` answer a lookup the test
    /// meant to force a miss on.
    #[must_use]
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            ("XDG_DATA_HOME".to_owned(), self.root.display().to_string()),
            ("XDG_DATA_DIRS".to_owned(), String::new()),
        ]
    }

    /// Where a staged icon file ended up, for a test that wants to name it in an assertion.
    #[must_use]
    pub fn icon(&self, set: &str, directory: &str, file: &str) -> PathBuf {
        self.root.join(icon_file(set, directory, file))
    }

    fn write_index(&self, set: &str) {
        self.write(
            &PathBuf::from("icons").join(set).join("index.theme"),
            format!(
                "[Icon Theme]\n\
                 Name={set}\n\
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
                 Type=Scalable\n"
            )
            .as_bytes(),
        );
    }

    fn write(&self, relative: &Path, bytes: &[u8]) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
        }
        std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn icon_file(set: &str, directory: &str, file: &str) -> PathBuf {
    let mut path = PathBuf::from("icons").join(set);
    for component in directory.split('/') {
        path.push(component);
    }
    path.join(file)
}

fn next_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    SERIAL.fetch_add(1, Ordering::Relaxed)
}
