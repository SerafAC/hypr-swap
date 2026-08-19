//! Painting the overlay into an shm buffer with cairo and pango (research.md R6).
//!
//! Deliberately logic-free: every number this module draws with comes from [`crate::ui::layout`]
//! and every string from [`crate::ordering`]. It decides only how an entry *looks* — which is why
//! it carries no unit tests and is covered by the E2E suite instead (plan.md → Complexity
//! Tracking). In particular the miniature arithmetic of FR-015a is not here: it is
//! [`crate::ui::layout::miniature_rect`], where SC-008 can be tested without a compositor.
//!
//! Pango is here for one requirement in particular: `ellipsize` truncates an overlong window
//! title with a visible ellipsis and gives the measurement to do it (FR-015b).

use std::fmt::Write as _;

use cairo::{Context, Format, ImageSurface};
use pango::{Alignment, EllipsizeMode};
use pangocairo::functions::{create_layout, show_layout};

use crate::config::Presentation;
use crate::ordering::{Entry, EntryWindow};
use crate::ui::layout::{self, Metrics};

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
/// The accent marking a monitor's active workspace (FR-008).
const ACTIVE_MARK: Rgba = (0.42, 0.72, 0.45, 1.0);
/// Workspace names and window titles.
const TEXT: Rgba = (0.92, 0.92, 0.94, 1.0);
const TEXT_HIGHLIGHTED: Rgba = (1.0, 1.0, 1.0, 1.0);
/// Window titles, which are secondary to the workspace name they follow.
const TEXT_DIM: Rgba = (0.66, 0.66, 0.70, 1.0);
const TEXT_DIM_HIGHLIGHTED: Rgba = (0.86, 0.90, 0.96, 1.0);

/// The miniature panel: the workspace's screen, behind the windows on it (FR-015).
const MINIATURE: Rgba = (0.16, 0.16, 0.19, 1.0);
/// One window inside a miniature, and its outline.
const WINDOW: Rgba = (0.30, 0.32, 0.38, 1.0);
const WINDOW_EDGE: Rgba = (0.52, 0.55, 0.62, 1.0);
/// A floating window, drawn on top of the tiled ones and distinguished from them (research.md R7).
const WINDOW_FLOATING: Rgba = (0.38, 0.40, 0.48, 1.0);

/// Corner radius of the backdrop and of a highlighted row, as a fraction of the row height.
const CORNER: f64 = 0.28;
/// Width of the active-workspace accent bar, as a fraction of the row height.
const MARK_WIDTH: f64 = 0.12;
/// The em size, as a fraction of the row's text line. Leaves room for descenders.
const FONT_FRACTION: f64 = 0.78;
/// The em size of a title inside a miniature, as a fraction of the rectangle holding it — and the
/// height below which a rectangle is too small to letter at all.
const MINIATURE_FONT_FRACTION: f64 = 0.42;
const MINIATURE_MIN_TEXT_HEIGHT: f64 = 9.0;
/// Outline width of a window rectangle, as a fraction of the miniature's height.
const MINIATURE_EDGE: f64 = 0.008;
/// Space between the workspace name and the first window title, as a percentage of the em — the
/// unit pango markup's `size` attribute takes.
const GAP_PERCENT: u32 = 120;
/// What an empty workspace's miniature says, so it reads as empty rather than as broken (FR-007,
/// US3-AS5).
const EMPTY_LABEL: &str = "empty";

/// The stride one row of the overlay occupies, in bytes.
///
/// # Errors
/// Propagates cairo's own refusal for a width it cannot represent.
pub fn stride_for(width: u32) -> Result<i32, cairo::Error> {
    FORMAT.stride_for_width(width)
}

/// Paint the overlay straight into an shm canvas, in whichever presentation `metrics` describes
/// (FR-008, FR-014, FR-015, FR-016).
///
/// `canvas` is the mapped buffer, which must be at least `stride_for(metrics.width) *
/// metrics.height` bytes. `first_visible` is the entry index at the top of the viewport, from
/// [`crate::ui::layout::first_visible_entry`]; `highlight` indexes `entries`, not the viewport.
///
/// # Errors
/// Propagates any cairo failure — a surface that cannot be created or drawn into means the
/// overlay cannot be shown, which the caller reports and abandons the session over.
///
/// # Panics
/// If `canvas` is too small for the metrics given.
pub fn overlay(
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
        paint(&cairo, metrics, entries, first_visible, highlight)?;
    }
    surface.flush();
    drop(surface);
    Ok(())
}

/// Paint into an existing context. Split out so the shm path and any future target share one
/// description of what the overlay looks like.
///
/// The two presentations differ only in how one entry is drawn (FR-016): the backdrop, the
/// viewport slice and the highlight are the same either way.
///
/// # Errors
/// Propagates cairo failures.
pub fn paint(
    cairo: &Context,
    metrics: &Metrics,
    entries: &[Entry],
    first_visible: usize,
    highlight: usize,
) -> Result<(), cairo::Error> {
    backdrop(cairo, metrics)?;
    for slot in 0..metrics.visible_entries() {
        let Some(entry) = entries.get(first_visible + slot) else {
            break;
        };
        let selected = first_visible + slot == highlight;
        match metrics.presentation {
            Presentation::List => paint_row(cairo, metrics, entry, slot, selected)?,
            Presentation::Grid => paint_cell(cairo, metrics, entry, slot, selected)?,
        }
    }
    Ok(())
}

