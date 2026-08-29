//! Painting the overlay into an shm buffer with cairo and pango (research.md R6).
//!
//! Deliberately logic-free: every number this module draws with comes from [`crate::ui::layout`]
//! or [`crate::theme`], and every string from [`crate::ordering`]. It decides only how an entry
//! *looks* — and since feature 002 not even which colour, font or corner radius it looks it in:
//! those arrive already resolved on a [`Style`], so the FR-050 precedence chain is applied once
//! and this module carries no default of its own (FR-045–FR-047, FR-060). Which is why
//! it carries no unit tests and is covered by the E2E suite instead (plan.md → Complexity
//! Tracking). In particular the miniature arithmetic of FR-015a is not here: it is
//! [`crate::ui::layout::miniature_rect`], where SC-008 can be tested without a compositor.
//!
//! Pango is here for one requirement in particular: `ellipsize` truncates an overlong window
//! title with a visible ellipsis and gives the measurement to do it (FR-015b).

use std::fmt::Write as _;

use cairo::{Context, Format, ImageSurface};
use pango::prelude::FontExt as _;
use pango::{Alignment, EllipsizeMode};
use pangocairo::functions::{create_layout, show_layout};

use crate::config::Presentation;
use crate::diag;
use crate::icons::{Drawn, IconStore, decode};
use crate::ordering::{Entry, EntryWindow};
use crate::theme::{Colour, Style};
use crate::ui::layout::{self, Metrics};

/// The buffer format the overlay is painted into: pre-multiplied ARGB, so the surface can be
/// translucent over whatever it covers.
pub const FORMAT: Format = Format::ARgb32;

/// Outline width of a window rectangle, as a fraction of the miniature's height.
const MINIATURE_EDGE: f64 = 0.008;
/// Space between the workspace name and the first window title, as a percentage of the em — the
/// unit pango markup's `size` attribute takes.
const GAP_PERCENT: u32 = 120;
/// What an empty workspace's miniature says, so it reads as empty rather than as broken (FR-007,
/// US3-AS5).
const EMPTY_LABEL: &str = "empty";

/// U+FFFC OBJECT REPLACEMENT CHARACTER: the stand-in one icon occupies in the row's text.
///
/// The row stays a single pango layout, and each icon is a character in it with a shaped
/// attribute reserving its box. That is what keeps pango's own ellipsisation — the property
/// FR-036 relies on for "exactly one line" and FR-036a for "truncates visibly" — working with
/// icons in the line rather than around them (research.md R23).
const ICON_MARK: &str = "\u{FFFC}";

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
    style: &Style,
    icons: &IconStore,
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
        paint(
            &cairo,
            style,
            icons,
            metrics,
            entries,
            first_visible,
            highlight,
        )?;
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
    style: &Style,
    icons: &IconStore,
    metrics: &Metrics,
    entries: &[Entry],
    first_visible: usize,
    highlight: usize,
) -> Result<(), cairo::Error> {
    // Read once per paint rather than once per entry: the gate is closed in every run that is not
    // an E2E one, and this is the whole of what it costs then (research.md R22).
    let record = diag::paint_records_enabled();
    arm_colour_tape(record);
    arm_font_tape(record);
    let presentation = match metrics.presentation {
        Presentation::List => "list",
        Presentation::Grid => "grid",
    };
    backdrop(cairo, style, metrics)?;
    for slot in 0..metrics.visible_entries() {
        let Some(entry) = entries.get(first_visible + slot) else {
            break;
        };
        let index = first_visible + slot;
        let selected = index == highlight;
        let painted = match metrics.presentation {
            Presentation::List => paint_row(cairo, style, icons, metrics, entry, slot, selected)?,
            Presentation::Grid => paint_cell(cairo, style, icons, metrics, entry, slot, selected)?,
        };
        if record {
            diag::paint(index, presentation, &drawn(entry, selected, &painted));
        }
    }
    // Every colour this paint reached the buffer with, once the whole overlay is on it — the
    // evidence that a named theme recoloured every element and left none behind (FR-045, FR-048).
    if let Some(colours) = take_colour_tape() {
        diag::paint_colours(presentation, &colours);
    }
    // And every family this paint laid text out in, asked for and loaded — the evidence that a
    // font override reached all of the overlay's text, in either presentation (FR-046).
    if let Some((requested, resolved)) = take_font_tape() {
        diag::paint_fonts(presentation, &requested, &resolved);
    }
    Ok(())
}

