//! Overlay geometry: how big an entry is, how big the surface is, and which slice of the entry
//! list is on screen (FR-019, SC-005, research.md R16).
//!
//! Pure arithmetic, so SC-005's twenty-workspace case is testable at every monitor size without a
//! compositor. The dimensions used to be `pub const`s here; FR-047 makes them user-settable, so
//! they are now the fields of [`crate::theme::Geometry`] and arrive as an argument. The arithmetic
//! below is unchanged — only where the numbers come from is (FR-049a).
//!
//! What is *not* settable is as deliberate as what is: [`SCROLL_MARGIN`] and the grid label height
//! are derived from the values above them and have no meaning apart from those, so making them
//! settings would only let them disagree (plan.md → Complexity Tracking).
//!
//! The rule that shapes all of it: **entries never shrink to make the set fit**. FR-019 caps the
//! overlay at a fraction of the monitor and scrolls when the entries exceed it, rather than
//! scaling rows down until twenty of them fit.

// Geometry is inherently lossy arithmetic between integer pixels and fractional scale factors;
// every cast below is a deliberate round-to-pixel and is covered by the tests in this module.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::config::Presentation;
use crate::theme::Geometry;

/// The highlight never sits closer than this many entries to a scrolled edge, so the user can
/// always see where the list continues (research.md R16).
///
/// Not a setting: it is a navigation courtesy rather than a dimension, and FR-047 does not list
/// it.
pub const SCROLL_MARGIN: usize = 1;

/// The space between a window's icon and the name it precedes, as a fraction of the icon slot.
///
/// Not a setting either, and for the same reason as [`SCROLL_MARGIN`]: it has no meaning apart
/// from the slot it separates, and FR-047 does not list it. Expressed as a fraction so it follows
/// the themed text height along with the slot itself (FR-052).
const ICON_GAP: f64 = 0.3;

/// The size a window's title is drawn at inside a miniature rectangle, as a fraction of that
/// rectangle's height.
///
/// Not settable, and for the third time for the same reason as [`SCROLL_MARGIN`]: the numbers
/// below describe when content stops being legible, which is a property of eyes rather than of
/// taste, and FR-047 does not list them.
const MINIATURE_FONT_FRACTION: f64 = 0.42;

/// The smallest a miniature's window title may be drawn, device pixels.
///
/// Below this a title is illegible rather than merely small, and drawing it would only smear the
/// rectangle it belongs to (FR-015b, FR-038).
pub const MINIATURE_MIN_TEXT_HEIGHT: f64 = 9.0;

/// The narrowest a title is worth laying out, in multiples of its own size — below this the line
/// is an ellipsis and nothing else, which is not a truncated title but a smudge (FR-038).
const MINIATURE_MIN_TITLE_WIDTH: f64 = 2.0;

/// An icon's size inside a miniature rectangle, as a fraction of that rectangle's shorter side.
const MINIATURE_ICON_FRACTION: f64 = 0.6;

/// The smallest an icon may be drawn in a miniature, device pixels — below this it is a smudge
/// rather than a recognisable program, so it is shed (FR-038).
const MINIATURE_MIN_ICON: f64 = 8.0;

/// Content's inset from a miniature rectangle's edge, as a fraction of the rectangle's width.
const MINIATURE_INSET: f64 = 0.06;

/// Where one window's title goes inside its miniature rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleBox {
    /// The box to lay the title out in — `(x, y, width, height)`, device pixels. The renderer
    /// centres the line it measures inside this box rather than being told a baseline, so a font
    /// with unusual metrics still sits straight.
    pub rect: (f64, f64, f64, f64),
    /// The size to draw it at, device pixels.
    pub font_size: f64,
}

/// What one window rectangle in a miniature has room for, and where that content goes (FR-037,
/// FR-038).
///
/// Both fields are absent on a rectangle too small for either, which is a state the renderer must
/// still draw the rectangle for: FR-038 sheds *content*, never the rectangle, whose position and
/// proportion are what FR-015a is about.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MiniatureContent {
    /// The square the window's icon is drawn in, device pixels — `None` when the icon was shed,
    /// or when icons are off altogether (FR-056).
    pub icon: Option<(f64, f64, f64, f64)>,
    /// Where the window's title goes — `None` when the title was shed.
    pub title: Option<TitleBox>,
}

/// The buffer size and the entry geometry for one overlay, in device pixels — plus the surface
/// size the compositor is asked for, in logical pixels.
///
/// **Two unit systems meet here, and keeping them apart is the whole point of this type.** The
/// buffer is allocated and painted in device pixels, so every drawing measurement below is
/// already multiplied by the monitor's scale. The compositor, however, sizes a surface in
/// *logical* pixels — `zwlr_layer_surface_v1::set_size`, `configure`'s reply and `hyprctl layers`
/// all speak them. Handing device pixels to `set_size` is what makes an overlay on a scaled
/// monitor come out `scale` times larger than every other window on it, so the logical size is
/// carried explicitly rather than left to the caller to remember to divide (FR-019).
///
/// The same type describes both presentations (FR-016). A list is the degenerate grid — one
/// column, a cell that fills the row, no gap and no miniature — which is what lets the session,
/// the viewport arithmetic and the surface plumbing be shared rather than written twice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Which presentation this geometry describes. Only [`refit`] and the renderer branch on it;
    /// everything else is the same arithmetic either way.
    pub presentation: Presentation,
    /// Overlay buffer width, device pixels.
    pub width: u32,
    /// Overlay buffer height, device pixels — exactly the rows on screen plus padding, so a short
    /// list gets a short overlay rather than an empty box.
    pub height: u32,
    /// Overlay surface width, logical pixels.
    pub logical_width: u32,
    /// Overlay surface height, logical pixels.
    pub logical_height: u32,
    /// The monitor's scale factor, i.e. the ratio between the two sizes above.
    pub scale: f32,
    /// The pitch of one row of entries, device pixels — the cell's own height plus [`Self::gap`].
    /// Fixed: it does not vary with the number of entries (FR-019).
    pub row_height: u32,
    /// The text line inside a row, device pixels. The renderer sizes its font from this, so type
    /// scales with the monitor exactly as the rows do.
    pub text_height: u32,
    /// Padding between the entry column and the surface edge, device pixels.
    pub padding: u32,
    /// How many rows of entries are on screen at once. In the list — one column — this is also
    /// the number of entries; the grid multiplies it by [`Self::columns`].
    pub visible_rows: usize,
    /// Entries per row: 1 in the list, as many cells as fit inside the cap in the grid.
    pub columns: usize,
    /// One entry's width, device pixels. The list's cell spans the surface; the grid's is the
    /// documented fixed width.
    pub cell_width: u32,
    /// The miniature area at the top of a grid cell, device pixels; the label line occupies what
    /// is left of the cell below it. Zero in the list, which has no miniature.
    pub miniature_height: u32,
    /// Space between cells, device pixels. Zero in the list, whose rows are adjacent.
    pub gap: u32,
}

impl Metrics {
    /// The square one window's icon is drawn in, device pixels (FR-035, FR-052).
    ///
    /// It is exactly [`Self::text_height`], which is the whole of FR-052: the slot follows the
    /// themed text height, so raising `text_line_height` raises the icons with it and a scaled
    /// monitor gets a proportionally larger icon to rasterise into (FR-039).
    ///
    /// Note what this is *not* a function of: the number of icons on a row, whether icons are
    /// enabled, or anything else about the entries. Nothing above depends on it either — the row
    /// height, the entry count and the visible-entry count are all settled before an icon is
    /// considered, which is how FR-036's "icons change none of those" is guaranteed structurally
    /// rather than by arithmetic that happens to agree.
    #[must_use]
    pub fn icon_slot(&self) -> u32 {
        self.text_height
    }

