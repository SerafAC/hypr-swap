//! Overlay geometry: how big an entry is, how big the surface is, and which slice of the entry
//! list is on screen (FR-019, SC-005, research.md R16).
//!
//! Pure arithmetic, so SC-005's twenty-workspace case is testable at every monitor size without a
//! compositor. The constants below are the documented ones from `contracts/config.md` — they are
//! deliberately *not* settings (Principle II), but they live here in one place (Principle III) so
//! they could become settings if a requirement ever asked.
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

/// The documented cap: the overlay claims at most this fraction of the monitor in each axis, so
/// the surrounding desktop stays visible as context (FR-019, research.md R16).
pub const OVERLAY_WIDTH_FRACTION: f64 = 0.8;
pub const OVERLAY_HEIGHT_FRACTION: f64 = 0.8;

/// One text line, in logical pixels. A list row is exactly one line tall regardless of how many
/// windows the workspace holds — the titles are laid out along the row and ellipsised (FR-014).
pub const TEXT_LINE_HEIGHT: u32 = 20;

/// Padding above and below the text in a list row, in logical pixels.
pub const ROW_PADDING: u32 = 8;

/// Padding between the entry column and the edge of the overlay, in logical pixels.
pub const OVERLAY_PADDING: u32 = 12;

/// The highlight never sits closer than this many entries to a scrolled edge, so the user can
/// always see where the list continues (research.md R16).
pub const SCROLL_MARGIN: usize = 1;

/// The surface size and the entry geometry for one overlay, in device pixels.
///
/// Everything here is already multiplied by the monitor's scale, so the renderer paints in the
/// same units the buffer is allocated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Overlay surface width.
    pub width: u32,
    /// Overlay surface height — exactly the rows on screen plus padding, so a short list gets a
    /// short overlay rather than an empty box.
    pub height: u32,
    /// One entry's height. Fixed: it does not vary with the number of entries (FR-019).
    pub row_height: u32,
    /// The text line inside a row. The renderer sizes its font from this, so type scales with
    /// the monitor exactly as the rows do.
    pub text_height: u32,
    /// Padding between the entry column and the surface edge.
    pub padding: u32,
    /// How many entries are on screen at once.
    pub visible_rows: usize,
}

impl Metrics {
    /// Whether the list is taller than the cap allows, i.e. whether the viewport scrolls.
    #[must_use]
    pub fn scrolls(&self, entry_count: usize) -> bool {
        entry_count > self.visible_rows
    }

    /// The rectangle of the `slot`-th row on screen — `(x, y, width, height)`, device pixels.
    ///
    /// `slot` is the position in the viewport, not the index in the entry list; the caller adds
    /// [`first_visible`] to go the other way.
    #[must_use]
    pub fn row_rect(&self, slot: usize) -> (u32, u32, u32, u32) {
        (
            self.padding,
            self.padding + self.row_height * slot as u32,
            self.width.saturating_sub(self.padding * 2),
            self.row_height,
        )
    }
}