/// What painting one entry actually produced, beyond the pixels — the evidence a visual
/// requirement is met (research.md R22).
///
/// Collected whether or not the gate is open, because it is three counters and a `Vec` that is
/// only filled when the gate asks for it; branching on the gate inside the paint path would put
/// the check in the inner loop instead of once per paint.
#[derive(Debug, Default)]
struct Painted {
    /// One entry per icon actually drawn, naming the file it came from or `placeholder`.
    icons: Vec<String>,
    /// Icons whose reserved slot fell past the line's ellipsis and so were not drawn (FR-036a).
    shed: usize,
    /// Whether pango truncated the line, i.e. whether the row shows an ellipsis (FR-036a).
    ellipsized: bool,
    /// Device pixels the icons took from the text on this row — the measure of "names truncate
    /// sooner than the same row without icons" (FR-036a).
    icon_width: u32,
    /// One entry per window rectangle drawn in a miniature, naming what that rectangle had room
    /// for, in the order the rectangles were drawn (FR-038).
    ///
    /// The grid's counterpart to `ellipsized`: FR-038's shedding is a decision about content that
    /// leaves no trace in the geometry, so what the rectangle *held* is the only evidence of it.
    rects: Vec<&'static str>,
}

/// What one window rectangle in a miniature ended up holding, as the paint record names it
/// (FR-038, research.md R22).
fn held(content: &layout::MiniatureContent) -> &'static str {
    match (content.icon.is_some(), content.title.is_some()) {
        (true, true) => "icon+title",
        (true, false) => "icon",
        // Reachable only with icons turned off, where there is no icon to shed (FR-056).
        (false, true) => "title",
        (false, false) => "none",
    }
}

/// What one painted entry is recorded as, under the environment gate (research.md R22).
///
/// This feature's requirements are visual, and screenshot comparison stays rejected, so what the
/// renderer *did* is the only evidence an E2E test can assert on. Says what the entry was, how it
/// was drawn, and which icon file answered for each of its windows.
fn drawn(entry: &Entry, selected: bool, painted: &Painted) -> String {
    format!(
        "label={:?} windows={} active={} highlighted={} icons=[{}] shed={} ellipsized={} \
         icon_width={} rects=[{}]",
        entry.label,
        entry.windows.len(),
        entry.is_active,
        selected,
        painted.icons.join(" "),
        painted.shed,
        painted.ellipsized,
        painted.icon_width,
        painted.rects.join(" "),
    )
}

/// The rounded translucent panel every entry is drawn on.
fn backdrop(cairo: &Context, style: &Style, metrics: &Metrics) -> Result<(), cairo::Error> {
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
        f64::from(metrics.row_height) * style.geometry.corner_radius,
    );
    set_colour(cairo, style.palette.backdrop);
    cairo.fill()
}

