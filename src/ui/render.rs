//! Painting the overlay into an shm buffer with cairo and pango (research.md R6).
//!
//! Deliberately logic-free: every number this module draws with comes from [`crate::ui::layout`]
//! and every string from [`crate::ordering`]. It decides only how a row *looks* — which is why it
//! carries no unit tests and is covered by the E2E suite instead (plan.md → Complexity Tracking).
//!
//! Pango is here for one requirement in particular: `ellipsize` truncates an overlong window
//! title with a visible ellipsis and gives the measurement to do it (FR-015b).

use std::fmt::Write as _;

use cairo::{Context, Format, ImageSurface};
use pango::EllipsizeMode;
use pangocairo::functions::{create_layout, show_layout};

use crate::ordering::Entry;
use crate::ui::layout::Metrics;

/// The buffer format the overlay is painted into: pre-multiplied ARGB, so the surface can be
/// translucent over whatever it covers.
pub const FORMAT: Format = Format::ARgb32;

/// A colour as cairo takes it.
type Rgba = (f64, f64, f64, f64);

/// The overlay's backdrop. Translucent, because FR-019's 80 % cap only reads as an overlay if
/// what is underneath still shows through.
const BACKDROP: Rgba = (0.09, 0.09, 0.11, 0.93);
/// The highlighted entry (FR-008).
const HIGHLIGHT: Rgba = (0.20, 0.42, 0.72, 1.0);
/// The accent bar marking a monitor's active workspace (FR-008).
const ACTIVE_MARK: Rgba = (0.42, 0.72, 0.45, 1.0);
/// Workspace names and window titles.
const TEXT: Rgba = (0.92, 0.92, 0.94, 1.0);
const TEXT_HIGHLIGHTED: Rgba = (1.0, 1.0, 1.0, 1.0);
/// Window titles, which are secondary to the workspace name they follow.
const TEXT_DIM: Rgba = (0.66, 0.66, 0.70, 1.0);
const TEXT_DIM_HIGHLIGHTED: Rgba = (0.86, 0.90, 0.96, 1.0);

/// Corner radius of the backdrop and of a highlighted row, as a fraction of the row height.
const CORNER: f64 = 0.28;
/// Width of the active-workspace accent bar, as a fraction of the row height.
const MARK_WIDTH: f64 = 0.12;
/// The em size, as a fraction of the row's text line. Leaves room for descenders.
const FONT_FRACTION: f64 = 0.78;
/// Space between the workspace name and the first window title, as a percentage of the em — the
/// unit pango markup's `size` attribute takes.
const GAP_PERCENT: u32 = 120;

/// The stride one row of the overlay occupies, in bytes.
///
/// # Errors
/// Propagates cairo's own refusal for a width it cannot represent.
pub fn stride_for(width: u32) -> Result<i32, cairo::Error> {
    FORMAT.stride_for_width(width)
}

/// Paint the flat-list presentation straight into an shm canvas (FR-008, FR-014, FR-015b).
///
/// `canvas` is the mapped buffer, which must be at least `stride_for(metrics.width) *
/// metrics.height` bytes. `first_visible` is the entry index at the top of the viewport, from
/// [`crate::ui::layout::first_visible`]; `highlight` indexes `entries`, not the viewport.
///
/// # Errors
/// Propagates any cairo failure — a surface that cannot be created or drawn into means the
/// overlay cannot be shown, which the caller reports and abandons the session over.
///
/// # Panics
/// If `canvas` is too small for the metrics given.
pub fn list(
    canvas: &mut [u8],
    metrics: &Metrics,
    entries: &[Entry],
    first_visible: usize,
    highlight: usize,
) -> Result<(), cairo::Error> {
    let width = i32::try_from(metrics.width).map_err(|_| cairo::Error::InvalidSize)?;
    let height = i32::try_from(metrics.height).map_err(|_| cairo::Error::InvalidSize)?;
    let stride = stride_for(metrics.width)?;

    let needed = usize::try_from(stride).unwrap_or(0) * metrics.height as usize;
    assert!(
        canvas.len() >= needed,
        "overlay canvas is {} bytes, needs {needed}",
        canvas.len()
    );

    // SAFETY: the surface borrows `canvas` for strictly less than this function's body — it is
    // dropped below, before `canvas`'s borrow ends — and the length assertion above guarantees
    // the region cairo is told about is inside it. Painting into the shm buffer directly is what
    // keeps a redraw free of a full-overlay memcpy.
    let surface = unsafe {
        ImageSurface::create_for_data_unsafe(canvas.as_mut_ptr(), FORMAT, width, height, stride)?
    };
    {
        let cairo = Context::new(&surface)?;
        paint_list(&cairo, metrics, entries, first_visible, highlight)?;
    }
    surface.flush();
    drop(surface);
    Ok(())
}