    /// The horizontal space one icon costs a row: its slot plus the gap separating it from the
    /// name it precedes, device pixels (FR-036a).
    ///
    /// This is what makes a row of many windows truncate its names sooner than the same row
    /// without icons: the icons occupy real width in the line, and pango ellipsises the text
    /// around them.
    #[must_use]
    pub fn icon_advance(&self) -> u32 {
        let slot = f64::from(self.icon_slot());
        // Rounded up, so an icon and its gap never overlap the glyph after them by a fraction of
        // a pixel at an awkward scale.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            (slot * (1.0 + ICON_GAP)).ceil() as u32
        }
    }

    /// What one window rectangle in a miniature has room for, and where that content goes
    /// (FR-037, FR-038).
    ///
    /// `rect` is the rectangle [`miniature_rect`] mapped a window to, and `text_cap` the largest a
    /// title is ever drawn at — the themed text height, so a miniature's titles never outgrow the
    /// list's. `icons` is whether icons are shown at all (FR-056).
    ///
    /// Content is shed in FR-038's order, the title first and then the icon, and that order is
    /// **structural rather than arithmetical**: the title is offered only where the icon already
    /// fits, so no rectangle can keep its title and drop its icon however the thresholds are later
    /// tuned. The three states a shrinking rectangle passes through are therefore exactly icon and
    /// title, icon alone, neither — with the fourth combination reachable only by turning icons
    /// off, where there is no icon to shed in the first place.
    ///
    /// What this does *not* decide is whether the rectangle itself is drawn. It always is
    /// (FR-015a); this only fills it.
    #[must_use]
    pub fn miniature_content(
        &self,
        rect: (f64, f64, f64, f64),
        text_cap: f64,
        icons: bool,
    ) -> MiniatureContent {
        let (x, y, width, height) = rect;
        if width <= 0.0 || height <= 0.0 {
            return MiniatureContent::default();
        }

        // A square on the rectangle's shorter side, never larger than the slot the same icon gets
        // in the list (FR-052) — which is what stops a lone fullscreen window's rectangle being
        // filled edge to edge by one enormous icon while every other miniature shows a small one.
        let side = (width.min(height) * MINIATURE_ICON_FRACTION).min(f64::from(self.icon_slot()));
        let icon_fits = icons && side >= MINIATURE_MIN_ICON;

        let font_size = (height * MINIATURE_FONT_FRACTION).min(text_cap);
        let inset = (width * MINIATURE_INSET).min(font_size * 0.5);
        // The icon costs the title the same slot-plus-gap it costs a row in the list (FR-036a).
        let advance = if icon_fits {
            side * (1.0 + ICON_GAP)
        } else {
            0.0
        };
        let text_width = width - inset * 2.0 - advance;
        let title_fits = (icon_fits || !icons)
            && font_size >= MINIATURE_MIN_TEXT_HEIGHT
            && text_width >= font_size * MINIATURE_MIN_TITLE_WIDTH;

        MiniatureContent {
            icon: icon_fits.then(|| {
                let top = y + (height - side) / 2.0;
                // Beside the title where there is one; centred where the icon is alone, since an
                // icon pinned to the left edge of an otherwise empty rectangle reads as a mistake
                // rather than as a deliberately reduced label.
                if title_fits {
                    (x + inset, top, side, side)
                } else {
                    (x + (width - side) / 2.0, top, side, side)
                }
            }),
            title: title_fits.then_some(TitleBox {
                rect: (x + inset + advance, y, text_width, height),
                font_size,
            }),
        }
    }

    /// How many entries are on screen at once.
    #[must_use]
    pub fn visible_entries(&self) -> usize {
        self.visible_rows * self.columns.max(1)
    }

    /// Whether the entries exceed the cap, i.e. whether the viewport scrolls.
    #[must_use]
    pub fn scrolls(&self, entry_count: usize) -> bool {
        entry_count > self.visible_entries()
    }

    /// The size to ask the compositor for, in the logical pixels its protocol speaks.
    ///
    /// This is the *only* size that may reach `set_size` or `wp_viewport::set_destination`;
    /// [`Self::buffer_size`] is what the shm buffer is allocated in.
    #[must_use]
    pub fn surface_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
    }

    /// The size to allocate and paint the buffer in, in device pixels.
    #[must_use]
    pub fn buffer_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The rectangle of the `slot`-th entry on screen — `(x, y, width, height)`, device pixels.
    ///
    /// `slot` is the position in the viewport, not the index in the entry list; the caller adds
    /// [`first_visible_entry`]'s result to go the other way. Rows fill left to right, which in
    /// the one-column list is simply top to bottom.
    #[must_use]
    pub fn cell_rect(&self, slot: usize) -> (u32, u32, u32, u32) {
        let columns = self.columns.max(1);
        let column = (slot % columns) as u32;
        let row = (slot / columns) as u32;
        (
            self.padding + (self.cell_width + self.gap) * column,
            self.padding + self.row_height * row,
            self.cell_width,
            self.row_height - self.gap,
        )
    }

    /// The miniature panel inside the `slot`-th cell — `(x, y, width, height)`, device pixels,
    /// fractional because it is letterboxed to the monitor's aspect ratio.
    ///
    /// Inset by half a gap so a selected cell's highlight stays visible around it.
    #[must_use]
    pub fn miniature_box(&self, slot: usize, monitor_size: (u32, u32)) -> (f64, f64, f64, f64) {
        let (x, y, width, _) = self.cell_rect(slot);
        let inset = self.gap / 2;
        miniature_area(
            (
                x + inset,
                y + inset,
                width.saturating_sub(inset * 2),
                self.miniature_height.saturating_sub(inset * 2),
            ),
            monitor_size,
        )
    }

    /// The label strip beneath the `slot`-th cell's miniature — `(x, y, width, height)`, device
    /// pixels. FR-015 puts the workspace name here, underneath the preview it names.
    #[must_use]
    pub fn label_rect(&self, slot: usize) -> (u32, u32, u32, u32) {
        let (x, y, width, height) = self.cell_rect(slot);
        (
            x,
            y + self.miniature_height,
            width,
            height.saturating_sub(self.miniature_height),
        )
    }
}

/// Size an overlay for the flat-list presentation on one monitor.
///
/// `monitor_size` is in device pixels and `scale` is that monitor's scale factor, both as
/// `j/monitors` reports them. Every drawing constant is multiplied by the scale and the resulting
/// buffer is divided back down to the logical size the surface is asked for, so the overlay
/// occupies the same fraction of the screen — and so the same physical size — on a `HiDPI`
/// monitor as on a standard one.
#[must_use]
pub fn list_metrics(
    geometry: &Geometry,
    monitor_size: (u32, u32),
    scale: f32,
    entry_count: usize,
) -> Metrics {
    let text_height = scaled(geometry.text_line_height, scale);
    let row_height = scaled(geometry.text_line_height + geometry.row_padding * 2, scale);
    let padding = scaled(geometry.overlay_padding, scale);

    // The cap is a fraction of what the user actually sees, so it is taken on the logical desktop
    // — 80 % of a 4K panel at scale 2 is 80 % of the 1920×1080 the compositor lays out on it.
    let logical_monitor = (
        logical(monitor_size.0, scale),
        logical(monitor_size.1, scale),
    );
    let logical_width = fraction(logical_monitor.0, geometry.width_fraction);
    let max_height = scaled(fraction(logical_monitor.1, geometry.height_fraction), scale);

    let visible_rows = entry_count.clamp(1, rows_that_fit(max_height, row_height, padding));
    // Built from the row geometry rather than scaled from a logical height, so the rows always
    // tile the buffer exactly however the scale rounds.
    let height = row_height * visible_rows as u32 + padding * 2;
    let width = scaled(logical_width, scale);

    Metrics {
        presentation: Presentation::List,
        width,
        height,
        logical_width,
        logical_height: logical(height, scale),
        scale,
        row_height,
        text_height,
        padding,
        visible_rows,
        // A row is a cell that fills the surface: one column, no gap, no miniature.
        columns: 1,
        cell_width: width.saturating_sub(padding * 2),
        miniature_height: 0,
        gap: 0,
    }
}

/// Size an overlay for the grid presentation on one monitor (FR-015, FR-019).
///
/// The cell is the documented fixed 240 × 135 plus its label line, so the arithmetic is the list's
/// with one extra question answered first: how many of those cells fit across the cap. Entries
/// never shrink here either — a set too large for the cap scrolls, exactly as the list's does.
#[must_use]
pub fn grid_metrics(
    geometry: &Geometry,
    monitor_size: (u32, u32),
    scale: f32,
    entry_count: usize,
) -> Metrics {
    let text_height = scaled(geometry.text_line_height, scale);
    let cell_width = scaled(geometry.grid_cell_width, scale);
    let miniature_height = scaled(geometry.grid_cell_height, scale);
    let gap = scaled(geometry.grid_gap, scale);
    let padding = scaled(geometry.overlay_padding, scale);
    // The pitch carries the gap, so `cell_rect` subtracts it to get the cell's own height.
    let row_height = miniature_height + scaled(geometry.grid_label_height(), scale) + gap;

    let logical_monitor = (
        logical(monitor_size.0, scale),
        logical(monitor_size.1, scale),
    );
    let max_width = scaled(fraction(logical_monitor.0, geometry.width_fraction), scale);
    let max_height = scaled(fraction(logical_monitor.1, geometry.height_fraction), scale);

    // Never wider than there are entries to put in it: three workspaces get a three-cell overlay,
    // not a full-width one with empty space.
    let columns = columns_that_fit(max_width, cell_width, gap, padding).min(entry_count.max(1));
    let rows_needed = entry_count.max(1).div_ceil(columns);
    // The last row spends no gap, so the height it needs is one gap less than its pitch implies.
    let visible_rows = rows_needed.min(rows_that_fit(max_height + gap, row_height, padding));

    // Both sizes are built from the cell geometry rather than scaled from a logical size, so the
    // cells tile the buffer exactly however the scale rounds — as the list's rows do.
    let width = padding * 2 + cell_width * columns as u32 + gap * (columns as u32 - 1);
    let height = padding * 2 + row_height * visible_rows as u32 - gap;

    Metrics {
        presentation: Presentation::Grid,
        width,
        height,
        logical_width: logical(width, scale),
        logical_height: logical(height, scale),
        scale,
        row_height,
        text_height,
        padding,
        visible_rows,
        columns,
        cell_width,
        miniature_height,
        gap,
    }
}

