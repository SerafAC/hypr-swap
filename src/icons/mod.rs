//! Program icons: turning a window's class into a drawable surface, once per program per run.
//!
//! The chain is the freedesktop one, and each link is a submodule because each is a separate rule
//! (`specs/002-overlay-visuals/contracts/icon-lookup.md`):
//!
//! ```text
//! window.class → desktop entry → icon name → icon file → cairo surface
//!                 entries.rs                  iconset.rs   decode.rs
//! ```
//!
//! Any link failing yields the placeholder below. That is a normal outcome, not a reported failure
//! (FR-041).
//!
//! Resolution happens ahead of time — at start-up and whenever the world is rebuilt — so opening
//! the overlay only blits surfaces that are already decoded and never touches the filesystem
//! (FR-042, FR-043, research.md R27). The cache is memory only and dies with the process; there is
//! no on-disk cache, by requirement (FR-043b).

pub mod decode;
pub mod entries;
pub mod iconset;

use std::collections::HashMap;
use std::path::PathBuf;

use cairo::ImageSurface;

use crate::diag::{self, Condition};

/// The generic icon drawn whenever a program's own icon cannot be resolved (FR-041).
///
/// It is embedded in the binary rather than looked up, so it is available on a system with no icon
/// set installed at all — which is what makes SC-016's "every name readable, no error raised"
/// hold. Unlike program artwork, which is drawn as supplied (FR-051), the placeholder follows the
/// theme's primary text colour.
pub const PLACEHOLDER_SVG: &[u8] = include_bytes!("../../assets/placeholder.svg");

/// What a paint record names when the placeholder was drawn (research.md R22).
pub const PLACEHOLDER_SOURCE: &str = "placeholder";

/// The outcome of resolving one program, cached against its class.
///
/// A resolution *result*, not merely an image: "we tried and failed" is a first-class value, which
/// is what makes FR-042's once-per-run guarantee hold for failures too and what stops FR-044's
/// diagnostic repeating on every overlay opening.
#[derive(Debug)]
enum Icon {
    /// The program's own artwork, decoded, with the file it came from for the paint record.
    Resolved {
        surface: ImageSurface,
        source: String,
    },
    /// No entry matched, no file was found, or decoding failed (FR-041, FR-044).
    Placeholder,
}

/// What the paint path is handed for one window.
///
/// The distinction the renderer needs is not "did it work" but "may this be recoloured": a
/// program's artwork is drawn exactly as supplied, and only the placeholder follows the theme
/// (FR-051).
#[derive(Debug, Clone, Copy)]
pub struct Drawn<'a> {
    /// The surface to blit, or `None` when there is nothing at all to draw — which happens only
    /// if even the embedded placeholder failed to decode.
    pub surface: Option<&'a ImageSurface>,
    /// Whether that surface is the placeholder, and so may be tinted (FR-051).
    pub placeholder: bool,
    /// What a paint record calls this: the icon file chosen, or [`PLACEHOLDER_SOURCE`].
    pub source: &'a str,
}

/// The per-program icon cache and its two operations.
///
/// Deliberately not `Clone` and deliberately holding no path to disk: FR-043b makes "memory only"
/// a requirement, so the type has no way to be written out or read back. It is owned by the
/// Wayland client and therefore dropped with it — on exit and on connection loss — which is the
/// same teardown every other piece of derived state gets (research.md R28, FR-026c).
pub struct IconStore {
    /// The icon slot in device pixels: the size vector icons are rasterised at, and the size a
    /// lookup asks the icon set for (FR-052).
    slot: u32,
    /// The desktop-entry index, built once.
    index: entries::Index,
    /// The icon set and its inheritance chain, resolved once.
    set: iconset::IconSet,
    /// One entry per class ever asked for, successes and failures alike (FR-042).
    cache: HashMap<String, Icon>,
    /// The embedded placeholder, rasterised once at the slot size. `None` only if the embedded
    /// asset itself failed, which the unit tests in this module make impossible to ship.
    placeholder: Option<ImageSurface>,
    /// `false` for the store `icons = false` gets, which resolves nothing and draws nothing
    /// (FR-056). A flag rather than an absent store, so every caller has one shape to talk to.
    enabled: bool,
}

impl IconStore {
    /// Build a store that resolves into a `slot`-device-pixel square.
    ///
    /// `configured` is the `icon_set` setting: a name the user gave, or `None` to follow the
    /// desktop's own set (FR-057). Choosing between those is
    /// [`iconset::select`]'s rule, and this is the one place it runs — the diagnostic it may
    /// produce is reported here because this is where the filesystem the rule asks about is
    /// finally in view.
    ///
    /// The desktop-entry index and the icon set are both read here, once: this is the expensive
    /// part, and it happens at start-up rather than in the paint path (research.md R27).
    #[must_use]
    pub fn new(slot: u32, configured: Option<&str>) -> Self {
        let roots = data_roots();
        let (set, diagnostic) = iconset::select(configured, &themed_roots(&roots), &config_roots());
        if let Some(diagnostic) = diagnostic {
            diagnostic.report();
        }
        Self::with_roots(slot, &set, &roots)
    }

