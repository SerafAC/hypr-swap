//! Icon file to cairo surface.
//!
//! Two formats, by requirement (FR-040a): PNG through cairo's own loader, which the `png` feature
//! on `cairo-rs` already provides (research.md R19), and SVG through `resvg` into a pixmap that is
//! converted once into a cairo surface (research.md R18). Anything else is unresolvable and yields
//! the placeholder.
//!
//! The icon is fitted to its slot without aspect distortion and rasterised at the monitor's device
//! resolution rather than upscaled from a smaller size (FR-039). A malformed or unreadable file is
//! reported once and cached as a failure, so the diagnostic cannot repeat on every opening
//! (FR-044).

use std::path::Path;

use cairo::{Format, ImageSurface};

/// The buffer format an icon is decoded into: the same pre-multiplied ARGB the overlay itself is
/// painted in, so blitting one is a plain `set_source_surface` with no conversion.
const FORMAT: Format = Format::ARgb32;

/// Why one icon file could not become a surface.
///
/// Three variants because they are three different things to say to the user, and FR-044 asks for
/// the reason as well as the fact.
#[derive(Debug)]
pub enum DecodeError {
    /// Not a format this application decodes — FR-040a's "unresolvable", not a fault in the file.
    Unsupported,
    /// The file could not be read at all.
    Unreadable(std::io::Error),
    /// The file was read and is not valid content for its extension (FR-044).
    Malformed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("not a raster or vector icon"),
            Self::Unreadable(e) => write!(f, "cannot be read: {e}"),
            Self::Malformed(detail) => write!(f, "cannot be decoded: {detail}"),
        }
    }
}

/// Decode one icon file into a surface sized for a `slot`-device-pixel square.
///
/// `slot` is the icon slot in device pixels — the themed text height times the monitor scale
/// (FR-052, `contracts/icon-lookup.md`). It is the *rasterisation* size for a vector icon, which
/// is what FR-039's "drawn at the monitor's device resolution" asks for; a raster icon is decoded
/// at whatever size the file holds, since inventing detail it does not have would only blur it,
/// and [`place`] fits it into the slot at paint time.
///
/// # Errors
/// [`DecodeError`] for a format this application does not decode, a file it cannot read, and a
/// file whose contents do not match its extension.
pub fn decode(path: &Path, slot: u32) -> Result<ImageSurface, DecodeError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "png" => png(path),
        "svg" => svg(&std::fs::read(path).map_err(DecodeError::Unreadable)?, slot),
        _ => Err(DecodeError::Unsupported),
    }
}

/// A raster icon, through the loader `cairo-rs`'s `png` feature already provides (research.md
/// R19).
fn png(path: &Path) -> Result<ImageSurface, DecodeError> {
    let mut file = std::fs::File::open(path).map_err(DecodeError::Unreadable)?;
    ImageSurface::create_from_png(&mut file).map_err(|e| DecodeError::Malformed(e.to_string()))
}

/// A vector icon, rasterised at the size it will actually be drawn at (FR-039).
///
/// Public because the embedded placeholder takes exactly this path — it is an SVG in the binary
/// rather than a file on disk, and nothing else about it differs.
///
/// # Errors
/// [`DecodeError::Malformed`] for data `resvg` cannot parse, and for a document with no drawable
/// size.
pub fn svg(data: &[u8], slot: u32) -> Result<ImageSurface, DecodeError> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default())
        .map_err(|e| DecodeError::Malformed(e.to_string()))?;

    let source = tree.size();
    let (width, height) = (f64::from(source.width()), f64::from(source.height()));
    if width <= 0.0 || height <= 0.0 {
        return Err(DecodeError::Malformed("no drawable size".to_owned()));
    }

    // Fitted rather than stretched: a non-square icon keeps its proportions and simply does not
    // fill the whole square slot (FR-039).
    let scale = f64::from(slot) / width.max(height);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = |length: f64| ((length * scale).round() as u32).max(1);
    let (target_width, target_height) = (pixels(width), pixels(height));

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or_else(|| DecodeError::Malformed("no rasterisation buffer".to_owned()))?;
    // Each axis takes its own factor so the rounding above cannot leave a sliver unpainted; the
    // two differ only in the last fraction of a pixel, so the aspect ratio is still the file's.
    #[allow(clippy::cast_possible_truncation)]
    let transform = resvg::tiny_skia::Transform::from_scale(
        (f64::from(target_width) / width) as f32,
        (f64::from(target_height) / height) as f32,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    surface_from_pixmap(&pixmap)
}