/// How many whole cells fit across `width` once the padding is spent. Never zero.
#[must_use]
pub fn columns_that_fit(width: u32, cell_width: u32, gap: u32, padding: u32) -> usize {
    if cell_width == 0 {
        return 1;
    }
    // One notional gap is added back because the last column does not spend one.
    let available = width.saturating_sub(padding * 2) + gap;
    ((available / (cell_width + gap)).max(1)) as usize
}

/// How many whole rows fit in `height` once the padding is spent.
///
/// Never zero: an overlay showing nothing at all would be worse than one that overflows a very
/// short monitor. Shared with the surface-configure path, where the compositor may hand back a
/// height other than the one requested and the row count has to follow it.
#[must_use]
pub fn rows_that_fit(height: u32, row_height: u32, padding: u32) -> usize {
    if row_height == 0 {
        return 1;
    }
    (height.saturating_sub(padding * 2) / row_height).max(1) as usize
}

/// Re-fit a set of metrics to a surface size the compositor chose.
///
/// `logical_width` and `logical_height` come straight from a `configure`, so they are in the
/// compositor's logical pixels; the buffer they imply is multiplied back up by the monitor scale.
///
/// Keeps the row height — entries never shrink (FR-019) — and changes only how many of them are
/// on screen, so the painted rows always fit the surface actually agreed to.
#[must_use]
pub fn refit(
    metrics: Metrics,
    logical_width: u32,
    logical_height: u32,
    entry_count: usize,
) -> Metrics {
    let width = scaled(logical_width, metrics.scale);
    let height = scaled(logical_height, metrics.scale);
    // The list's cell spans the surface, so a narrower surface narrows it; the grid's cell is a
    // documented fixed size, so a narrower surface fits fewer of them instead.
    let (columns, cell_width) = match metrics.presentation {
        Presentation::List => (1, width.saturating_sub(metrics.padding * 2)),
        Presentation::Grid => (
            columns_that_fit(width, metrics.cell_width, metrics.gap, metrics.padding),
            metrics.cell_width,
        ),
    };
    let rows_needed = entry_count.max(1).div_ceil(columns);
    let visible_rows = rows_needed.min(rows_that_fit(
        height + metrics.gap,
        metrics.row_height,
        metrics.padding,
    ));
    Metrics {
        width,
        height,
        logical_width,
        logical_height,
        visible_rows,
        columns,
        cell_width,
        ..metrics
    }
}

/// The index of the first entry on screen, for either presentation.
///
/// Scrolling moves whole rows: in the grid a cell keeps its column as the viewport moves, which is
/// what makes the arrangement stable enough to navigate. Reduces to [`first_visible`] on rows and
/// is exactly it in the one-column list.
#[must_use]
pub fn first_visible_entry(
    metrics: &Metrics,
    entry_count: usize,
    highlight: usize,
    previous: usize,
) -> usize {
    let columns = metrics.columns.max(1);
    let first_row = first_visible(
        metrics.visible_rows,
        entry_count.div_ceil(columns),
        highlight / columns,
        previous / columns,
    );
    first_row * columns
}

/// Letterbox a monitor-shaped box inside `cell`, centred — `(x, y, width, height)`.
///
/// A miniature that filled a 16:9 cell with a 4:3 monitor's layout would stretch every window in
/// it, and FR-015a asks for the proportion each window *occupies*, not merely its position. The
/// cell stays the documented fixed size; the drawing area inside it takes the monitor's shape.
#[must_use]
pub fn miniature_area(
    cell: (u32, u32, u32, u32),
    monitor_size: (u32, u32),
) -> (f64, f64, f64, f64) {
    let (x, y, width, height) = (
        f64::from(cell.0),
        f64::from(cell.1),
        f64::from(cell.2),
        f64::from(cell.3),
    );
    if monitor_size.0 == 0 || monitor_size.1 == 0 || width <= 0.0 || height <= 0.0 {
        return (x, y, width, height);
    }
    let aspect = f64::from(monitor_size.0) / f64::from(monitor_size.1);
    let (fitted_width, fitted_height) = if width / height > aspect {
        (height * aspect, height)
    } else {
        (width, width / aspect)
    };
    (
        x + (width - fitted_width) / 2.0,
        y + (height - fitted_height) / 2.0,
        fitted_width,
        fitted_height,
    )
}

/// Map one window's layout rectangle into a miniature (FR-015a, SC-008).
///
/// The normalisation is `(window.at − monitor.position) / monitor.size`, against the monitor the
/// *workspace* is bound to — never the one the overlay happens to be shown on. That is the whole
/// of "equally accurate for workspaces that are not currently visible": the compositor reports the
/// same layout coordinates either way, so a workspace that has never been composited maps exactly
/// as a visible one does (research.md R7).
///
/// Returns `None` for a window with no area, for a monitor with no size, and for a window that
/// falls entirely outside its monitor — none of which can be drawn as a proportion of anything.
#[must_use]
pub fn miniature_rect(
    window_at: (i32, i32),
    window_size: (u32, u32),
    monitor_position: (i32, i32),
    monitor_size: (u32, u32),
    area: (f64, f64, f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    if window_size.0 == 0 || window_size.1 == 0 || monitor_size.0 == 0 || monitor_size.1 == 0 {
        return None;
    }
    let (monitor_width, monitor_height) = (f64::from(monitor_size.0), f64::from(monitor_size.1));
    let relative_x = f64::from(window_at.0 - monitor_position.0) / monitor_width;
    let relative_y = f64::from(window_at.1 - monitor_position.1) / monitor_height;
    let relative_width = f64::from(window_size.0) / monitor_width;
    let relative_height = f64::from(window_size.1) / monitor_height;

    let (area_x, area_y, area_width, area_height) = area;
    let left = area_x + relative_x * area_width;
    let top = area_y + relative_y * area_height;

    // Clamped to the miniature: a window straddling the edge of its monitor — a shell overhanging
    // its output, say — must not paint over the neighbouring cell.
    let clamped_left = left.clamp(area_x, area_x + area_width);
    let clamped_top = top.clamp(area_y, area_y + area_height);
    let width = (left + relative_width * area_width).min(area_x + area_width) - clamped_left;
    let height = (top + relative_height * area_height).min(area_y + area_height) - clamped_top;

    (width > 0.0 && height > 0.0).then_some((clamped_left, clamped_top, width, height))
}

/// The index of the first entry on screen, given where the viewport was before.
///
/// Scrolling is stateful — it depends on which way the user came from — so the previous position
/// is an input rather than something recomputed. The result keeps the highlight at least
/// [`SCROLL_MARGIN`] entries from each edge of the viewport, except at the true ends of the list
/// where there is nothing to reveal.
#[must_use]
pub fn first_visible(
    visible_rows: usize,
    entry_count: usize,
    highlight: usize,
    previous: usize,
) -> usize {
    if entry_count <= visible_rows {
        return 0;
    }
    let max_first = entry_count - visible_rows;
    let mut first = previous.min(max_first);

    // Scrolled up past the margin: pull the viewport up to restore it.
    if highlight < first + SCROLL_MARGIN {
        first = highlight.saturating_sub(SCROLL_MARGIN);
    }
    // Scrolled down past the margin: push it down.
    if highlight + SCROLL_MARGIN >= first + visible_rows {
        first = (highlight + SCROLL_MARGIN + 1).saturating_sub(visible_rows);
    }

    // A viewport shorter than twice the margin cannot honour it on both sides, and the margin
    // adjustments above can then push the highlight itself off screen. Visibility is the
    // requirement (SC-005); the margin is the courtesy, so it yields.
    first = first
        .min(highlight)
        .max((highlight + 1).saturating_sub(visible_rows));

    first.min(max_first)
}

/// The scale to compute with: anything the compositor cannot have meant — zero, negative, `NaN` —
/// becomes 1 rather than collapsing or exploding the overlay.
fn sane_scale(scale: f32) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        f64::from(scale)
    } else {
        1.0
    }
}

/// Round a logical-pixel measurement up to device pixels, never to zero.
fn scaled(logical: u32, scale: f32) -> u32 {
    ((f64::from(logical) * sane_scale(scale)).round() as u32).max(1)
}

/// Round a device-pixel measurement back down to logical pixels, never to zero.
fn logical(device: u32, scale: f32) -> u32 {
    ((f64::from(device) / sane_scale(scale)).round() as u32).max(1)
}