    /// The same, against explicit data roots and an already-chosen set — the seam the unit tests
    /// below use so no assertion depends on what the developer has installed (research.md R22).
    #[must_use]
    pub fn with_roots(slot: u32, set: &str, roots: &[PathBuf]) -> Self {
        let themed = themed_roots(roots);
        // Unthemed directories: bare `name.png` files belonging to no set. `/usr/share/pixmaps`
        // is the one everybody has, and it arrives here as `$XDG_DATA_DIRS`'s `/usr/share`
        // rather than as a hard-coded path, so a test that empties the environment really is
        // isolated from the machine.
        let flat: Vec<PathBuf> = roots.iter().map(|root| root.join("pixmaps")).collect();

        Self {
            slot,
            index: entries::Index::scan(roots),
            set: iconset::IconSet::load(set, &themed, &flat),
            cache: HashMap::new(),
            placeholder: decode::svg(PLACEHOLDER_SVG, slot).ok(),
            enabled: true,
        }
    }

    /// An empty store that resolves nothing — what `icons = false` gets (FR-056).
    ///
    /// Nothing is scanned and nothing is decoded, so turning icons off really does mean no
    /// filesystem work rather than work whose result is discarded.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            slot: 0,
            index: entries::Index::default(),
            set: iconset::IconSet::default(),
            cache: HashMap::new(),
            placeholder: None,
            enabled: false,
        }
    }

    /// Resolve every class not already cached (FR-042, FR-043).
    ///
    /// Called at start-up and on the world-rebuild path, never from the paint path: by the time
    /// an overlay can open, everything it will draw is already decoded (research.md R27). A class
    /// that is already known costs one hash lookup, which is what makes calling this on every
    /// window-open event free.
    pub fn ensure<'a>(&mut self, classes: impl IntoIterator<Item = &'a str>) {
        if !self.enabled {
            return;
        }
        for class in classes {
            if class.is_empty() || self.cache.contains_key(class) {
                continue;
            }
            let icon = self.resolve(class);
            self.cache.insert(class.to_owned(), icon);
        }
    }

    /// What to draw for `class` — a pure lookup that never resolves and never touches the
    /// filesystem (FR-043).
    ///
    /// A class that has not been resolved yet is the placeholder for this opening, and the
    /// overlay is neither held back nor repainted when the real icon arrives; it simply appears
    /// the next time the overlay opens (FR-043a).
    #[must_use]
    pub fn get(&self, class: &str) -> Drawn<'_> {
        match self.cache.get(class) {
            Some(Icon::Resolved { surface, source }) => Drawn {
                surface: Some(surface),
                placeholder: false,
                source,
            },
            // Cached as a failure, or not resolved yet: the same picture either way (FR-043a).
            Some(Icon::Placeholder) | None => Drawn {
                surface: self.placeholder.as_ref(),
                placeholder: true,
                source: PLACEHOLDER_SOURCE,
            },
        }
    }

    /// How many programs have been resolved. The evidence FR-042's once-per-program rule holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// The icon slot this store decodes for, in device pixels.
    #[must_use]
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// Whether icons are drawn at all (FR-056). The renderer asks before reserving any space, so
    /// `icons = false` costs no layout as well as no lookup.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The whole chain for one class: entry → icon name → file → surface.
    ///
    /// Each step failing is a different kind of nothing, and only one of them is worth telling
    /// the user about. A class no entry claims, and an entry whose icon no set holds, are the
    /// ordinary state of a desktop and yield the placeholder in silence (FR-041). A file that
    /// exists and cannot be decoded is a fault in what is installed, and is reported — once,
    /// because the failure is cached alongside the successes (FR-044).
    fn resolve(&self, class: &str) -> Icon {
        let Some(name) = self.index.icon_for(class) else {
            return Icon::Placeholder;
        };
        let Some(path) = self.set.lookup(name, self.slot) else {
            return Icon::Placeholder;
        };
        match decode::decode(&path, self.slot) {
            Ok(surface) => Icon::Resolved {
                surface,
                source: path.display().to_string(),
            },
            Err(decode::DecodeError::Unsupported) => Icon::Placeholder,
            Err(e) => {
                diag::report(
                    Condition::IconUnreadable,
                    &format!("icon.{class}"),
                    &format!("{} {e}", path.display()),
                );
                Icon::Placeholder
            }
        }
    }
}