/// One row of the flat list: the workspace name, then each window's icon and title (FR-014,
/// FR-035, FR-036).
///
/// Returns what was drawn, for the paint record (research.md R22).
fn paint_row(
    cairo: &Context,
    style: &Style,
    icons: &IconStore,
    metrics: &Metrics,
    entry: &Entry,
    slot: usize,
    selected: bool,
) -> Result<Painted, cairo::Error> {
    let mut painted = Painted::default();
    let row_height = f64::from(metrics.row_height);
    let radius = row_height * style.geometry.corner_radius;
    let (x, y, width, height) = rect_of(metrics.cell_rect(slot));

    if selected {
        rounded_rect(cairo, x, y, width, height, radius);
        set_colour(cairo, style.palette.highlight);
        cairo.fill()?;
    }

    // The active workspace of its monitor is marked whether or not it is also highlighted, so the
    // two pieces of information never hide each other (FR-008).
    let mark = row_height * style.geometry.mark_width;
    if entry.is_active {
        cairo.rectangle(x, y + row_height * 0.2, mark, row_height * 0.6);
        set_colour(cairo, style.palette.active_mark);
        cairo.fill()?;
    }
    let text_left = x + mark * 2.0;
    let text_width = width - (text_left - x);
    if text_width <= 0.0 {
        return Ok(painted);
    }

    // A single ellipsised line, which is what keeps a row exactly one line tall no matter how many
    // windows a workspace holds (FR-019) and truncates visibly rather than overflowing when it
    // does not fit (FR-015b). The icons live *inside* that line rather than beside it, so the
    // names ellipsise around them (FR-036a, research.md R23).
    let font_size = f64::from(metrics.text_height) * style.text_size;
    let reserve = icons.enabled().then(|| {
        (
            f64::from(metrics.icon_slot()),
            f64::from(metrics.icon_advance()),
        )
    });
    let (layout, slots) = line(
        cairo,
        style,
        text_width,
        font_size,
        &row_markup(style, entry, selected, reserve.is_some()),
        reserve,
    );

    // Centre the line in the row using pango's own measurement rather than the nominal height, so
    // a font with unusual metrics still sits straight.
    let (_, extents) = layout.pixel_extents();
    let top = y + (row_height - f64::from(extents.height())) / 2.0;
    cairo.move_to(text_left, top);
    set_colour(cairo, primary_text(style, selected));
    show_layout(cairo, &layout);

    painted.ellipsized = layout.is_ellipsized();
    if let Some((icon_slot, advance)) = reserve {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss
        )]
        {
            painted.icon_width = (advance * entry.windows.len() as f64) as u32;
        }
        paint_row_icons(
            cairo,
            icons,
            &layout,
            &slots,
            entry,
            (text_left, top),
            (icon_slot, f64::from(extents.width())),
            primary_text(style, selected),
            &mut painted,
        )?;
    }
    Ok(painted)
}

/// Draw each window's icon into the slot pango reserved for it (research.md R23, FR-035).
///
/// `slots` are the byte offsets of the reserved characters, in the same order as `entry.windows`,
/// and `origin` is where the layout was drawn. An icon whose slot fell past the line's ellipsis is
/// skipped rather than drawn on top of it: the row must stay one visibly-truncated line, and
/// FR-036a is explicit that names truncate sooner rather than the row overflowing.
#[allow(clippy::too_many_arguments)]
fn paint_row_icons(
    cairo: &Context,
    icons: &IconStore,
    layout: &pango::Layout,
    slots: &[usize],
    entry: &Entry,
    origin: (f64, f64),
    sizes: (f64, f64),
    tint: Colour,
    painted: &mut Painted,
) -> Result<(), cairo::Error> {
    let (left, top) = origin;
    let (icon_slot, line_width) = sizes;
    for (window, at) in entry.windows.iter().zip(slots) {
        let Ok(index) = i32::try_from(*at) else {
            painted.shed += 1;
            continue;
        };
        let position = layout.index_to_pos(index);
        let (offset_x, offset_y) = (
            f64::from(position.x()) / f64::from(pango::SCALE),
            f64::from(position.y()) / f64::from(pango::SCALE),
        );
        // Past the ellipsis: pango laid the character out beyond what it actually drew, so the
        // icon has nowhere to go (FR-036a).
        if offset_x < 0.0 || offset_x + icon_slot > line_width {
            painted.shed += 1;
            continue;
        }

        let drawn = icons.get(&window.class);
        blit(
            cairo,
            drawn,
            (left + offset_x, top + offset_y, icon_slot, icon_slot),
            tint,
        )?;
        painted.icons.push(drawn.source.to_owned());
    }
    Ok(())
}