/// Copy a `tiny_skia` pixmap into a cairo surface, swapping the channel order (research.md R18).
///
/// Both sides are **pre-multiplied**, so no alpha arithmetic is needed — only the byte order
/// differs. `tiny_skia::PremultipliedColorU8` is laid out `R, G, B, A`; cairo's `ARgb32` is a
/// native-endian `u32` of `0xAARRGGBB`, which on the little-endian targets this project builds for
/// is `B, G, R, A` in memory. So red and blue swap and nothing else moves.
///
/// The `unpack` accessors are used rather than the raw bytes precisely so this holds on a
/// big-endian target too: they name channels, and the `u32` below is assembled in native order.
fn surface_from_pixmap(pixmap: &resvg::tiny_skia::Pixmap) -> Result<ImageSurface, DecodeError> {
    let width =
        i32::try_from(pixmap.width()).map_err(|_| DecodeError::Malformed("too wide".into()))?;
    let height =
        i32::try_from(pixmap.height()).map_err(|_| DecodeError::Malformed("too tall".into()))?;
    let mut surface = ImageSurface::create(FORMAT, width, height)
        .map_err(|e| DecodeError::Malformed(e.to_string()))?;
    let stride = usize::try_from(surface.stride()).unwrap_or(0);
    {
        let mut data = surface
            .data()
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;
        for (y, row) in pixmap
            .pixels()
            .chunks_exact(pixmap.width() as usize)
            .enumerate()
        {
            let start = y * stride;
            for (x, pixel) in row.iter().enumerate() {
                let argb = u32::from(pixel.alpha()) << 24
                    | u32::from(pixel.red()) << 16
                    | u32::from(pixel.green()) << 8
                    | u32::from(pixel.blue());
                let at = start + x * 4;
                data[at..at + 4].copy_from_slice(&argb.to_ne_bytes());
            }
        }
    }
    Ok(surface)
}

/// Where a `source`-sized icon goes inside a slot rectangle — `(x, y, width, height)`, fitted
/// without distortion and centred (FR-039).
///
/// Pure arithmetic, and the only place the aspect rule is written: both presentations place their
/// icons through it, so a wide icon is letterboxed identically in a list row and in a miniature.
/// `None` when either the source or the slot has no area, which is a rectangle too small to draw
/// in rather than an error.
#[must_use]
pub fn place(source: (f64, f64), slot: (f64, f64, f64, f64)) -> Option<(f64, f64, f64, f64)> {
    let (source_width, source_height) = source;
    let (x, y, width, height) = slot;
    if source_width <= 0.0 || source_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let scale = (width / source_width).min(height / source_height);
    let (drawn_width, drawn_height) = (source_width * scale, source_height * scale);
    Some((
        x + (width - drawn_width) / 2.0,
        y + (height - drawn_height) / 2.0,
        drawn_width,
        drawn_height,
    ))
}