/// The data roots the desktop entries and the icon sets are searched under, in order
/// (`contracts/icon-lookup.md`).
///
/// `XDG_DATA_DIRS` set to the empty string means *no* system directories, not the default ones.
/// The specification's "not set or empty" wording reads the other way, but taking it literally
/// would leave a caller no way to say "search nothing but what I gave you" — which is exactly what
/// the E2E fixtures need in order to force a lookup miss without depending on what the developer
/// has installed (research.md R22).
/// The icon set that would actually be drawn from, given what the user configured (FR-116).
///
/// The same rule [`IconStore::new`] runs, against the same roots, but without building a store or
/// reporting anything: `--environment` answers a question rather than starting a daemon, and the
/// set that was *resolved* — which differs from the one configured exactly when something is
/// wrong — is the fact a bug report needs.
#[must_use]
pub fn resolved_set(configured: Option<&str>) -> String {
    let roots = data_roots();
    let (set, _diagnostic) = iconset::select(configured, &themed_roots(&roots), &config_roots());
    set
}

fn data_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".local").join("share")));

    let dirs = match std::env::var_os("XDG_DATA_DIRS") {
        Some(value) => std::env::split_paths(&value).collect(),
        None => vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ],
    };

    home.into_iter().chain(dirs).collect()
}

/// The roots holding icon *sets*, in search order (`contracts/icon-lookup.md`).
///
/// Shared by the store and by the set-selection rule, so "where a set could be installed" has one
/// definition rather than one per caller.
fn themed_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .map(|root| root.join("icons"))
        .chain(home().map(|home| home.join(".icons")))
        .collect()
}