/// The rounded translucent panel every entry is drawn on.
fn backdrop(cairo: &Context, metrics: &Metrics) -> Result<(), cairo::Error> {
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
        f64::from(metrics.row_height) * CORNER,
    );
    set_colour(cairo, BACKDROP);
    cairo.fill()
}

/// One row of the flat list: the workspace name, then the titles of its windows (FR-014).
fn paint_row(
    cairo: &Context,
    metrics: &Metrics,
    entry: &Entry,
    slot: usize,
    selected: bool,
) -> Result<(), cairo::Error> {
    let row_height = f64::from(metrics.row_height);
    let radius = row_height * CORNER;
    let (x, y, width, height) = rect_of(metrics.cell_rect(slot));

    if selected {
        rounded_rect(cairo, x, y, width, height, radius);
        set_colour(cairo, HIGHLIGHT);
        cairo.fill()?;
    }

    // The active workspace of its monitor is marked whether or not it is also highlighted, so the
    // two pieces of information never hide each other (FR-008).
    let mark = row_height * MARK_WIDTH;
    if entry.is_active {
        cairo.rectangle(x, y + row_height * 0.2, mark, row_height * 0.6);
        set_colour(cairo, ACTIVE_MARK);
        cairo.fill()?;
    }
    let text_left = x + mark * 2.0;
    let text_width = width - (text_left - x);
    if text_width <= 0.0 {
        return Ok(());
    }

    // A single ellipsised line, which is what keeps a row exactly one line tall no matter how many
    // windows a workspace holds (FR-019) and truncates visibly rather than overflowing when it
    // does not fit (FR-015b).
    let font_size = f64::from(metrics.text_height) * FONT_FRACTION;
    let layout = line(cairo, text_width, font_size, &row_markup(entry, selected));

    // Centre the line in the row using pango's own measurement rather than the nominal height, so
    // a font with unusual metrics still sits straight.
    let (_, extents) = layout.pixel_extents();
    cairo.move_to(
        text_left,
        y + (row_height - f64::from(extents.height())) / 2.0,
    );
    set_colour(cairo, if selected { TEXT_HIGHLIGHTED } else { TEXT });
    show_layout(cairo, &layout);
    Ok(())
}

/// One grid cell: a schematic miniature of the workspace's layout with its name underneath
/// (FR-015, FR-015a).
fn paint_cell(
    cairo: &Context,
    metrics: &Metrics,
    entry: &Entry,
    slot: usize,
    selected: bool,
) -> Result<(), cairo::Error> {
    let radius = f64::from(metrics.gap.max(1)) * 0.6;
    let (x, y, width, height) = rect_of(metrics.cell_rect(slot));

    if selected {
        rounded_rect(cairo, x, y, width, height, radius);
        set_colour(cairo, HIGHLIGHT);
        cairo.fill()?;
    }

    // The miniature is the workspace's monitor, so it is letterboxed to that monitor's shape
    // inside the fixed cell — a window's proportion is part of what FR-015a asks for.
    let area = metrics.miniature_box(slot, entry.monitor_size);
    rounded_rect(cairo, area.0, area.1, area.2, area.3, radius * 0.5);
    set_colour(cairo, MINIATURE);
    cairo.fill_preserve()?;
    // The active workspace of its monitor is outlined rather than filled differently, so the
    // marking survives being highlighted at the same time (FR-008).
    if entry.is_active {
        set_colour(cairo, ACTIVE_MARK);
        cairo.set_line_width((area.3 * MINIATURE_EDGE * 3.0).max(1.0));
        cairo.stroke()?;
    } else {
        cairo.new_path();
    }

    if entry.windows.is_empty() {
        paint_empty(cairo, metrics, area);
    } else {
        // Floating windows last, so they land on top of the tiled ones exactly as they do on the
        // real workspace; within each group the compositor's own order is preserved (research.md
        // R7).
        let tiled = entry.windows.iter().filter(|window| !window.floating);
        let floating = entry.windows.iter().filter(|window| window.floating);
        for window in tiled.chain(floating) {
            paint_window(cairo, metrics, entry, window, area)?;
        }
    }

    // The workspace name, beneath the miniature it names (FR-015).
    let (label_x, label_y, label_width, label_height) = rect_of(metrics.label_rect(slot));
    if label_width > 0.0 && label_height > 0.0 {
        let font_size = f64::from(metrics.text_height) * FONT_FRACTION;
        let layout = centred(cairo, label_width, font_size, &escape(&entry.label));
        let (_, extents) = layout.pixel_extents();
        cairo.move_to(
            label_x,
            label_y + (label_height - f64::from(extents.height())) / 2.0,
        );
        set_colour(cairo, if selected { TEXT_HIGHLIGHTED } else { TEXT });
        show_layout(cairo, &layout);
    }
    Ok(())
}