/// A decoded icon's size, as [`place`] takes it.
#[must_use]
pub fn size_of(surface: &ImageSurface) -> (f64, f64) {
    (f64::from(surface.width()), f64::from(surface.height()))
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, decode, place, size_of, svg};
    use cairo::{Context, Format, ImageSurface};
    use std::path::{Path, PathBuf};

    /// The colour every fixture below is painted in, distinctive enough that finding it in the
    /// decoded surface proves the channels did not swap on the way through (research.md R18).
    const RED: u8 = 0x33;
    const GREEN: u8 = 0x99;
    const BLUE: u8 = 0x66;

    /// A temporary directory of icon files, removed when it drops.
    struct Files(PathBuf);

    impl Files {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "hypr-swap-decode-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("a staged icon directory");
            Self(path)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).expect("a staged icon file");
            path
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A real PNG of the fixture colour, written by cairo itself rather than pasted in as a byte
    /// array — the `png` feature R19 relies on is then proving itself in both directions.
    fn png_bytes(width: i32, height: i32) -> Vec<u8> {
        let surface = ImageSurface::create(Format::ARgb32, width, height).expect("a surface");
        {
            let cairo = Context::new(&surface).expect("a context");
            cairo.set_source_rgb(
                f64::from(RED) / 255.0,
                f64::from(GREEN) / 255.0,
                f64::from(BLUE) / 255.0,
            );
            cairo.paint().expect("a filled surface");
        }
        let mut bytes = Vec::new();
        surface
            .write_to_png(&mut std::io::Cursor::new(&mut bytes))
            .expect("cairo can write a PNG");
        bytes
    }

    fn svg_bytes(width: u32, height: u32) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
             viewBox=\"0 0 {width} {height}\">\
             <rect width=\"{width}\" height=\"{height}\" fill=\"#{RED:02x}{GREEN:02x}{BLUE:02x}\"/>\
             </svg>"
        )
    }

    /// The `(red, green, blue, alpha)` of one pixel of a decoded surface, un-premultiplied.
    ///
    /// Reads through cairo's own `ARgb32` layout — a native-endian `u32` — so the assertion is
    /// about colour rather than about byte order, and holds on any target.
    ///
    /// Takes the surface exclusively because cairo's `data()` does: a surface with more than one
    /// live reference refuses to hand out its bytes.
    fn pixel_at(surface: &mut ImageSurface, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let stride = usize::try_from(surface.stride()).expect("a sane stride");
        let data = surface
            .data()
            .expect("the surface is not borrowed elsewhere");
        let at = y * stride + x * 4;
        let argb = u32::from_ne_bytes(data[at..at + 4].try_into().expect("four bytes"));
        #[allow(clippy::cast_possible_truncation)]
        let channel = |shift: u32| (argb >> shift) as u8;
        (channel(16), channel(8), channel(0), channel(24))
    }

    // --- T032/T034: the two formats, and the channel order (research.md R18) --

    #[test]
    fn a_valid_png_decodes_to_a_surface_of_its_own_size() {
        let files = Files::new();
        let path = files.write("valid.png", &png_bytes(48, 48));
        let mut surface = decode(&path, 20).expect("a valid PNG decodes");

        // Decoded at the file's own size, not the slot's: upscaling here would only blur it, and
        // `place` does the fitting at paint time (FR-039).
        assert_eq!((surface.width(), surface.height()), (48, 48));
        assert_eq!(pixel_at(&mut surface, 24, 24), (RED, GREEN, BLUE, 0xff));
    }

    #[test]
    fn a_valid_svg_is_rasterised_at_the_slot_size() {
        // FR-039's "drawn at the monitor's device resolution": the vector is rendered at the size
        // it will be drawn at rather than at a nominal size and scaled.
        let files = Files::new();
        let path = files.write("valid.svg", svg_bytes(48, 48).as_bytes());
        let mut surface = decode(&path, 40).expect("a valid SVG decodes");

        assert_eq!((surface.width(), surface.height()), (40, 40));
        assert_eq!(
            pixel_at(&mut surface, 20, 20),
            (RED, GREEN, BLUE, 0xff),
            "the tiny_skia to cairo conversion preserves the colour, so the channel order in \
             research.md R18 is the right way round"
        );
    }

    #[test]
    fn a_non_square_icon_keeps_its_aspect_ratio() {
        let files = Files::new();
        let path = files.write("wide.svg", svg_bytes(96, 48).as_bytes());
        let surface = decode(&path, 40).expect("a valid SVG decodes");

        // Twice as wide as tall, fitted to the slot's longer edge: 40 x 20, not 40 x 40.
        assert_eq!((surface.width(), surface.height()), (40, 20));
    }

    #[test]
    fn an_svg_that_is_only_a_viewbox_is_still_rasterised_at_the_slot_size() {
        // Icon sets routinely ship SVGs whose width and height come from the viewBox alone.
        let surface = svg(
            br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#339966"/></svg>"##,
            30,
        )
        .expect("a viewBox-only SVG decodes");
        assert_eq!((surface.width(), surface.height()), (30, 30));
    }

    // --- T034: the failure modes (FR-040a, FR-044) ---------------------------

    #[test]
    fn an_unsupported_extension_is_unresolvable_rather_than_a_fault() {
        // FR-040a: anything that is not PNG or SVG is the placeholder, and the file is never
        // opened — so a huge XPM costs nothing.
        let files = Files::new();
        let path = files.write("icon.xpm", b"/* XPM */");
        assert!(matches!(decode(&path, 20), Err(DecodeError::Unsupported)));
    }

    #[test]
    fn a_truncated_png_is_a_decode_failure_that_names_itself() {
        // FR-044: reported once and cached as the placeholder. The header is intact, so this is a
        // real decode failure rather than an unrecognised file.
        let files = Files::new();
        let whole = png_bytes(48, 48);
        let path = files.write("broken.png", &whole[..whole.len() / 3]);

        let Err(e) = decode(&path, 20) else {
            panic!("a truncated PNG must not decode");
        };
        assert!(matches!(e, DecodeError::Malformed(_)));
        assert!(
            e.to_string().contains("cannot be decoded"),
            "the message says what went wrong, got {e}"
        );
    }

    #[test]
    fn a_truncated_svg_is_a_decode_failure() {
        let files = Files::new();
        let path = files.write("broken.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"");
        assert!(matches!(decode(&path, 20), Err(DecodeError::Malformed(_))));
    }

    #[test]
    fn a_file_that_is_not_there_is_unreadable_rather_than_a_panic() {
        assert!(matches!(
            decode(Path::new("/nowhere/at/all.png"), 20),
            Err(DecodeError::Unreadable(_))
        ));
        assert!(matches!(
            decode(Path::new("/nowhere/at/all.svg"), 20),
            Err(DecodeError::Unreadable(_))
        ));
    }

    // --- T033: fitting an icon into its slot (FR-039) ------------------------

    #[test]
    fn a_square_icon_fills_a_square_slot() {
        assert_eq!(
            place((48.0, 48.0), (10.0, 20.0, 40.0, 40.0)),
            Some((10.0, 20.0, 40.0, 40.0))
        );
    }

    #[test]
    fn a_wide_icon_is_letterboxed_and_centred_rather_than_stretched() {
        let (x, y, width, height) =
            place((96.0, 48.0), (0.0, 0.0, 40.0, 40.0)).expect("a drawable slot");
        assert!((width - 40.0).abs() < f64::EPSILON);
        assert!((height - 20.0).abs() < f64::EPSILON, "half as tall as wide");
        assert!((x - 0.0).abs() < f64::EPSILON);
        assert!((y - 10.0).abs() < f64::EPSILON, "centred in the slot");
    }

    #[test]
    fn a_tall_icon_is_pillarboxed_the_same_way() {
        let (x, y, width, height) =
            place((48.0, 96.0), (0.0, 0.0, 40.0, 40.0)).expect("a drawable slot");
        assert!((width - 20.0).abs() < f64::EPSILON);
        assert!((height - 40.0).abs() < f64::EPSILON);
        assert!((x - 10.0).abs() < f64::EPSILON);
        assert!((y - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_small_icon_is_scaled_up_to_its_slot_rather_than_left_adrift_in_it() {
        // A raster icon whose set has nothing bigger: blurred is still better than a placeholder.
        assert_eq!(
            place((16.0, 16.0), (0.0, 0.0, 40.0, 40.0)),
            Some((0.0, 0.0, 40.0, 40.0))
        );
    }

    #[test]
    fn a_slot_or_a_source_with_no_area_is_declined() {
        assert_eq!(place((48.0, 48.0), (0.0, 0.0, 0.0, 40.0)), None);
        assert_eq!(place((48.0, 48.0), (0.0, 0.0, 40.0, 0.0)), None);
        assert_eq!(place((0.0, 48.0), (0.0, 0.0, 40.0, 40.0)), None);
    }

    #[test]
    fn a_decoded_surfaces_size_is_what_place_takes() {
        let files = Files::new();
        let path = files.write("valid.png", &png_bytes(32, 16));
        let surface = decode(&path, 20).expect("a valid PNG decodes");
        assert_eq!(size_of(&surface), (32.0, 16.0));
    }
}
