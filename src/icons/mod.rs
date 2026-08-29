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

/// The generic icon drawn whenever a program's own icon cannot be resolved (FR-041).
///
/// It is embedded in the binary rather than looked up, so it is available on a system with no icon
/// set installed at all — which is what makes SC-016's "every name readable, no error raised"
/// hold. Unlike program artwork, which is drawn as supplied (FR-051), the placeholder follows the
/// theme's primary text colour.
pub const PLACEHOLDER_SVG: &[u8] = include_bytes!("../../assets/placeholder.svg");

#[cfg(test)]
mod tests {
    use super::PLACEHOLDER_SVG;

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
}