/// One window inside a miniature: a rectangle in the position and proportion it occupies on its
/// workspace, labelled with its title (FR-015a, FR-015b).
fn paint_window(
    cairo: &Context,
    metrics: &Metrics,
    entry: &Entry,
    window: &EntryWindow,
    area: (f64, f64, f64, f64),
) -> Result<(), cairo::Error> {
    // A window with no area cannot be drawn as a proportion of anything, so `miniature_rect`
    // declines it and it is skipped rather than painted as a degenerate sliver (SC-008).
    let Some((x, y, width, height)) = layout::miniature_rect(
        window.at,
        window.size,
        entry.monitor_position,
        entry.monitor_size,
        area,
    ) else {
        return Ok(());
    };

    cairo.rectangle(x, y, width, height);
    set_colour(
        cairo,
        if window.floating {
            WINDOW_FLOATING
        } else {
            WINDOW
        },
    );
    cairo.fill_preserve()?;
    set_colour(cairo, WINDOW_EDGE);
    cairo.set_line_width((area.3 * MINIATURE_EDGE).max(1.0));
    cairo.stroke()?;

    // Below a certain size a title is illegible rather than merely small, and drawing it would
    // only smear the rectangle it belongs to. FR-015b is about truncation, which pango does for
    // every size above that.
    let font_size = height * MINIATURE_FONT_FRACTION;
    if font_size < MINIATURE_MIN_TEXT_HEIGHT {
        return Ok(());
    }
    let font_size = font_size.min(f64::from(metrics.text_height) * FONT_FRACTION);
    let inset = (width * 0.06).min(font_size * 0.5);
    let text_width = width - inset * 2.0;
    if text_width <= 0.0 {
        return Ok(());
    }
    let layout = line(cairo, text_width, font_size, &escape(&window.label));
    let (_, extents) = layout.pixel_extents();
    cairo.move_to(x + inset, y + (height - f64::from(extents.height())) / 2.0);
    set_colour(cairo, TEXT);
    show_layout(cairo, &layout);
    Ok(())
}

/// A workspace with no windows, marked as empty rather than left as a blank panel (FR-007,
/// US3-AS5).
fn paint_empty(cairo: &Context, metrics: &Metrics, area: (f64, f64, f64, f64)) {
    let font_size = (f64::from(metrics.text_height) * FONT_FRACTION * 0.8).min(area.3 * 0.3);
    if font_size < MINIATURE_MIN_TEXT_HEIGHT || area.2 <= 0.0 {
        return;
    }
    let layout = centred(cairo, area.2, font_size, EMPTY_LABEL);
    let (_, extents) = layout.pixel_extents();
    cairo.move_to(
        area.0,
        area.1 + (area.3 - f64::from(extents.height())) / 2.0,
    );
    set_colour(cairo, TEXT_DIM);
    show_layout(cairo, &layout);
}

/// One ellipsised line of pango markup, `width` device pixels wide (FR-015b).
fn line(cairo: &Context, width: f64, font_size: f64, markup: &str) -> pango::Layout {
    let layout = create_layout(cairo);

    let mut font = pango::FontDescription::from_string("Sans");
    #[allow(clippy::cast_possible_truncation)]
    font.set_absolute_size(font_size * f64::from(pango::SCALE));
    layout.set_font_description(Some(&font));

    layout.set_ellipsize(EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    #[allow(clippy::cast_possible_truncation)]
    layout.set_width((width * f64::from(pango::SCALE)) as i32);
    layout.set_markup(markup);
    layout
}

/// The same, centred in its width — how a miniature's label sits under it.
fn centred(cairo: &Context, width: f64, font_size: f64, markup: &str) -> pango::Layout {
    let layout = line(cairo, width, font_size, markup);
    layout.set_alignment(Alignment::Center);
    layout
}

/// `<b>name</b>  title · title`, with everything the compositor reported escaped.
fn row_markup(entry: &Entry, selected: bool) -> String {
    let dim = if selected {
        TEXT_DIM_HIGHLIGHTED
    } else {
        TEXT_DIM
    };
    let mut markup = format!("<b>{}</b>", escape(&entry.label));
    if !entry.windows.is_empty() {
        let titles = entry
            .windows
            .iter()
            .map(|window| escape(&window.label))
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

/// Window titles and workspace names are arbitrary user data and routinely contain `&` and `<` —
/// a shell prompt or a browser tab is enough — so escaping is correctness, not caution.
fn escape(text: &str) -> String {
    pango::glib::markup_escape_text(text).to_string()
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

/// A device-pixel rectangle as the floating-point one cairo draws with.
fn rect_of(rect: (u32, u32, u32, u32)) -> (f64, f64, f64, f64) {
    (
        f64::from(rect.0),
        f64::from(rect.1),
        f64::from(rect.2),
        f64::from(rect.3),
    )
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