/// Size an overlay for the flat-list presentation on one monitor.
///
/// `monitor_size` is in device pixels and `scale` is that monitor's scale factor, so the result
/// is the same physical size on a `HiDPI` monitor as on a standard one.
#[must_use]
pub fn list_metrics(monitor_size: (u32, u32), scale: f32, entry_count: usize) -> Metrics {
    let text_height = scaled(TEXT_LINE_HEIGHT, scale);
    let row_height = scaled(TEXT_LINE_HEIGHT + ROW_PADDING * 2, scale);
    let padding = scaled(OVERLAY_PADDING, scale);

    let width = fraction(monitor_size.0, OVERLAY_WIDTH_FRACTION);
    let max_height = fraction(monitor_size.1, OVERLAY_HEIGHT_FRACTION);

    let visible_rows = entry_count.clamp(1, rows_that_fit(max_height, row_height, padding));

    Metrics {
        width,
        height: row_height * visible_rows as u32 + padding * 2,
        row_height,
        text_height,
        padding,
        visible_rows,
    }
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
/// Keeps the row height — entries never shrink (FR-019) — and changes only how many of them are
/// on screen, so the painted rows always fit the surface actually agreed to.
#[must_use]
pub fn refit(metrics: Metrics, width: u32, height: u32, entry_count: usize) -> Metrics {
    let visible_rows =
        entry_count
            .max(1)
            .min(rows_that_fit(height, metrics.row_height, metrics.padding));
    Metrics {
        width,
        height,
        visible_rows,
        ..metrics
    }
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

/// Round a logical-pixel constant to device pixels, never to zero.
fn scaled(logical: u32, scale: f32) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    ((f64::from(logical) * f64::from(scale)).round() as u32).max(1)
}

fn fraction(pixels: u32, of: f64) -> u32 {
    ((f64::from(pixels) * of).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1080p monitor at scale 1 — the reference case every expectation below is anchored to.
    const HD: (u32, u32) = (1920, 1080);

    // --- Metrics -----------------------------------------------------------

    #[test]
    fn the_overlay_is_capped_at_eighty_percent_of_the_monitor() {
        // FR-019's documented fraction.
        let metrics = list_metrics(HD, 1.0, 100);
        assert_eq!(metrics.width, 1536, "80 % of 1920");
        assert!(
            metrics.height <= 864,
            "80 % of 1080, got {}",
            metrics.height
        );
    }

    #[test]
    fn a_short_list_gets_a_short_overlay_rather_than_an_empty_box() {
        let metrics = list_metrics(HD, 1.0, 3);
        assert_eq!(metrics.visible_rows, 3);
        assert_eq!(metrics.height, metrics.row_height * 3 + metrics.padding * 2);
        assert!(!metrics.scrolls(3));
    }

    #[test]
    fn the_row_height_is_one_text_line_plus_its_padding() {
        let metrics = list_metrics(HD, 1.0, 5);
        assert_eq!(metrics.row_height, TEXT_LINE_HEIGHT + ROW_PADDING * 2);
    }

    #[test]
    fn entries_keep_their_size_no_matter_how_many_there_are() {
        // FR-019: the overlay scrolls instead of scaling entries down. This is the requirement.
        let reference = list_metrics(HD, 1.0, 1).row_height;
        for count in [2, 5, 20, 100, 1000] {
            assert_eq!(
                list_metrics(HD, 1.0, count).row_height,
                reference,
                "{count} entries changed the row height"
            );
        }
    }

    #[test]
    fn twenty_workspaces_keep_full_entry_size_whether_or_not_they_fit() {
        // SC-005. On 1080p all twenty fit inside the cap; on a 720p monitor they do not, and the
        // difference is that the overlay scrolls — never that the rows get smaller (FR-019).
        let roomy = list_metrics(HD, 1.0, 20);
        assert_eq!(roomy.row_height, 36);
        assert_eq!(roomy.visible_rows, 20, "20 × 36 + 24 fits inside 864");
        assert!(!roomy.scrolls(20));
        assert!(roomy.height <= 864);

        let cramped = list_metrics((1280, 720), 1.0, 20);
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
                let metrics = list_metrics(size, 1.0, count);
                assert_eq!(
                    metrics.row_height,
                    TEXT_LINE_HEIGHT + ROW_PADDING * 2,
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
        let metrics = list_metrics(HD, 1.0, 5);
        assert_eq!(metrics.text_height, TEXT_LINE_HEIGHT);
        assert_eq!(metrics.row_height - metrics.text_height, ROW_PADDING * 2);
    }

    #[test]
    fn every_constant_is_multiplied_by_the_monitor_scale() {
        let one = list_metrics(HD, 1.0, 5);
        let two = list_metrics((3840, 2160), 2.0, 5);
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
        let metrics = list_metrics((2560, 1440), 1.5, 10);
        assert_eq!(metrics.row_height, 54, "36 logical px at 1.5");
        assert_eq!(metrics.padding, 18);
    }

    #[test]
    fn a_nonsense_scale_falls_back_to_one_rather_than_collapsing_the_overlay() {
        for scale in [0.0, -1.0, f32::NAN] {
            let metrics = list_metrics(HD, scale, 5);
            assert_eq!(metrics.row_height, 36, "scale {scale}");
        }
    }

    #[test]
    fn a_monitor_too_short_for_even_one_row_still_shows_one() {
        let metrics = list_metrics((640, 40), 1.0, 10);
        assert_eq!(metrics.visible_rows, 1);
    }

    #[test]
    fn row_rects_stack_without_gaps_or_overlap() {
        let metrics = list_metrics(HD, 1.0, 20);
        for slot in 1..metrics.visible_rows {
            let (_, previous_y, _, height) = metrics.row_rect(slot - 1);
            let (_, y, _, _) = metrics.row_rect(slot);
            assert_eq!(y, previous_y + height, "slot {slot}");
        }
        let (_, last_y, _, height) = metrics.row_rect(metrics.visible_rows - 1);
        assert!(last_y + height + metrics.padding <= metrics.height);
    }

    // --- Refitting to a compositor-chosen size -----------------------------

    #[test]
    fn refitting_to_a_shorter_surface_shows_fewer_rows_at_the_same_size() {
        let metrics = list_metrics(HD, 1.0, 20);
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
        let metrics = list_metrics(HD, 1.0, 3);
        assert_eq!(refit(metrics, 1536, 864, 3).visible_rows, 3);
    }

    #[test]
    fn a_surface_too_short_for_a_row_still_shows_one() {
        let metrics = list_metrics(HD, 1.0, 20);
        assert_eq!(refit(metrics, 1536, 10, 20).visible_rows, 1);
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
                let metrics = list_metrics(size, 1.0, count);
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
}