/// Blit one icon into a rectangle, fitted without distortion (FR-039).
///
/// A program's own artwork is drawn exactly as supplied; only the placeholder is recoloured, and
/// it is drawn as a mask so it takes the theme's primary text colour (FR-051).
fn blit(
    cairo: &Context,
    drawn: Drawn<'_>,
    rect: (f64, f64, f64, f64),
    tint: Colour,
) -> Result<(), cairo::Error> {
    let Some(surface) = drawn.surface else {
        return Ok(());
    };
    let source = decode::size_of(surface);
    let Some((x, y, width, height)) = decode::place(source, rect) else {
        return Ok(());
    };

    cairo.save()?;
    cairo.translate(x, y);
    cairo.scale(width / source.0, height / source.1);
    if drawn.placeholder {
        // The mask uses the surface's alpha and the current source colour, which is exactly
        // FR-051's "the placeholder follows the theme's primary text colour".
        set_colour(cairo, tint);
        cairo.mask_surface(surface, 0.0, 0.0)?;
    } else {
        cairo.set_source_surface(surface, 0.0, 0.0)?;
        cairo.paint()?;
    }
    cairo.restore()
}

/// One grid cell: a schematic miniature of the workspace's layout with its name underneath
/// (FR-015, FR-015a).
///
/// Returns what was drawn, for the paint record (research.md R22).
fn paint_cell(
    cairo: &Context,
    style: &Style,
    icons: &IconStore,
    metrics: &Metrics,
    entry: &Entry,
    slot: usize,
    selected: bool,
) -> Result<Painted, cairo::Error> {
    let mut painted = Painted::default();
    let radius = f64::from(metrics.gap.max(1)) * 0.6;
    let (x, y, width, height) = rect_of(metrics.cell_rect(slot));

    if selected {
        rounded_rect(cairo, x, y, width, height, radius);
        set_colour(cairo, style.palette.highlight);
        cairo.fill()?;
    }

    // The miniature is the workspace's monitor, so it is letterboxed to that monitor's shape
    // inside the fixed cell — a window's proportion is part of what FR-015a asks for.
    let area = metrics.miniature_box(slot, entry.monitor_size);
    rounded_rect(cairo, area.0, area.1, area.2, area.3, radius * 0.5);
    set_colour(cairo, style.palette.miniature);
    cairo.fill_preserve()?;
    // The active workspace of its monitor is outlined rather than filled differently, so the
    // marking survives being highlighted at the same time (FR-008).
    if entry.is_active {
        set_colour(cairo, style.palette.active_mark);
        cairo.set_line_width((area.3 * MINIATURE_EDGE * 3.0).max(1.0));
        cairo.stroke()?;
    } else {
        cairo.new_path();
    }

    if entry.windows.is_empty() {
        paint_empty(cairo, style, metrics, area);
    } else {
        // Floating windows last, so they land on top of the tiled ones exactly as they do on the
        // real workspace; within each group the compositor's own order is preserved (research.md
        // R7).
        let tiled = entry.windows.iter().filter(|window| !window.floating);
        let floating = entry.windows.iter().filter(|window| window.floating);
        for window in tiled.chain(floating) {
            paint_window(
                cairo,
                style,
                icons,
                metrics,
                entry,
                window,
                area,
                &mut painted,
            )?;
        }
    }

    // The workspace name, beneath the miniature it names (FR-015).
    let (label_x, label_y, label_width, label_height) = rect_of(metrics.label_rect(slot));
    if label_width > 0.0 && label_height > 0.0 {
        let font_size = f64::from(metrics.text_height) * style.text_size;
        let layout = centred(cairo, style, label_width, font_size, &escape(&entry.label));
        let (_, extents) = layout.pixel_extents();
        cairo.move_to(
            label_x,
            label_y + (label_height - f64::from(extents.height())) / 2.0,
        );
        set_colour(cairo, primary_text(style, selected));
        show_layout(cairo, &layout);
    }
    Ok(painted)
}