fn fraction(pixels: u32, of: f64) -> u32 {
    ((f64::from(pixels) * of).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults, which are exactly the `pub const`s this module used to hold. Every
    /// expectation below is anchored to them and asserts the same numbers as before the refactor,
    /// which is the proof that turning the constants into settings changed no arithmetic
    /// (FR-047, FR-049a).
    const G: Geometry = Geometry::DEFAULT;
    const GRID_LABEL_HEIGHT: u32 = G.text_line_height + G.row_padding;

    /// A 1080p monitor at scale 1 — the reference case every expectation below is anchored to.
    const HD: (u32, u32) = (1920, 1080);

    // --- Metrics -----------------------------------------------------------

    #[test]
    fn the_overlay_is_capped_at_eighty_percent_of_the_monitor() {
        // FR-019's documented fraction.
        let metrics = list_metrics(&G, HD, 1.0, 100);
        assert_eq!(metrics.width, 1536, "80 % of 1920");
        assert!(
            metrics.height <= 864,
            "80 % of 1080, got {}",
            metrics.height
        );
    }

    #[test]
    fn a_short_list_gets_a_short_overlay_rather_than_an_empty_box() {
        let metrics = list_metrics(&G, HD, 1.0, 3);
        assert_eq!(metrics.visible_rows, 3);
        assert_eq!(metrics.height, metrics.row_height * 3 + metrics.padding * 2);
        assert!(!metrics.scrolls(3));
    }

    #[test]
    fn the_row_height_is_one_text_line_plus_its_padding() {
        let metrics = list_metrics(&G, HD, 1.0, 5);
        assert_eq!(metrics.row_height, G.text_line_height + G.row_padding * 2);
    }

    #[test]
    fn entries_keep_their_size_no_matter_how_many_there_are() {
        // FR-019: the overlay scrolls instead of scaling entries down. This is the requirement.
        let reference = list_metrics(&G, HD, 1.0, 1).row_height;
        for count in [2, 5, 20, 100, 1000] {
            assert_eq!(
                list_metrics(&G, HD, 1.0, count).row_height,
                reference,
                "{count} entries changed the row height"
            );
        }
    }

    #[test]
    fn twenty_workspaces_keep_full_entry_size_whether_or_not_they_fit() {
        // SC-005. On 1080p all twenty fit inside the cap; on a 720p monitor they do not, and the
        // difference is that the overlay scrolls — never that the rows get smaller (FR-019).
        let roomy = list_metrics(&G, HD, 1.0, 20);
        assert_eq!(roomy.row_height, 36);
        assert_eq!(roomy.visible_rows, 20, "20 × 36 + 24 fits inside 864");
        assert!(!roomy.scrolls(20));
        assert!(roomy.height <= 864);

        let cramped = list_metrics(&G, (1280, 720), 1.0, 20);
        assert_eq!(cramped.row_height, roomy.row_height, "rows never shrink");
        assert!(cramped.scrolls(20), "20 rows exceed 80 % of 720");
        assert_eq!(cramped.visible_rows, 15, "(576 − 24) / 36");
        assert!(cramped.height <= 576);
    }

    #[test]
    fn entry_size_and_the_cap_hold_at_every_monitor_size() {
        // SC-005 is "at every monitor size", so this is the loop that proves it.
        let sizes = [
            (1280, 720),
            (1920, 1080),
            (2560, 1440),
            (3440, 1440),
            (3840, 2160),
            (800, 600),
        ];
        for size in sizes {
            for count in [1, 3, 20, 50] {
                let metrics = list_metrics(&G, size, 1.0, count);
                assert_eq!(
                    metrics.row_height,
                    G.text_line_height + G.row_padding * 2,
                    "{size:?} with {count} entries shrank the rows"
                );
                assert!(
                    metrics.width <= size.0 && metrics.height <= size.1,
                    "{size:?} with {count} entries overflowed the monitor: {metrics:?}"
                );
                assert!(metrics.visible_rows >= 1);
            }
        }
    }

    #[test]
    fn the_text_line_leaves_room_for_its_padding_inside_the_row() {
        let metrics = list_metrics(&G, HD, 1.0, 5);
        assert_eq!(metrics.text_height, G.text_line_height);
        assert_eq!(metrics.row_height - metrics.text_height, G.row_padding * 2);
    }

    #[test]
    fn every_constant_is_multiplied_by_the_monitor_scale() {
        let one = list_metrics(&G, HD, 1.0, 5);
        let two = list_metrics(&G, (3840, 2160), 2.0, 5);
        assert_eq!(two.row_height, one.row_height * 2);
        assert_eq!(two.text_height, one.text_height * 2);
        assert_eq!(two.padding, one.padding * 2);
        assert_eq!(
            two.visible_rows, one.visible_rows,
            "the same physical size shows the same number of entries"
        );
    }

    #[test]
    fn a_fractional_scale_still_yields_whole_pixels() {
        let metrics = list_metrics(&G, (2560, 1440), 1.5, 10);
        assert_eq!(metrics.row_height, 54, "36 logical px at 1.5");
        assert_eq!(metrics.padding, 18);
    }

    #[test]
    fn a_nonsense_scale_falls_back_to_one_rather_than_collapsing_the_overlay() {
        for scale in [0.0, -1.0, f32::NAN] {
            let metrics = list_metrics(&G, HD, scale, 5);
            assert_eq!(metrics.row_height, 36, "scale {scale}");
            assert_eq!(
                metrics.surface_size(),
                metrics.buffer_size(),
                "scale {scale}"
            );
        }
    }

    // --- Logical versus device pixels --------------------------------------

    #[test]
    fn the_surface_size_is_logical_pixels_and_the_buffer_size_is_device_pixels() {
        // The bug this separation exists to prevent: `set_size` takes logical pixels, so handing
        // it the buffer size makes the overlay `scale` times too large on a scaled monitor.
        let metrics = list_metrics(&G, (3840, 2160), 2.0, 5);
        assert_eq!(metrics.buffer_size(), (3072, metrics.height));
        assert_eq!(metrics.surface_size(), (1536, metrics.height / 2));
    }

    #[test]
    fn a_scaled_monitor_asks_for_the_same_surface_as_the_unscaled_one_behind_it() {
        // A 4K panel at scale 2 presents a 1920×1080 logical desktop, so the overlay must occupy
        // exactly what it would on a real 1920×1080 monitor — the same size as every other window
        // on that screen, not twice it.
        let unscaled = list_metrics(&G, (1920, 1080), 1.0, 7);
        let scaled_up = list_metrics(&G, (3840, 2160), 2.0, 7);
        assert_eq!(scaled_up.surface_size(), unscaled.surface_size());
        assert_eq!(scaled_up.visible_rows, unscaled.visible_rows);
        assert_eq!(
            scaled_up.buffer_size(),
            (unscaled.width * 2, unscaled.height * 2),
            "the buffer is still painted at full device resolution"
        );
    }

    #[test]
    fn the_overlay_stays_inside_the_cap_of_the_logical_monitor_at_every_scale() {
        // FR-019's 80 % cap is a fraction of what the user sees, which is the logical desktop.
        for (size, scale) in [
            ((1920, 1080), 1.0),
            ((2560, 1440), 1.25),
            ((3840, 2160), 1.5),
            ((3840, 2160), 2.0),
            ((3000, 2000), 1.75),
            ((5120, 2880), 2.5),
        ] {
            let logical_monitor = (logical(size.0, scale), logical(size.1, scale));
            for count in [1, 3, 20, 50] {
                let metrics = list_metrics(&G, size, scale, count);
                let (width, height) = metrics.surface_size();
                assert!(
                    width <= logical_monitor.0 * 4 / 5 + 1
                        && height <= logical_monitor.1 * 4 / 5 + 1,
                    "{size:?} at scale {scale} with {count} entries: {:?} exceeds 80 % of {logical_monitor:?}",
                    metrics.surface_size()
                );
                assert!(metrics.visible_rows >= 1);
            }
        }
    }

    #[test]
    fn a_fractional_scale_round_trips_between_the_two_unit_systems() {
        // A compositor that grants the size asked for must not cause a resize on the way back.
        // The *surface* size is what the protocol contracts on, so it round-trips exactly; the
        // buffer is free to land a device pixel either side of where it started, which at worst
        // trims a pixel off the bottom padding.
        for scale in [1.0, 1.25, 1.5, 1.6, 1.75, 2.0, 2.5, 3.0] {
            let metrics = list_metrics(&G, (3840, 2160), scale, 10);
            let (width, height) = metrics.surface_size();
            let refitted = refit(metrics, width, height, 10);
            assert_eq!(refitted.surface_size(), (width, height), "scale {scale}");
            assert_eq!(refitted.visible_rows, metrics.visible_rows, "scale {scale}");
            assert_eq!(refitted.width, metrics.width, "scale {scale}");
            assert!(
                refitted.height.abs_diff(metrics.height) <= 1,
                "scale {scale}: {} against {}",
                refitted.height,
                metrics.height
            );
        }
    }

    #[test]
    fn the_rows_tile_the_buffer_exactly_at_every_scale() {
        // The buffer is what cairo paints into, so a row running past its bottom edge would be
        // silently clipped. Deriving the height from the row geometry is what prevents that.
        for scale in [1.0, 1.125, 1.25, 1.5, 1.6, 1.75, 2.0, 2.5, 3.0] {
            for count in [1, 5, 20] {
                let metrics = list_metrics(&G, (3840, 2160), scale, count);
                let (_, last_y, _, row) = metrics.cell_rect(metrics.visible_rows - 1);
                assert_eq!(
                    last_y + row + metrics.padding,
                    metrics.height,
                    "scale {scale} with {count} entries"
                );
            }
        }
    }

    #[test]
    fn refitting_reads_the_compositors_logical_size_and_scales_the_buffer_back_up() {
        let metrics = list_metrics(&G, (3840, 2160), 2.0, 20);
        let refitted = refit(metrics, 800, 300, 20);
        assert_eq!(refitted.surface_size(), (800, 300));
        assert_eq!(
            refitted.buffer_size(),
            (1600, 600),
            "device pixels to paint"
        );
        assert_eq!(
            refitted.row_height, metrics.row_height,
            "rows never shrink, at any scale"
        );
        assert_eq!(refitted.visible_rows, rows_that_fit(600, 72, 24));
    }

    #[test]
    fn a_monitor_too_short_for_even_one_row_still_shows_one() {
        let metrics = list_metrics(&G, (640, 40), 1.0, 10);
        assert_eq!(metrics.visible_rows, 1);
    }

    #[test]
    fn list_cell_rects_stack_without_gaps_or_overlap() {
        let metrics = list_metrics(&G, HD, 1.0, 20);
        for slot in 1..metrics.visible_rows {
            let (_, previous_y, _, height) = metrics.cell_rect(slot - 1);
            let (_, y, _, _) = metrics.cell_rect(slot);
            assert_eq!(y, previous_y + height, "slot {slot}");
        }
        let (_, last_y, _, height) = metrics.cell_rect(metrics.visible_rows - 1);
        assert!(last_y + height + metrics.padding <= metrics.height);
    }

    // --- Refitting to a compositor-chosen size -----------------------------

    #[test]
    fn refitting_to_a_shorter_surface_shows_fewer_rows_at_the_same_size() {
        let metrics = list_metrics(&G, HD, 1.0, 20);
        assert_eq!(metrics.visible_rows, 20);

        let refitted = refit(metrics, metrics.width, 300, 20);
        assert_eq!(refitted.row_height, metrics.row_height, "rows never shrink");
        assert_eq!(refitted.visible_rows, rows_that_fit(300, 36, 12));
        assert!(
            refitted.visible_rows * refitted.row_height as usize <= 300,
            "the painted rows fit the surface actually agreed to"
        );
    }

    #[test]
    fn refitting_never_shows_more_rows_than_there_are_entries() {
        let metrics = list_metrics(&G, HD, 1.0, 3);
        assert_eq!(refit(metrics, 1536, 864, 3).visible_rows, 3);
    }

    #[test]
    fn a_surface_too_short_for_a_row_still_shows_one() {
        let metrics = list_metrics(&G, HD, 1.0, 20);
        assert_eq!(refit(metrics, 1536, 10, 20).visible_rows, 1);
    }

    // --- Grid metrics ------------------------------------------------------

    /// Geometry is fractional; comparing it exactly would test the rounding, not the arithmetic.
    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 0.001
    }

    fn assert_rect(actual: (f64, f64, f64, f64), expected: (f64, f64, f64, f64), what: &str) {
        assert!(
            close(actual.0, expected.0)
                && close(actual.1, expected.1)
                && close(actual.2, expected.2)
                && close(actual.3, expected.3),
            "{what}: got {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    fn a_grid_cell_is_the_documented_size_plus_its_label_line() {
        // `contracts/config.md`: 240 × 135 logical px (16:9) + label line.
        let metrics = grid_metrics(&G, HD, 1.0, 6);
        assert_eq!(metrics.cell_width, G.grid_cell_width);
        assert_eq!(metrics.miniature_height, G.grid_cell_height);
        let (_, _, _, cell_height) = metrics.cell_rect(0);
        assert_eq!(cell_height, G.grid_cell_height + GRID_LABEL_HEIGHT);
        let (_, _, _, label_height) = metrics.label_rect(0);
        assert_eq!(label_height, GRID_LABEL_HEIGHT, "the label sits below it");
    }

    #[test]
    fn grid_cells_keep_their_size_no_matter_how_many_there_are() {
        // FR-019 in the grid: the overlay scrolls rather than shrinking miniatures.
        let reference = grid_metrics(&G, HD, 1.0, 1);
        for count in [2, 6, 20, 100, 1000] {
            let metrics = grid_metrics(&G, HD, 1.0, count);
            assert_eq!(metrics.cell_width, reference.cell_width, "{count} entries");
            assert_eq!(metrics.row_height, reference.row_height, "{count} entries");
            assert_eq!(
                metrics.miniature_height, reference.miniature_height,
                "{count} entries"
            );
        }
    }

    #[test]
    fn the_grid_fits_as_many_cells_as_the_cap_allows_and_no_more() {
        let metrics = grid_metrics(&G, HD, 1.0, 40);
        // 80 % of 1920 is 1536; six 240-wide cells with 12 px gaps and padding come to 1524.
        assert_eq!(metrics.columns, 6);
        assert_eq!(metrics.width, 1524);
        assert!(metrics.width <= 1536, "inside 80 % of 1920");
        assert!(metrics.height <= 864, "inside 80 % of 1080");
        assert!(metrics.scrolls(40), "40 entries exceed the cap");
    }

    #[test]
    fn the_grid_is_never_wider_than_it_has_entries_to_fill() {
        for count in 1..=6 {
            let metrics = grid_metrics(&G, HD, 1.0, count);
            assert_eq!(metrics.columns, count, "{count} entries");
            assert_eq!(metrics.visible_rows, 1, "{count} entries fit on one row");
        }
    }

    #[test]
    fn grid_cells_tile_the_surface_without_overlapping() {
        let metrics = grid_metrics(&G, HD, 1.0, 20);
        let mut seen: Vec<(u32, u32, u32, u32)> = Vec::new();
        for slot in 0..metrics.visible_entries() {
            let cell = metrics.cell_rect(slot);
            assert!(
                cell.0 + cell.2 + metrics.padding <= metrics.width,
                "slot {slot} runs past the right edge: {cell:?} in {:?}",
                metrics.buffer_size()
            );
            assert!(
                cell.1 + cell.3 + metrics.padding <= metrics.height,
                "slot {slot} runs past the bottom edge: {cell:?}"
            );
            for other in &seen {
                let separated = cell.0 >= other.0 + other.2
                    || other.0 >= cell.0 + cell.2
                    || cell.1 >= other.1 + other.3
                    || other.1 >= cell.1 + cell.3;
                assert!(separated, "slot {slot} at {cell:?} overlaps {other:?}");
            }
            seen.push(cell);
        }
    }

    #[test]
    fn the_grid_stays_inside_the_cap_at_every_monitor_size_and_scale() {
        for (size, scale) in [
            ((1280, 720), 1.0),
            ((1920, 1080), 1.0),
            ((2560, 1440), 1.25),
            ((3840, 2160), 2.0),
            ((800, 600), 1.0),
            ((640, 480), 1.0),
        ] {
            let logical_monitor = (logical(size.0, scale), logical(size.1, scale));
            for count in [1, 3, 20, 50] {
                let metrics = grid_metrics(&G, size, scale, count);
                let (width, height) = metrics.surface_size();
                assert!(
                    width <= logical_monitor.0 * 4 / 5 + 1
                        && height <= logical_monitor.1 * 4 / 5 + 1,
                    "{size:?} at scale {scale} with {count} entries: {:?} exceeds 80 % of {logical_monitor:?}",
                    metrics.surface_size()
                );
                assert!(metrics.columns >= 1 && metrics.visible_rows >= 1);
                assert_eq!(
                    metrics.cell_width,
                    grid_metrics(&G, size, scale, 1).cell_width,
                    "cells never shrink to make {count} of them fit"
                );
            }
        }
    }

    #[test]
    fn every_grid_constant_is_multiplied_by_the_monitor_scale() {
        let one = grid_metrics(&G, HD, 1.0, 6);
        let two = grid_metrics(&G, (3840, 2160), 2.0, 6);
        assert_eq!(two.cell_width, one.cell_width * 2);
        assert_eq!(two.miniature_height, one.miniature_height * 2);
        assert_eq!(two.gap, one.gap * 2);
        assert_eq!(two.row_height, one.row_height * 2);
        assert_eq!(
            two.columns, one.columns,
            "the same physical size shows the same number of cells"
        );
        assert_eq!(two.surface_size(), one.surface_size());
    }

    #[test]
    fn a_grid_surface_round_trips_between_the_two_unit_systems() {
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let metrics = grid_metrics(&G, (3840, 2160), scale, 20);
            let (width, height) = metrics.surface_size();
            let refitted = refit(metrics, width, height, 20);
            assert_eq!(refitted.columns, metrics.columns, "scale {scale}");
            assert_eq!(refitted.visible_rows, metrics.visible_rows, "scale {scale}");
            assert_eq!(refitted.cell_width, metrics.cell_width, "scale {scale}");
        }
    }

    #[test]
    fn refitting_a_grid_to_a_narrower_surface_shows_fewer_columns_at_the_same_size() {
        let metrics = grid_metrics(&G, HD, 1.0, 20);
        assert_eq!(metrics.columns, 6);

        let refitted = refit(metrics, 800, metrics.logical_height, 20);
        assert_eq!(
            refitted.cell_width, metrics.cell_width,
            "cells never shrink"
        );
        assert_eq!(refitted.columns, columns_that_fit(800, 240, 12, 12));
        assert!(
            refitted.columns < metrics.columns,
            "a narrower surface holds fewer cells, not smaller ones"
        );
    }

    // --- Miniatures (FR-015a, SC-008) --------------------------------------

    /// A 1920×1080 monitor at the origin, and the 240×135 area a miniature maps into.
    const MONITOR: (u32, u32) = (1920, 1080);
    const AREA: (f64, f64, f64, f64) = (0.0, 0.0, 240.0, 135.0);

    fn mapped(
        at: (i32, i32),
        size: (u32, u32),
        monitor_position: (i32, i32),
    ) -> Option<(f64, f64, f64, f64)> {
        miniature_rect(at, size, monitor_position, MONITOR, AREA)
    }

    #[test]
    fn a_window_keeps_its_relative_position_and_proportion() {
        // FR-015a: the right-hand half of the monitor is the right-hand half of the miniature.
        assert_rect(
            mapped((960, 0), (960, 1080), (0, 0)).expect("a window with area maps"),
            (120.0, 0.0, 120.0, 135.0),
            "the right half",
        );
        assert_rect(
            mapped((0, 0), (1920, 1080), (0, 0)).expect("a full-screen window maps"),
            (0.0, 0.0, 240.0, 135.0),
            "a window filling the monitor fills the miniature",
        );
        assert_rect(
            mapped((480, 270), (960, 540), (0, 0)).expect("a centred window maps"),
            (60.0, 33.75, 120.0, 67.5),
            "a quarter-size window centred stays a quarter-size window centred",
        );
    }

    #[test]
    fn a_workspace_bound_to_a_monitor_it_is_not_shown_on_maps_identically() {
        // SC-008 and US3-AS3: the normalisation subtracts the *workspace's* monitor origin, so a
        // workspace laid out on a second monitor at x=1920 produces exactly the miniature the same
        // layout produces on the monitor at the origin — whether or not either is being displayed.
        let on_the_origin = mapped((960, 0), (960, 1080), (0, 0)).expect("maps");
        let on_the_second = mapped((1920 + 960, 0), (960, 1080), (1920, 0)).expect("maps");
        assert_rect(
            on_the_second,
            on_the_origin,
            "the same layout, another monitor",
        );

        let below =
            miniature_rect((0, 1080), (1920, 1080), (0, 1080), MONITOR, AREA).expect("maps");
        assert_rect(
            below,
            (0.0, 0.0, 240.0, 135.0),
            "a monitor stacked vertically",
        );
    }

    #[test]
    fn the_three_window_arrangement_keeps_its_shape() {
        // US3-AS2: two windows side by side and a third below the second.
        let left = mapped((0, 0), (960, 1080), (0, 0)).expect("maps");
        let top_right = mapped((960, 0), (960, 540), (0, 0)).expect("maps");
        let bottom_right = mapped((960, 540), (960, 540), (0, 0)).expect("maps");

        assert!(left.0 < top_right.0, "the first window is to the left");
        assert!(
            close(top_right.0, bottom_right.0) && close(top_right.2, bottom_right.2),
            "the stacked pair share a column: {top_right:?} against {bottom_right:?}"
        );
        assert!(
            close(bottom_right.1, top_right.1 + top_right.3),
            "the third sits directly below the second"
        );
        assert!(
            close(left.3, top_right.3 + bottom_right.3),
            "and the two of them are as tall as the one beside them"
        );
    }

    #[test]
    fn zero_size_windows_are_skipped() {
        // SC-008 counts one rectangle per window, and a window with no area is not one.
        assert_eq!(mapped((0, 0), (0, 1080), (0, 0)), None);
        assert_eq!(mapped((0, 0), (960, 0), (0, 0)), None);
        assert_eq!(
            miniature_rect((0, 0), (960, 540), (0, 0), (0, 0), AREA),
            None,
            "a monitor with no size cannot normalise anything"
        );
    }

    #[test]
    fn a_window_outside_its_monitor_is_clamped_or_skipped() {
        let overhanging = mapped((1440, 0), (960, 1080), (0, 0)).expect("partly on screen");
        assert!(
            overhanging.0 + overhanging.2 <= AREA.2 + 0.001,
            "a window overhanging the monitor stops at the miniature's edge: {overhanging:?}"
        );
        assert_eq!(
            mapped((3840, 0), (960, 1080), (0, 0)),
            None,
            "a window entirely off its monitor has nothing to draw"
        );
    }

    #[test]
    fn the_miniature_area_takes_the_monitors_aspect_ratio() {
        // A 4:3 monitor drawn into a 16:9 cell must be letterboxed, or every window in it would be
        // stretched and FR-015a's "proportion" would be false.
        let cell = (0, 0, 240, 135);
        assert_rect(
            miniature_area(cell, (1920, 1080)),
            (0.0, 0.0, 240.0, 135.0),
            "a 16:9 monitor fills a 16:9 cell",
        );
        let four_by_three = miniature_area(cell, (1600, 1200));
        assert!(
            close(four_by_three.3, 135.0) && close(four_by_three.2, 180.0),
            "a 4:3 monitor is pillarboxed: {four_by_three:?}"
        );
        assert!(
            close(four_by_three.0, 30.0) && close(four_by_three.1, 0.0),
            "and centred in the cell: {four_by_three:?}"
        );
        assert!(
            close(four_by_three.2 / four_by_three.3, 1600.0 / 1200.0),
            "the area has the monitor's aspect ratio"
        );
    }

    #[test]
    fn the_miniature_box_sits_inside_its_cell() {
        let metrics = grid_metrics(&G, HD, 1.0, 12);
        for slot in 0..metrics.visible_entries() {
            let (x, y, width, height) = metrics.cell_rect(slot);
            let area = metrics.miniature_box(slot, (1920, 1080));
            assert!(
                area.0 >= f64::from(x)
                    && area.1 >= f64::from(y)
                    && area.0 + area.2 <= f64::from(x + width) + 0.001
                    && area.1 + area.3 <= f64::from(y + height) + 0.001,
                "slot {slot}: {area:?} escapes its cell {:?}",
                (x, y, width, height)
            );
            let label = metrics.label_rect(slot);
            assert!(
                area.1 + area.3 <= f64::from(label.1) + 0.001,
                "slot {slot}: the miniature overlaps the label beneath it"
            );
        }
    }

    // --- Viewport ----------------------------------------------------------

    #[test]
    fn a_list_that_fits_never_scrolls() {
        for highlight in 0..5 {
            assert_eq!(first_visible(10, 5, highlight, 0), 0);
        }
    }

    #[test]
    fn scrolling_down_keeps_one_entry_below_the_highlight() {
        let rows = 5;
        let mut first = 0;
        for highlight in 0..20 {
            first = first_visible(rows, 20, highlight, first);
            let last_visible = first + rows - 1;
            assert!(
                (first..=last_visible).contains(&highlight),
                "highlight {highlight} outside {first}..={last_visible}"
            );
            if highlight + SCROLL_MARGIN < 20 {
                assert!(
                    highlight + SCROLL_MARGIN <= last_visible,
                    "highlight {highlight} sits flush against the bottom edge"
                );
            }
        }
    }

    #[test]
    fn scrolling_back_up_keeps_one_entry_above_the_highlight() {
        let rows = 5;
        let mut first = 15;
        for highlight in (0..20).rev() {
            first = first_visible(rows, 20, highlight, first);
            let last_visible = first + rows - 1;
            assert!(
                (first..=last_visible).contains(&highlight),
                "highlight {highlight} outside {first}..={last_visible}"
            );
            if highlight >= SCROLL_MARGIN {
                assert!(
                    highlight - SCROLL_MARGIN >= first,
                    "highlight {highlight} sits flush against the top edge"
                );
            }
        }
    }

    #[test]
    fn the_margin_is_dropped_at_the_true_ends_of_the_list() {
        // There is nothing above the first entry to keep in view.
        assert_eq!(first_visible(5, 20, 0, 10), 0);
        assert_eq!(first_visible(5, 20, 19, 0), 15, "the last full viewport");
    }

    #[test]
    fn the_viewport_never_runs_past_the_end_of_the_list() {
        for highlight in 0..20 {
            for previous in [0, 5, 15, 19, 100] {
                let first = first_visible(5, 20, highlight, previous);
                assert!(first + 5 <= 20, "first {first} with highlight {highlight}");
            }
        }
    }

    #[test]
    fn a_stationary_highlight_does_not_move_the_viewport() {
        // Reopening on the same entry must not jump the list around.
        assert_eq!(first_visible(5, 20, 10, 7), 7);
    }

    #[test]
    fn the_highlight_is_always_in_view_for_every_count_and_monitor_size() {
        // SC-005 as a property, across the sizes a user might actually have.
        for size in [(1280, 720), (1920, 1080), (3840, 2160), (800, 600)] {
            for count in [1, 2, 7, 20, 63] {
                let metrics = list_metrics(&G, size, 1.0, count);
                let rows = metrics.visible_rows;
                let mut first = 0;
                // Walk the whole list forwards then backwards, as Tab and Shift+Tab would.
                for highlight in (0..count).chain((0..count).rev()) {
                    first = first_visible(rows, count, highlight, first);
                    assert!(
                        highlight >= first && highlight < first + rows,
                        "{size:?}/{count}: highlight {highlight} outside {first}..{}",
                        first + rows
                    );
                }
            }
        }
    }

    #[test]
    fn wrapping_from_the_last_entry_to_the_first_scrolls_back_to_the_top() {
        // FR-004's wrap has to take the viewport with it.
        let first = first_visible(5, 20, 19, 0);
        assert_eq!(first_visible(5, 20, 0, first), 0);
    }

    #[test]
    fn a_viewport_too_small_for_the_margin_still_keeps_the_highlight_visible() {
        // A one-row viewport cannot honour a one-entry margin; visibility still wins.
        for highlight in 0..20 {
            let first = first_visible(1, 20, highlight, 0);
            assert_eq!(
                first, highlight,
                "highlight {highlight} must be the one row"
            );
        }
    }

    #[test]
    fn the_list_viewport_is_unchanged_by_the_shared_grid_arithmetic() {
        // One column means `first_visible_entry` is `first_visible`, so US1's behaviour is the
        // same function it always was.
        let metrics = list_metrics(&G, (1280, 720), 1.0, 20);
        let mut first = 0;
        for highlight in (0..20).chain((0..20).rev()) {
            let expected = first_visible(metrics.visible_rows, 20, highlight, first);
            first = first_visible_entry(&metrics, 20, highlight, first);
            assert_eq!(first, expected, "highlight {highlight}");
        }
    }

    #[test]
    fn the_grid_scrolls_by_whole_rows_and_keeps_the_highlight_in_view() {
        // A cell that changed column as the viewport moved would make the grid unreadable while
        // navigating, so the first visible entry is always the start of a row.
        let metrics = grid_metrics(&G, HD, 1.0, 40);
        let columns = metrics.columns;
        let visible = metrics.visible_entries();
        assert!(metrics.scrolls(40));

        let mut first = 0;
        for highlight in (0..40).chain((0..40).rev()) {
            first = first_visible_entry(&metrics, 40, highlight, first);
            assert_eq!(first % columns, 0, "highlight {highlight} split a row");
            assert!(
                highlight >= first && highlight < first + visible,
                "highlight {highlight} outside {first}..{}",
                first + visible
            );
        }
    }

    #[test]
    fn a_grid_that_fits_never_scrolls() {
        let metrics = grid_metrics(&G, HD, 1.0, 12);
        assert!(!metrics.scrolls(12));
        for highlight in 0..12 {
            assert_eq!(first_visible_entry(&metrics, 12, highlight, 0), 0);
        }
    }

    // --- T040: the icon slot (FR-036, FR-052, SC-015) ------------------------

    #[test]
    fn the_icon_slot_is_the_themed_text_height() {
        // FR-052, stated as directly as it can be: the slot *is* the text height, so there is no
        // second rule that could drift from it.
        let metrics = list_metrics(&G, HD, 1.0, 5);
        assert_eq!(metrics.icon_slot(), metrics.text_height);
        assert_eq!(metrics.icon_slot(), G.text_line_height);
    }

    #[test]
    fn raising_the_themed_text_height_raises_the_icons_with_it() {
        // FR-052's purpose: a user who makes the overlay bigger to read it gets bigger icons too,
        // not the same small icons beside larger type.
        let bigger = Geometry {
            text_line_height: G.text_line_height * 2,
            ..G
        };
        let plain = list_metrics(&G, HD, 1.0, 5);
        let raised = list_metrics(&bigger, HD, 1.0, 5);

        assert_eq!(raised.icon_slot(), plain.icon_slot() * 2);
        assert_eq!(raised.icon_advance(), plain.icon_advance() * 2);
    }

    #[test]
    fn the_icon_slot_scales_with_the_monitor() {
        // FR-039: a scaled monitor asks for a larger icon rather than upscaling a small one, and
        // the slot is what the rasteriser is told to render into.
        let plain = list_metrics(&G, HD, 1.0, 5);
        let scaled = list_metrics(&G, (3840, 2160), 2.0, 5);
        assert_eq!(scaled.icon_slot(), plain.icon_slot() * 2);
    }

    #[test]
    fn an_icon_costs_a_row_its_slot_plus_a_gap() {
        // FR-036a: icons occupy real horizontal space, which is what makes names truncate sooner.
        let metrics = list_metrics(&G, HD, 1.0, 5);
        assert!(
            metrics.icon_advance() > metrics.icon_slot(),
            "an icon flush against the name it precedes would be unreadable"
        );
        assert!(
            metrics.icon_advance() < metrics.icon_slot() * 2,
            "the gap is a separator, not a second icon's worth of space"
        );
    }

    #[test]
    fn the_grid_gets_the_same_slot_as_the_list() {
        // Both presentations size an icon from the same text height, so a program's icon is the
        // same size in either (FR-035, FR-052).
        let list = list_metrics(&G, HD, 1.0, 8);
        let grid = grid_metrics(&G, HD, 1.0, 8);
        assert_eq!(list.icon_slot(), grid.icon_slot());
    }

    #[test]
    fn nothing_about_a_row_or_the_viewport_depends_on_icons() {
        // FR-036 and SC-015, proved structurally: `Metrics` has no icon input at all, so the row
        // height, the entry count and the visible-entry count are literally the same values the
        // pre-feature build computed. This test is the assertion that no future edit sneaks an
        // icon term into any of them — it reads the whole shape and compares it against the
        // documented pre-feature numbers.
        for count in [1, 3, 20, 100] {
            let metrics = list_metrics(&G, HD, 1.0, count);
            assert_eq!(
                metrics.row_height,
                G.text_line_height + G.row_padding * 2,
                "{count} entries: the row is still one text line plus its padding"
            );
            assert_eq!(
                metrics.height,
                metrics.row_height * metrics.visible_rows as u32 + metrics.padding * 2,
                "{count} entries: the overlay is still exactly its rows plus padding"
            );
            assert_eq!(metrics.visible_entries(), metrics.visible_rows);
            assert_eq!(
                metrics.cell_rect(0).3,
                metrics.row_height,
                "{count} entries: a row's drawn height is its full pitch"
            );
        }

        let grid = grid_metrics(&G, HD, 1.0, 20);
        assert_eq!(
            grid.row_height,
            grid.miniature_height + GRID_LABEL_HEIGHT + grid.gap,
            "the grid cell is unchanged too"
        );
    }

    // --- T064: shedding content inside a miniature (FR-037, FR-038) ----------

    /// The grid every shedding test measures against.
    fn grid() -> Metrics {
        grid_metrics(&G, HD, 1.0, 12)
    }

    /// The largest a miniature title is drawn at — the themed text height, which at the default
    /// text size is the text line itself.
    fn cap() -> f64 {
        f64::from(G.text_line_height)
    }

    /// Which of FR-038's states a rectangle ended in, named as the requirement names them.
    fn state(content: &MiniatureContent) -> &'static str {
        match (content.icon.is_some(), content.title.is_some()) {
            (true, true) => "icon+title",
            (true, false) => "icon",
            (false, true) => "title",
            (false, false) => "none",
        }
    }

    #[test]
    fn a_roomy_rectangle_shows_its_icon_beside_its_title() {
        let metrics = grid();
        let rect = (10.0, 20.0, 120.0, 68.0);
        let content = metrics.miniature_content(rect, cap(), true);
        let icon = content.icon.expect("a rectangle this size holds an icon");
        let title = content.title.expect("and its title alongside it (FR-037)");

        assert!(
            close(icon.2, icon.3),
            "the icon keeps its square slot: {icon:?}"
        );
        assert!(
            icon.0 + icon.2 <= title.rect.0 + 0.001,
            "the title starts after the icon rather than under it: {icon:?} against {:?}",
            title.rect
        );
        assert!(
            icon.0 >= rect.0 - 0.001
                && icon.1 >= rect.1 - 0.001
                && icon.0 + icon.2 <= rect.0 + rect.2 + 0.001
                && icon.1 + icon.3 <= rect.1 + rect.3 + 0.001,
            "the icon escapes the window rectangle it belongs to: {icon:?} in {rect:?}"
        );
        assert!(
            title.rect.0 + title.rect.2 <= rect.0 + rect.2 + 0.001,
            "and neither does the title: {:?} in {rect:?}",
            title.rect
        );
        assert!(
            title.font_size <= cap(),
            "a miniature title never outgrows the themed text height (FR-046)"
        );
    }

    #[test]
    fn a_shrinking_rectangle_sheds_its_title_and_then_its_icon() {
        // FR-038 in one sweep: a rectangle of a fixed shape shrunk from a whole miniature down to
        // a sliver passes through all three states, in the documented order, and through no
        // others — no flapping back and forth at a threshold, and never a title without an icon.
        let metrics = grid();
        let mut seen: Vec<&'static str> = Vec::new();
        let mut width = 240.0_f64;
        while width >= 2.0 {
            let rect = (0.0, 0.0, width, width * 9.0 / 16.0);
            let content = metrics.miniature_content(rect, cap(), true);
            let now = state(&content);
            if seen.last() != Some(&now) {
                seen.push(now);
            }
            assert!(
                content.title.is_none() || content.icon.is_some(),
                "width {width}: kept the title and shed the icon, which is FR-038's order backwards"
            );
            width -= 0.5;
        }
        assert_eq!(
            seen,
            vec!["icon+title", "icon", "none"],
            "the three states, once each, in FR-038's order"
        );
    }

    #[test]
    fn a_title_is_shed_at_the_size_it_stops_being_legible() {
        // The first threshold, isolated: a rectangle wide enough that only its height decides.
        let metrics = grid();
        let legible = metrics.miniature_content((0.0, 0.0, 400.0, 24.0), cap(), true);
        let illegible = metrics.miniature_content((0.0, 0.0, 400.0, 20.0), cap(), true);

        assert_eq!(state(&legible), "icon+title");
        assert_eq!(
            state(&illegible),
            "icon",
            "a title below the legible size is dropped rather than drawn as a smear (FR-015b)"
        );
        assert!(
            legible.title.expect("legible").font_size >= MINIATURE_MIN_TEXT_HEIGHT,
            "and what is kept is kept because it is legible"
        );
    }

    #[test]
    fn an_icon_is_shed_at_the_size_it_stops_being_recognisable() {
        // The second threshold, isolated: a square rectangle, too small for a title either way,
        // so the icon is the only thing left to shed.
        let metrics = grid();
        assert_eq!(
            state(&metrics.miniature_content((0.0, 0.0, 15.0, 15.0), cap(), true)),
            "icon"
        );
        assert_eq!(
            state(&metrics.miniature_content((0.0, 0.0, 12.0, 12.0), cap(), true)),
            "none",
            "below its own minimum the icon goes too, leaving the rectangle empty (FR-038)"
        );
    }

    #[test]
    fn an_icon_alone_is_centred_in_its_rectangle() {
        let metrics = grid();
        let rect = (30.0, 40.0, 20.0, 20.0);
        let icon = metrics
            .miniature_content(rect, cap(), true)
            .icon
            .expect("a rectangle this size still holds an icon");
        assert!(
            close(icon.0 - rect.0, rect.0 + rect.2 - (icon.0 + icon.2)),
            "an icon with no title beside it sits in the middle: {icon:?} in {rect:?}"
        );
    }

    #[test]
    fn a_rectangle_too_small_for_any_content_is_still_a_rectangle() {
        // FR-038's "MUST still be drawn in every case", from the other side: the window whose
        // rectangle sheds everything still *has* a rectangle, in its own position and proportion.
        let metrics = grid();
        let rect = mapped((0, 0), (38, 21), (0, 0)).expect("a tiny window still maps");
        assert_eq!(state(&metrics.miniature_content(rect, cap(), true)), "none");
        assert!(
            rect.2 > 0.0 && rect.3 > 0.0,
            "the rectangle survives what it cannot hold: {rect:?}"
        );

        // And a degenerate rectangle is answered rather than panicked over.
        assert_eq!(
            metrics.miniature_content((0.0, 0.0, 0.0, 0.0), cap(), true),
            MiniatureContent::default()
        );
    }

    #[test]
    fn a_fullscreen_windows_icon_is_not_scaled_up_to_fill_it() {
        // FR-052 and the spec's edge case: the icon follows the themed text height, so the one
        // window on an otherwise empty workspace gets the same icon every other window gets
        // rather than a mural.
        let metrics = grid();
        let whole = metrics
            .miniature_content((0.0, 0.0, 240.0, 135.0), cap(), true)
            .icon
            .expect("a fullscreen window's rectangle holds an icon");
        let quarter = metrics
            .miniature_content((0.0, 0.0, 120.0, 67.5), cap(), true)
            .icon
            .expect("and so does a quarter of one");

        assert!(
            close(whole.2, f64::from(metrics.icon_slot())),
            "the icon is the themed slot, not a fraction of the rectangle: {whole:?}"
        );
        assert!(
            close(whole.2, quarter.2),
            "the same icon at two rectangle sizes: {whole:?} against {quarter:?}"
        );
        assert!(
            whole.2 * 4.0 < 240.0,
            "an icon filling the rectangle would hide the layout the miniature is for"
        );
    }

    #[test]
    fn the_miniature_icon_is_the_size_the_list_draws_the_same_icon_at() {
        // A program's icon is the same size in either presentation, because both take it from the
        // one slot (FR-035, FR-052).
        let metrics = grid();
        let icon = metrics
            .miniature_content((0.0, 0.0, 240.0, 135.0), cap(), true)
            .icon
            .expect("an icon");
        assert!(close(
            icon.2,
            f64::from(list_metrics(&G, HD, 1.0, 5).icon_slot())
        ));
    }

    #[test]
    fn with_icons_off_the_title_keeps_the_space_they_would_have_taken() {
        // FR-056: icons off reserves nothing, so the miniature is exactly what it was before this
        // feature — a rectangle with its title in it.
        let metrics = grid();
        let rect = (0.0, 0.0, 120.0, 68.0);
        let with = metrics.miniature_content(rect, cap(), true);
        let without = metrics.miniature_content(rect, cap(), false);

        assert_eq!(state(&without), "title");
        let (crowded, roomy) = (
            with.title.expect("the icon leaves room for a title here"),
            without.title.expect("and so does its absence"),
        );
        assert!(
            roomy.rect.2 > crowded.rect.2,
            "the title did not get the icon's space back: {:?} against {:?}",
            roomy.rect,
            crowded.rect
        );
        assert!(
            roomy.rect.0 < crowded.rect.0,
            "and it starts where it used to rather than where the icon left it"
        );
        assert!(close(roomy.font_size, crowded.font_size));
    }

    #[test]
    fn content_never_escapes_the_rectangle_it_belongs_to() {
        // Whatever is drawn is drawn *inside* the window's rectangle, so one window's icon can
        // never smear across its neighbour's (FR-037, FR-015a).
        let metrics = grid();
        for width in [3.0_f64, 9.0, 14.0, 30.0, 61.0, 100.0, 240.0] {
            for aspect in [0.2_f64, 0.5625, 1.0, 3.0] {
                let rect = (5.0, 7.0, width, width * aspect);
                let content = metrics.miniature_content(rect, cap(), true);
                let inside = |box_: (f64, f64, f64, f64)| {
                    box_.0 >= rect.0 - 0.001
                        && box_.1 >= rect.1 - 0.001
                        && box_.0 + box_.2 <= rect.0 + rect.2 + 0.001
                        && box_.1 + box_.3 <= rect.1 + rect.3 + 0.001
                };
                if let Some(icon) = content.icon {
                    assert!(inside(icon), "icon {icon:?} escapes {rect:?}");
                }
                if let Some(title) = content.title {
                    assert!(
                        inside(title.rect),
                        "title {:?} escapes {rect:?}",
                        title.rect
                    );
                    assert!(
                        title.rect.2 > 0.0,
                        "a title box with no width is not a title"
                    );
                }
            }
        }
    }
}