/// The configuration directories, in search order — where the desktop records its icon set
/// (FR-057). The `$XDG_CONFIG_*` counterpart of [`data_roots`], and empty-means-empty for the
/// same reason.
fn config_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".config")));

    let dirs = match std::env::var_os("XDG_CONFIG_DIRS") {
        Some(value) => std::env::split_paths(&value).collect(),
        None => vec![PathBuf::from("/etc/xdg")],
    };

    home.into_iter().chain(dirs).collect()
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{IconStore, PLACEHOLDER_SOURCE, PLACEHOLDER_SVG};
    use std::path::PathBuf;

    /// The placeholder has to be usable on a system with no icon set at all (SC-016), which means
    /// it has to survive the `resvg` build this project actually ships: no `text`, no
    /// `system-fonts`, no `svgz` (research.md R18). Parsing it here catches an asset that only
    /// renders under a fuller feature set, at the moment the asset changes rather than at runtime.
    #[test]
    fn the_embedded_placeholder_parses_and_has_size() {
        let tree = resvg::usvg::Tree::from_data(PLACEHOLDER_SVG, &resvg::usvg::Options::default())
            .expect("the embedded placeholder is valid SVG");
        let size = tree.size();
        assert!(
            size.width() > 0.0 && size.height() > 0.0,
            "the placeholder has a drawable size, got {size:?}"
        );
        assert!(
            (size.width() - size.height()).abs() < f32::EPSILON,
            "the placeholder is square, so it fits a square icon slot without letterboxing"
        );

        // Parsing is not enough: an asset made of elements this build cannot draw would parse and
        // then rasterise to nothing. Render it at the small size the icon slot actually asks for
        // and require real coverage.
        let mut pixmap = resvg::tiny_skia::Pixmap::new(20, 20).expect("a 20x20 pixmap");
        let scale = 20.0 / size.width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let covered = pixmap.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(
            covered > 20,
            "the placeholder draws something at icon size, got {covered} covered pixels of 400"
        );
    }

    // --- T037: the store, against a staged root (FR-042, FR-043, SC-017) -----

    /// The set the fixture root installs, mirroring `tests/e2e/fixtures.rs` so the unit and E2E
    /// halves of this story exercise the same shapes.
    const SET: &str = "FixtureSet";

    /// A staged data root: two desktop entries, one resolvable icon and one broken file.
    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "hypr-swap-store-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("applications")).expect("a staged root");
            let staged = Self(root);

            for (id, class, icon) in [
                ("fixture-alpha", "fixturealpha", "fixture-alpha"),
                ("fixture-broken", "fixturebroken", "fixture-broken"),
            ] {
                staged.write(
                    &PathBuf::from("applications").join(format!("{id}.desktop")),
                    format!(
                        "[Desktop Entry]\nType=Application\nName={id}\n\
                         StartupWMClass={class}\nIcon={icon}\n"
                    )
                    .as_bytes(),
                );
            }

            let set = PathBuf::from("icons").join(SET);
            staged.write(
                &set.join("index.theme"),
                b"[Icon Theme]\nDirectories=scalable/apps\n\n\
                  [scalable/apps]\nSize=48\nMinSize=8\nMaxSize=512\nType=Scalable\n",
            );
            staged.write(
                &set.join("scalable").join("apps").join("fixture-alpha.svg"),
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect width="48" height="48"/></svg>"#,
            );
            // Enough of an SVG to be found and not enough to parse: FR-044's reported failure.
            staged.write(
                &set.join("scalable").join("apps").join("fixture-broken.svg"),
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"",
            );
            staged
        }

        fn write(&self, relative: &PathBuf, bytes: &[u8]) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a staged directory");
            std::fs::write(&path, bytes).expect("a staged file");
        }

        fn store(&self) -> IconStore {
            IconStore::with_roots(20, SET, std::slice::from_ref(&self.0))
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_resolvable_class_yields_its_own_artwork_named_by_the_file_it_came_from() {
        let root = Root::new();
        let mut store = root.store();
        store.ensure(["fixturealpha"]);

        let drawn = store.get("fixturealpha");
        assert!(drawn.surface.is_some());
        assert!(
            !drawn.placeholder,
            "FR-051: program artwork is never tinted"
        );
        assert!(
            drawn.source.ends_with("fixture-alpha.svg"),
            "the paint record names the file chosen, got {}",
            drawn.source
        );
    }

    #[test]
    fn a_class_no_entry_claims_is_the_placeholder_in_silence() {
        // FR-041: a normal outcome. Nothing is reported and nothing is notified.
        let root = Root::new();
        let mut store = root.store();
        store.ensure(["fixturenobody"]);

        let drawn = store.get("fixturenobody");
        assert!(drawn.placeholder);
        assert!(
            drawn.surface.is_some(),
            "the placeholder is always available"
        );
        assert_eq!(drawn.source, PLACEHOLDER_SOURCE);
    }

    #[test]
    fn a_class_that_was_never_resolved_is_also_the_placeholder() {
        // FR-043a: the overlay is not held back for a window that appeared moments ago.
        let root = Root::new();
        let store = root.store();
        assert!(store.get("fixturealpha").placeholder);
        assert!(store.is_empty(), "and asking did not resolve anything");
    }

    #[test]
    fn a_malformed_file_is_cached_as_a_failure_rather_than_retried() {
        // FR-044: the report happens inside `ensure`; what this proves is that the *second*
        // `ensure` cannot produce a second one, because the class is already cached.
        let root = Root::new();
        let mut store = root.store();
        store.ensure(["fixturebroken"]);
        assert_eq!(store.len(), 1);
        assert!(store.get("fixturebroken").placeholder);

        store.ensure(["fixturebroken"]);
        store.ensure(["fixturebroken"]);
        assert_eq!(
            store.len(),
            1,
            "the failure is a cached result, not a retry"
        );
    }

    #[test]
    fn each_class_is_resolved_once_however_often_it_is_asked_for() {
        // FR-042 and SC-017: three windows of one program across two openings resolve once.
        let root = Root::new();
        let mut store = root.store();
        store.ensure(["fixturealpha", "fixturealpha", "fixturealpha"]);
        assert_eq!(store.len(), 1);

        store.ensure(["fixturealpha", "fixturebroken"]);
        assert_eq!(
            store.len(),
            2,
            "only the class it had not seen was resolved"
        );

        store.ensure(["fixturealpha", "fixturebroken"]);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn get_never_resolves_however_often_it_is_called() {
        // FR-043: opening the overlay reads and does nothing else.
        let root = Root::new();
        let store = root.store();
        for _ in 0..5 {
            let _ = store.get("fixturealpha");
            let _ = store.get("fixturebroken");
        }
        assert!(store.is_empty());
    }

    #[test]
    fn an_empty_class_is_not_a_program_and_is_never_resolved() {
        let root = Root::new();
        let mut store = root.store();
        store.ensure([""]);
        assert!(store.is_empty());
    }

    #[test]
    fn a_disabled_store_resolves_nothing_and_draws_nothing() {
        // FR-056: no desktop-entry scan, no icon-set lookup, no placeholder, no reserved space.
        let mut store = IconStore::disabled();
        store.ensure(["fixturealpha", "fixturebroken"]);

        assert!(store.is_empty());
        assert_eq!(store.slot(), 0);
        assert!(!store.enabled());
        let drawn = store.get("fixturealpha");
        assert!(
            drawn.surface.is_none(),
            "with icons off there is nothing to draw, not even a placeholder"
        );
    }

    #[test]
    fn a_root_with_nothing_installed_still_gives_every_window_a_placeholder() {
        // SC-016: no icon set installed at all, no error raised, every name still readable.
        let empty =
            std::env::temp_dir().join(format!("hypr-swap-store-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("an empty root");
        let mut store = IconStore::with_roots(20, SET, std::slice::from_ref(&empty));
        store.ensure(["anything"]);

        let drawn = store.get("anything");
        assert!(drawn.placeholder);
        assert!(drawn.surface.is_some());
        let _ = std::fs::remove_dir_all(&empty);
    }
}