/// One window inside a miniature: a rectangle in the position and proportion it occupies on its
/// workspace, holding its program's icon and its title (FR-015a, FR-015b, FR-037, FR-038).
#[allow(clippy::too_many_arguments)]
fn paint_window(
    cairo: &Context,
    style: &Style,
    icons: &IconStore,
    metrics: &Metrics,
    entry: &Entry,
    window: &EntryWindow,
    area: (f64, f64, f64, f64),
    painted: &mut Painted,
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
            style.palette.window_floating
        } else {
            style.palette.window
        },
    );
    cairo.fill_preserve()?;
    set_colour(cairo, style.palette.window_edge);
    cairo.set_line_width((area.3 * MINIATURE_EDGE).max(1.0));
    cairo.stroke()?;

    // The rectangle is on the buffer before anything is asked about what fits inside it: FR-038
    // sheds content from a rectangle too small for it, never the rectangle itself.
    let text_cap = f64::from(metrics.text_height) * style.text_size;
    let content = metrics.miniature_content((x, y, width, height), text_cap, icons.enabled());

    if icons.enabled() {
        if let Some(rect) = content.icon {
            let drawn = icons.get(&window.class);
            blit(cairo, drawn, rect, style.palette.text)?;
            painted.icons.push(drawn.source.to_owned());
        } else {
            painted.shed += 1;
        }
    }

    // FR-015b is about truncation, which pango does at every size the title survives to.
    if let Some(title) = content.title {
        let (text_x, text_y, text_width, text_height) = title.rect;
        let (layout, _) = line(
            cairo,
            style,
            text_width,
            title.font_size,
            &escape(&window.label),
            None,
        );
        let (_, extents) = layout.pixel_extents();
        cairo.move_to(
            text_x,
            text_y + (text_height - f64::from(extents.height())) / 2.0,
        );
        set_colour(cairo, style.palette.text);
        show_layout(cairo, &layout);
        // A title kept but too long for its rectangle is truncated with a visible indication
        // rather than overflowing it, exactly as a row is (FR-015b).
        painted.ellipsized |= layout.is_ellipsized();
    }

    painted.rects.push(held(&content));
    Ok(())
}

/// A workspace with no windows, marked as empty rather than left as a blank panel (FR-007,
/// US3-AS5).
fn paint_empty(cairo: &Context, style: &Style, metrics: &Metrics, area: (f64, f64, f64, f64)) {
    let font_size = (f64::from(metrics.text_height) * style.text_size * 0.8).min(area.3 * 0.3);
    if font_size < layout::MINIATURE_MIN_TEXT_HEIGHT || area.2 <= 0.0 {
        return;
    }
    let layout = centred(cairo, style, area.2, font_size, EMPTY_LABEL);
    let (_, extents) = layout.pixel_extents();
    cairo.move_to(
        area.0,
        area.1 + (area.3 - f64::from(extents.height())) / 2.0,
    );
    set_colour(cairo, style.palette.text_dim);
    show_layout(cairo, &layout);
}