/// Paint into an existing context. Split out so the shm path and any future target share one
/// description of what the overlay looks like.
///
/// # Errors
/// Propagates cairo failures.
pub fn paint_list(
    cairo: &Context,
    metrics: &Metrics,
    entries: &[Entry],
    first_visible: usize,
    highlight: usize,
) -> Result<(), cairo::Error> {
    let row_height = f64::from(metrics.row_height);
    let radius = row_height * CORNER;

    // Start from fully transparent: the corners outside the backdrop must not be painted.
    cairo.set_operator(cairo::Operator::Source);
    cairo.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cairo.paint()?;
    cairo.set_operator(cairo::Operator::Over);

    rounded_rect(
        cairo,
        0.0,
        0.0,
        f64::from(metrics.width),
        f64::from(metrics.height),
        radius,
    );
    set_colour(cairo, BACKDROP);
    cairo.fill()?;

    for slot in 0..metrics.visible_rows {
        let Some(entry) = entries.get(first_visible + slot) else {
            break;
        };
        let selected = first_visible + slot == highlight;
        let (x, y, width, height) = metrics.row_rect(slot);
        let (x, y, width, height) = (
            f64::from(x),
            f64::from(y),
            f64::from(width),
            f64::from(height),
        );

        if selected {
            rounded_rect(cairo, x, y, width, height, radius);
            set_colour(cairo, HIGHLIGHT);
            cairo.fill()?;
        }

        // The active workspace of its monitor is marked whether or not it is also highlighted,
        // so the two pieces of information never hide each other (FR-008).
        let mark = row_height * MARK_WIDTH;
        let text_left = if entry.is_active {
            cairo.rectangle(x, y + row_height * 0.2, mark, row_height * 0.6);
            set_colour(cairo, ACTIVE_MARK);
            cairo.fill()?;
            x + mark * 2.0
        } else {
            x + mark * 2.0
        };

        row_text(
            cairo,
            metrics,
            entry,
            selected,
            text_left,
            y,
            width - (text_left - x),
        );
    }

    Ok(())
}

/// One row's text: the workspace name, then the titles of its windows (FR-014).
///
/// Laid out as a single ellipsised line, which is what keeps a row exactly one line tall no
/// matter how many windows a workspace holds (FR-019) and truncates visibly rather than
/// overflowing when it does not fit (FR-015b).
fn row_text(
    cairo: &Context,
    metrics: &Metrics,
    entry: &Entry,
    selected: bool,
    x: f64,
    y: f64,
    width: f64,
) {
    if width <= 0.0 {
        return;
    }
    let layout = create_layout(cairo);
    let font_size = f64::from(metrics.text_height) * FONT_FRACTION;

    let mut font = pango::FontDescription::from_string("Sans");
    #[allow(clippy::cast_possible_truncation)]
    font.set_absolute_size(font_size * f64::from(pango::SCALE));
    layout.set_font_description(Some(&font));

    // A single ellipsised line: pango measures and truncates, so no title can push the row out
    // of shape (FR-015b).
    layout.set_ellipsize(EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    #[allow(clippy::cast_possible_truncation)]
    layout.set_width((width * f64::from(pango::SCALE)) as i32);
    layout.set_markup(&markup(entry, selected));

    // Centre the line in the row using pango's own measurement rather than the nominal height,
    // so a font with unusual metrics still sits straight.
    let (_, logical) = layout.pixel_extents();
    let text_y = y + (f64::from(metrics.row_height) - f64::from(logical.height())) / 2.0;

    cairo.move_to(x, text_y);
    set_colour(cairo, if selected { TEXT_HIGHLIGHTED } else { TEXT });
    show_layout(cairo, &layout);
}

/// `<b>name</b>  title · title`, with everything the compositor reported escaped.
///
/// Window titles are arbitrary user data and routinely contain `&` and `<` — a shell prompt or a
/// browser tab is enough — so escaping is correctness, not caution.
fn markup(entry: &Entry, selected: bool) -> String {
    let dim = if selected {
        TEXT_DIM_HIGHLIGHTED
    } else {
        TEXT_DIM
    };
    let mut markup = format!(
        "<b>{}</b>",
        pango::glib::markup_escape_text(&entry.label).as_str()
    );
    if !entry.windows.is_empty() {
        let titles = entry
            .windows
            .iter()
            .map(|window| pango::glib::markup_escape_text(&window.label).to_string())
            .collect::<Vec<_>>()
            .join("  ·  ");
        // Writing into a `String` cannot fail, so the result is genuinely nothing to handle.
        let _ = write!(
            markup,
            "<span size=\"{GAP_PERCENT}%\"> </span><span foreground=\"{}\">{titles}</span>",
            hex(dim),
        );
    }
    markup
}

fn hex(colour: Rgba) -> String {
    // Clamped to 0.0..=1.0 first, so the scaled value is always inside `u8`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(colour.0),
        channel(colour.1),
        channel(colour.2)
    )
}

fn set_colour(cairo: &Context, colour: Rgba) {
    cairo.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
}

/// A rectangle with rounded corners, as a path ready to fill.
fn rounded_rect(cairo: &Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let (right, bottom) = (x + width, y + height);
    let quarter = std::f64::consts::FRAC_PI_2;

    cairo.new_sub_path();
    cairo.arc(right - radius, y + radius, radius, -quarter, 0.0);
    cairo.arc(right - radius, bottom - radius, radius, 0.0, quarter);
    cairo.arc(x + radius, bottom - radius, radius, quarter, 2.0 * quarter);
    cairo.arc(x + radius, y + radius, radius, 2.0 * quarter, 3.0 * quarter);
    cairo.close_path();
}