/// One ellipsised line of pango markup, `width` device pixels wide (FR-015b), with a box
/// reserved for each [`ICON_MARK`] in it when `reserve` is given.
///
/// Returns the byte offset of each reserved box in the laid-out text, so the caller can ask
/// `index_to_pos` where pango actually put it (research.md R23).
///
/// `reserve` is `(slot, advance)` in device pixels: the icon's own square, and the width it costs
/// the line including the gap that separates it from the name it precedes.
fn line(
    cairo: &Context,
    style: &Style,
    width: f64,
    font_size: f64,
    markup: &str,
    reserve: Option<(f64, f64)>,
) -> (pango::Layout, Vec<usize>) {
    let layout = create_layout(cairo);

    // An absent family is the platform's business to substitute, and nothing is reported about it
    // (FR-046, US4-AS5).
    let mut font = pango::FontDescription::from_string(&style.font_family);
    #[allow(clippy::cast_possible_truncation)]
    font.set_absolute_size(font_size * f64::from(pango::SCALE));
    layout.set_font_description(Some(&font));
    note_font(&layout, &font);

    layout.set_ellipsize(EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    #[allow(clippy::cast_possible_truncation)]
    layout.set_width((width * f64::from(pango::SCALE)) as i32);

    // With nothing to reserve the layout is the markup and nothing else, which is every path this
    // module had before icons existed and every path the grid still takes.
    let Some((slot, advance)) = reserve else {
        layout.set_markup(markup);
        return (layout, Vec::new());
    };
    // `set_markup` and `set_attributes` are exclusive — the second discards the first — so the
    // markup is parsed by hand and the shapes are merged into the attribute list it yields
    // (research.md R23).
    let Ok((attributes, text, _)) = pango::parse_markup(markup, '\0') else {
        layout.set_markup(markup);
        return (layout, Vec::new());
    };

    let units = |value: f64| {
        #[allow(clippy::cast_possible_truncation)]
        {
            (value * f64::from(pango::SCALE)) as i32
        }
    };
    // Both rectangles are relative to the text's baseline, so the box hangs above it by its own
    // height and the icon sits on the line rather than under it. The ink box is the icon itself;
    // the logical box is what it costs the line, which is what makes the names ellipsise sooner
    // (FR-036a).
    let ink = pango::Rectangle::new(0, -units(slot), units(slot), units(slot));
    let logical = pango::Rectangle::new(0, -units(slot), units(advance), units(slot));

    let slots: Vec<usize> = text.match_indices(ICON_MARK).map(|(at, _)| at).collect();
    for at in &slots {
        let mut shape = pango::AttrShape::new(&ink, &logical);
        #[allow(clippy::cast_possible_truncation)]
        {
            shape.set_start_index(*at as u32);
            shape.set_end_index((*at + ICON_MARK.len()) as u32);
        }
        attributes.insert(shape);
    }

    layout.set_text(&text);
    layout.set_attributes(Some(&attributes));
    (layout, slots)
}

/// The same, centred in its width and with nothing reserved — how a miniature's label sits under
/// it.
fn centred(
    cairo: &Context,
    style: &Style,
    width: f64,
    font_size: f64,
    markup: &str,
) -> pango::Layout {
    let (layout, _) = line(cairo, style, width, font_size, markup, None);
    layout.set_alignment(Alignment::Center);
    layout
}

/// `<b>name</b>  ⬚title · ⬚title`, with everything the compositor reported escaped.
///
/// With `icons` set, each window's title is preceded by an [`ICON_MARK`] — the character whose box
/// the icon is later drawn into (FR-036, research.md R23). The workspace name gets none: it is not
/// a window, and FR-036 puts an icon before a *window's* name.
fn row_markup(style: &Style, entry: &Entry, selected: bool, icons: bool) -> String {
    let dim = if selected {
        style.palette.text_dim_highlighted
    } else {
        style.palette.text_dim
    };
    // Secondary text is the one themed colour that reaches the buffer through pango markup rather
    // than through `set_colour`, so it is noted here or it is missing from the tape entirely.
    note_colour(dim);
    let mark = if icons { ICON_MARK } else { "" };
    let mut markup = format!("<b>{}</b>", escape(&entry.label));
    if !entry.windows.is_empty() {
        let titles = entry
            .windows
            .iter()
            .map(|window| format!("{mark}{}", escape(&window.label)))
            .collect::<Vec<_>>()
            .join("  ·  ");
        // Writing into a `String` cannot fail, so the result is genuinely nothing to handle.
        let _ = write!(
            markup,
            "<span size=\"{GAP_PERCENT}%\"> </span><span foreground=\"{}\">{titles}</span>",
            dim.hex(),
        );
    }
    markup
}

/// Window titles and workspace names are arbitrary user data and routinely contain `&` and `<` —
/// a shell prompt or a browser tab is enough — so escaping is correctness, not caution.
fn escape(text: &str) -> String {
    pango::glib::markup_escape_text(text).to_string()
}

/// Primary entry text, in whichever of its two states the entry is in (FR-045).
fn primary_text(style: &Style, selected: bool) -> Colour {
    if selected {
        style.palette.text_highlighted
    } else {
        style.palette.text
    }
}

fn set_colour(cairo: &Context, colour: Colour) {
    note_colour(colour);
    let (red, green, blue, alpha) = colour.rgba();
    cairo.set_source_rgba(red, green, blue, alpha);
}

// --- The colour tape (T058, research.md R22) --------------------------------
//
// A theme's requirements are about what reaches the screen, and screenshot comparison stays
// rejected, so the evidence has to be taken where the colour is actually handed to cairo rather
// than where it was resolved. `set_colour` is that single point, and the tape below is what it
// writes to: armed once per paint from the gate `paint` already read, and `None` — a thread-local
// read and a null check — in every run that is not an E2E one.
//
// Thread-local rather than threaded through the paint functions because the alternative is a
// `&mut` parameter on every drawing helper paired with a push beside every `set_colour` call,
// which is both more code and weaker evidence: a paired push can disagree with the call it sits
// next to, and a tap inside `set_colour` cannot.

thread_local! {
    /// The colours handed to cairo during the current paint, in first-use order, or `None` when
    /// the gate is shut.
    static COLOUR_TAPE: std::cell::RefCell<Option<Vec<Colour>>> =
        const { std::cell::RefCell::new(None) };
}

/// Start (or discard) a tape for the paint about to begin.
fn arm_colour_tape(recording: bool) {
    COLOUR_TAPE.with_borrow_mut(|tape| *tape = recording.then(Vec::new));
}

/// Note a colour on the tape, if one is running. Distinct values only: the record is "which
/// colours the overlay was drawn in", and an entry drawn twice says nothing more than once.
fn note_colour(colour: Colour) {
    COLOUR_TAPE.with_borrow_mut(|tape| {
        if let Some(tape) = tape.as_mut()
            && !tape.contains(&colour)
        {
            tape.push(colour);
        }
    });
}

/// Take the finished tape, leaving the gate shut until the next paint arms it again.
fn take_colour_tape() -> Option<Vec<String>> {
    COLOUR_TAPE.with_borrow_mut(|tape| {
        tape.take()
            .map(|colours| colours.into_iter().map(Colour::hex_rgba).collect())
    })
}

// --- The font tape (T069, research.md R22) ----------------------------------
//
// The same tap, one layer up: `line` is the single point where every piece of overlay text is
// given a font, so a family recorded there is a family some text was actually laid out in. Both
// halves are kept — the family asked for, and the family pango loaded for it — because FR-046 and
// US4-AS5 are different claims about the same paint: that the override reached every layout, and
// that an absent family is quietly substituted rather than refused.

thread_local! {
    /// `(requested, resolved)` families of the current paint, in first-use order, or `None` when
    /// the gate is shut.
    static FONT_TAPE: std::cell::RefCell<Option<(Vec<String>, Vec<String>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Start (or discard) a tape for the paint about to begin.
fn arm_font_tape(recording: bool) {
    FONT_TAPE.with_borrow_mut(|tape| *tape = recording.then(|| (Vec::new(), Vec::new())));
}

/// Note one laid-out font, if a tape is running. Distinct families only, as with the colours.
///
/// Asking pango which font it loaded is the only way to see a substitution from outside, and it
/// is done here rather than at resolve time because it is the loaded font that draws the pixels.
fn note_font(layout: &pango::Layout, font: &pango::FontDescription) {
    FONT_TAPE.with_borrow_mut(|tape| {
        let Some((requested, resolved)) = tape.as_mut() else {
            return;
        };
        let asked = font
            .family()
            .map_or_else(String::new, |family| family.to_string());
        if !requested.contains(&asked) {
            requested.push(asked);
        }
        // An unresolvable font is not an error here: it is recorded as nothing loaded, and the
        // paint carries on with whatever pango falls back to on its own.
        let loaded = layout
            .context()
            .load_font(font)
            .map(|loaded| loaded.describe())
            .and_then(|described| described.family())
            .map_or_else(String::new, |family| family.to_string());
        if !resolved.contains(&loaded) {
            resolved.push(loaded);
        }
    });
}

/// Take the finished tape, leaving the gate shut until the next paint arms it again.
fn take_font_tape() -> Option<(Vec<String>, Vec<String>)> {
    FONT_TAPE.with_borrow_mut(Option::take)
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
