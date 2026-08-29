//! The overlay's appearance as resolved values: the palette, the font, and the geometry.
//!
//! Everything the overlay draws with has exactly one definition here (FR-045–FR-047). Before this
//! module existed the colours were `const`s in [`crate::ui::render`] and the dimensions `const`s in
//! [`crate::ui::layout`]; collapsing both into one place is what makes "which value is actually in
//! effect" answerable from a single file, and what lets the FR-050 precedence chain —
//! explicit override, then named theme, then default — be written once rather than per setting.
//!
//! The module is pure: it parses, clamps and resolves, and never touches the filesystem or the
//! compositor. [`crate::config`] hands it what the user wrote; the renderer is handed the result.
//!
//! A built-in theme is a **palette and nothing more** (FR-049, research.md R24). Fonts and geometry
//! have one shared default each and are reachable only through per-key overrides, so switching
//! theme can never move the layout (SC-023).
//!
//! The catalogue of every value, its form, its range and its default is
//! `specs/002-overlay-visuals/contracts/style-values.md`, and that document is authoritative
//! (FR-061).

use crate::diag::{Condition, Diagnostic};

// --- Colour (T006, research.md R25) -----------------------------------------

/// A colour as cairo takes it: straight RGBA with every channel in `0.0..=1.0`.
///
/// The same shape [`crate::ui::render`] already painted with, so the refactor that introduced this
/// type changed no arithmetic — only where the numbers come from (FR-049a).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colour {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

/// What a colour that could not be read looks like to the caller.
///
/// One variant, because there is one accepted notation and everything else is equally wrong
/// (research.md R25). The `Display` text is the "what was wrong with it" half of FR-059.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColourError;

impl std::fmt::Display for ColourError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("expected #rgb, #rrggbb or #rrggbbaa")
    }
}

impl Colour {
    #[must_use]
    pub const fn new(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Parse the one accepted notation: `#rgb`, `#rrggbb` or `#rrggbbaa`, alpha opaque when
    /// omitted (research.md R25, `contracts/style-values.md`).
    ///
    /// Total: every other input — no `#`, a wrong length, a non-hex digit, the empty string — is
    /// an error rather than a partial reading, which is what makes FR-059's "only that setting
    /// falls back" safe.
    ///
    /// # Errors
    /// [`ColourError`] for anything that is not one of the three forms.
    pub fn parse(text: &str) -> Result<Self, ColourError> {
        let digits = text.strip_prefix('#').ok_or(ColourError)?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ColourError);
        }
        // `is_ascii_hexdigit` above guarantees every one of these parses.
        let nibble = |at: usize| f64::from(u8::from_str_radix(&digits[at..=at], 16).unwrap_or(0));
        let byte = |at: usize| f64::from(u8::from_str_radix(&digits[at..at + 2], 16).unwrap_or(0));

        Ok(match digits.len() {
            // The short form doubles each digit, so `#f00` is `#ff0000` rather than `#0f0000`.
            3 => Self::new(nibble(0) / 15.0, nibble(1) / 15.0, nibble(2) / 15.0, 1.0),
            6 => Self::new(byte(0) / 255.0, byte(2) / 255.0, byte(4) / 255.0, 1.0),
            8 => Self::new(
                byte(0) / 255.0,
                byte(2) / 255.0,
                byte(4) / 255.0,
                byte(6) / 255.0,
            ),
            _ => return Err(ColourError),
        })
    }

    /// The tuple cairo's `set_source_rgba` takes.
    #[must_use]
    pub fn rgba(self) -> (f64, f64, f64, f64) {
        (self.red, self.green, self.blue, self.alpha)
    }

    /// `#rrggbb`, the form pango markup's `foreground` attribute takes.
    ///
    /// Alpha is dropped because pango markup has no place for it; the renderer only formats
    /// colours this way for text, which is drawn opaque.
    #[must_use]
    pub fn hex(self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            Self::channel(self.red),
            Self::channel(self.green),
            Self::channel(self.blue)
        )
    }

    /// `#rrggbbaa` — the full colour including its opacity, which is the form the recorded
    /// baseline holds and the form a paint record names a drawn colour by (research.md R22).
    ///
    /// The renderer's evidence has to carry alpha: the backdrop is the one themed colour that is
    /// not opaque, and a theme that got its transparency wrong would otherwise look identical to
    /// one that got it right.
    #[must_use]
    pub fn hex_rgba(self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            Self::channel(self.red),
            Self::channel(self.green),
            Self::channel(self.blue),
            Self::channel(self.alpha)
        )
    }

    /// One channel as the 8-bit value the two hex forms print, rounded half away from zero
    /// (`contracts/style-values.md`).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn channel(value: f64) -> u8 {
        // Clamped to `0.0..=1.0` first, so the scaled value is always inside `u8`.
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

// --- Geometry (T008, FR-047) ------------------------------------------------

/// The ten configurable dimensions of FR-047, in logical units.
///
/// These were `pub const`s in [`crate::ui::layout`]. They are logical pixels and fractions,
/// scaled per monitor by that module's existing rule, so there are no per-monitor variants
/// (FR-055).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// Entry text height; the row height follows it.
    pub text_line_height: u32,
    /// Vertical padding within a row.
    pub row_padding: u32,
    /// The overlay's outer padding.
    pub overlay_padding: u32,
    /// Overlay width cap, as a fraction of the monitor (FR-019).
    pub width_fraction: f64,
    /// Overlay height cap, as a fraction of the monitor (FR-019).
    pub height_fraction: f64,
    /// A grid cell's miniature width.
    pub grid_cell_width: u32,
    /// A grid cell's miniature height.
    pub grid_cell_height: u32,
    /// Space between grid cells.
    pub grid_gap: u32,
    /// Corner rounding, as a fraction of the row height.
    pub corner_radius: f64,
    /// Active-mark width, as a fraction of the row height.
    pub mark_width: f64,
}

impl Geometry {
    /// Exactly the constants the pre-feature build used, so an unconfigured overlay is unchanged
    /// (FR-049a, SC-018). The recorded originals are `tests/fixtures/baseline/style.json`.
    ///
    /// A `const` as well as the [`Default`] impl, so [`crate::ui::layout`]'s tests can anchor
    /// their expectations to it exactly as they used to anchor them to that module's constants.
    pub const DEFAULT: Self = Self {
        text_line_height: 20,
        row_padding: 8,
        overlay_padding: 12,
        width_fraction: 0.8,
        height_fraction: 0.8,
        grid_cell_width: 240,
        grid_cell_height: 135,
        grid_gap: 12,
        corner_radius: 0.28,
        mark_width: 0.12,
    };
}

impl Default for Geometry {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Geometry {
    /// The label line beneath a miniature: one text line plus the space separating it from the
    /// miniature it names (FR-015).
    ///
    /// Derived rather than settable: it has no meaning independent of the two values it is built
    /// from, and making it a setting would let it disagree with them (plan.md → Complexity
    /// Tracking).
    #[must_use]
    pub fn grid_label_height(&self) -> u32 {
        self.text_line_height + self.row_padding
    }
}

/// One geometry setting as data: how it is spelled, what it accepts, and how it is read and
/// written.
///
/// The ranges are `const` data rather than scattered conditionals, which is what lets one clamping
/// function, one configuration loop and one catalogue test cover all ten (research.md R26). They
/// are authoritative here and reproduced in `contracts/style-values.md`.
/// Reading and writing one geometry field, as the pair of fn pointers the table holds.
type Access = (fn(&Geometry) -> f64, fn(&mut Geometry, f64));

pub struct GeometrySetting {
    /// The key under `[style]`.
    pub key: &'static str,
    pub min: f64,
    pub max: f64,
    /// Whether the value is a whole number of logical pixels rather than a fraction — which is
    /// what decides how it is read from the file and how it is spelled back in a message.
    pub integral: bool,
    read: fn(&Geometry) -> f64,
    write: fn(&mut Geometry, f64),
}

impl GeometrySetting {
    /// This setting's current value.
    #[must_use]
    pub fn read(&self, geometry: &Geometry) -> f64 {
        (self.read)(geometry)
    }

    /// Set it, rounding to a whole unit where the setting takes one.
    pub fn write(&self, geometry: &mut Geometry, value: f64) {
        (self.write)(geometry, value);
    }

    /// The value as a message spells it: `28` rather than `28.0` for a whole-unit setting.
    #[must_use]
    pub fn show(&self, value: f64) -> String {
        if self.integral {
            format!("{}", value.round())
        } else {
            format!("{value}")
        }
    }

    /// Bring a value within range, and say so when it had to move (FR-054, FR-059).
    ///
    /// Out-of-range is clamped rather than rejected: a user who writes a cap of `5.0` more likely
    /// meant "as large as possible" than "the default" (research.md R26).
    #[must_use]
    pub fn clamp(&self, value: f64) -> (f64, Option<Diagnostic>) {
        // A `NaN` cannot be compared into range at all, so it is treated as the failure it is and
        // pinned to the minimum rather than propagating through the layout arithmetic.
        if value.is_nan() {
            let used = self.min;
            return (used, Some(self.clamped_to("is not a number", used)));
        }
        if value < self.min {
            let used = self.min;
            let was = format!(
                "{} is below the minimum {}",
                self.show(value),
                self.show(used)
            );
            return (used, Some(self.clamped_to(&was, used)));
        }
        if value > self.max {
            let used = self.max;
            let was = format!(
                "{} is above the maximum {}",
                self.show(value),
                self.show(used)
            );
            return (used, Some(self.clamped_to(&was, used)));
        }
        (value, None)
    }

    fn clamped_to(&self, was: &str, used: f64) -> Diagnostic {
        Diagnostic::new(
            Condition::InvalidConfigValue,
            subject(self.key),
            format!("{was}; using {}", self.show(used)),
        )
    }
}

/// Read and write a whole-unit setting, as fn pointers the table below can hold.
macro_rules! integral {
    ($field:ident) => {
        (
            |geometry: &Geometry| f64::from(geometry.$field),
            |geometry: &mut Geometry, value: f64| {
                // Every value reaching this point has already been clamped into a range whose
                // bounds are small positive integers, so the cast is exact.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    geometry.$field = value.round() as u32;
                }
            },
        )
    };
}

/// The same for a fractional setting.
macro_rules! fractional {
    ($field:ident) => {
        (
            |geometry: &Geometry| geometry.$field,
            |geometry: &mut Geometry, value: f64| geometry.$field = value,
        )
    };
}

/// Every geometry setting, its range and its accessors — the FR-061 catalogue as data.
pub const GEOMETRY: &[GeometrySetting] = &[
    setting(
        "text_line_height",
        8.0,
        200.0,
        true,
        integral!(text_line_height),
    ),
    setting("row_padding", 0.0, 100.0, true, integral!(row_padding)),
    setting(
        "overlay_padding",
        0.0,
        200.0,
        true,
        integral!(overlay_padding),
    ),
    setting(
        "width_fraction",
        0.1,
        1.0,
        false,
        fractional!(width_fraction),
    ),
    setting(
        "height_fraction",
        0.1,
        1.0,
        false,
        fractional!(height_fraction),
    ),
    setting(
        "grid_cell_width",
        40.0,
        2000.0,
        true,
        integral!(grid_cell_width),
    ),
    setting(
        "grid_cell_height",
        40.0,
        2000.0,
        true,
        integral!(grid_cell_height),
    ),
    setting("grid_gap", 0.0, 200.0, true, integral!(grid_gap)),
    setting("corner_radius", 0.0, 1.0, false, fractional!(corner_radius)),
    setting("mark_width", 0.0, 1.0, false, fractional!(mark_width)),
];

/// Assemble one catalogue row. A `const fn` so the table above stays a single readable column.
const fn setting(
    key: &'static str,
    min: f64,
    max: f64,
    integral: bool,
    access: Access,
) -> GeometrySetting {
    GeometrySetting {
        key,
        min,
        max,
        integral,
        read: access.0,
        write: access.1,
    }
}

// --- Theme (T011, FR-045, FR-049) -------------------------------------------

/// A named palette — the eleven colours of FR-045 and nothing else.
///
/// Fonts and geometry are deliberately absent (research.md R24): that absence is what makes
/// SC-023's "switching theme never moves the layout" a property of the type rather than something
/// tests have to police.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// The value a user writes as `theme = "…"`.
    pub name: &'static str,
    /// Overlay background.
    pub backdrop: Colour,
    /// Highlighted entry background (FR-008).
    pub highlight: Colour,
    /// Active-workspace mark (FR-008).
    pub active_mark: Colour,
    /// Primary entry text — the workspace name.
    pub text: Colour,
    /// Primary text on the highlighted entry.
    pub text_highlighted: Colour,
    /// Secondary text — window names.
    pub text_dim: Colour,
    /// Secondary text on the highlighted entry.
    pub text_dim_highlighted: Colour,
    /// Miniature background (FR-015).
    pub miniature: Colour,
    /// Tiled window rectangle fill.
    pub window: Colour,
    /// Floating window rectangle fill (research.md R7).
    pub window_floating: Colour,
    /// Window rectangle edge.
    pub window_edge: Colour,
}

/// The default theme, and the one an unknown name falls back to (FR-049).
///
/// **Built from the float constants the pre-feature renderer used, never by parsing hex.** Two
/// channels land exactly on an 8-bit half-step, so a round-trip through text could shift them by
/// one and break FR-049a's "byte for byte" claim (`contracts/style-values.md`).
pub const DARK: Theme = Theme {
    name: "dark",
    backdrop: Colour::new(0.09, 0.09, 0.11, 0.93),
    highlight: Colour::new(0.20, 0.42, 0.72, 1.0),
    active_mark: Colour::new(0.42, 0.72, 0.45, 1.0),
    text: Colour::new(0.92, 0.92, 0.94, 1.0),
    text_highlighted: Colour::new(1.0, 1.0, 1.0, 1.0),
    text_dim: Colour::new(0.66, 0.66, 0.70, 1.0),
    text_dim_highlighted: Colour::new(0.86, 0.90, 0.96, 1.0),
    miniature: Colour::new(0.16, 0.16, 0.19, 1.0),
    window: Colour::new(0.30, 0.32, 0.38, 1.0),
    window_floating: Colour::new(0.38, 0.40, 0.48, 1.0),
    window_edge: Colour::new(0.52, 0.55, 0.62, 1.0),
};

/// The light counterpart: the same overlay read against a light desktop (FR-049, T055).
///
/// A palette and nothing else, exactly as [`DARK`] is — no font, no geometry — which is what makes
/// SC-023's "switching theme never moves the layout" true by construction rather than by
/// discipline. The values invert the dark palette's *roles* rather than its channels: the backdrop
/// becomes near-white at the same opacity, the two text colours darken, and the miniature and its
/// window rectangles stay a shade apart from the backdrop so a miniature still reads as a panel.
/// `text_highlighted` stays white because the highlight stays a saturated blue in both themes —
/// the one value the two palettes share, and deliberately so.
pub const LIGHT: Theme = Theme {
    name: "light",
    backdrop: Colour::new(0.97, 0.97, 0.98, 0.93),
    highlight: Colour::new(0.18, 0.44, 0.80, 1.0),
    active_mark: Colour::new(0.16, 0.55, 0.28, 1.0),
    text: Colour::new(0.11, 0.11, 0.14, 1.0),
    text_highlighted: Colour::new(1.0, 1.0, 1.0, 1.0),
    text_dim: Colour::new(0.35, 0.35, 0.40, 1.0),
    text_dim_highlighted: Colour::new(0.88, 0.92, 0.98, 1.0),
    miniature: Colour::new(0.90, 0.90, 0.93, 1.0),
    window: Colour::new(0.76, 0.78, 0.84, 1.0),
    window_floating: Colour::new(0.68, 0.71, 0.79, 1.0),
    window_edge: Colour::new(0.45, 0.48, 0.56, 1.0),
};

/// Every theme a user can name. The first is the default (FR-049).
pub const BUILT_IN: &[Theme] = &[DARK, LIGHT];

/// One colour setting as data, mirroring [`GeometrySetting`]: the key a user writes, and how the
/// value is read from and written into a palette.
pub struct ColourSetting {
    /// The key under `[style]`.
    pub key: &'static str,
    read: fn(&Theme) -> Colour,
    write: fn(&mut Theme, Colour),
}

impl ColourSetting {
    #[must_use]
    pub fn read(&self, theme: &Theme) -> Colour {
        (self.read)(theme)
    }

    pub fn write(&self, theme: &mut Theme, colour: Colour) {
        (self.write)(theme, colour);
    }
}

macro_rules! colour {
    ($key:literal, $field:ident) => {
        ColourSetting {
            key: $key,
            read: |theme: &Theme| theme.$field,
            write: |theme: &mut Theme, colour: Colour| theme.$field = colour,
        }
    };
}

/// The eleven colours of FR-045, in the order `contracts/style-values.md` lists them.
pub const COLOURS: &[ColourSetting] = &[
    colour!("backdrop", backdrop),
    colour!("highlight", highlight),
    colour!("active_mark", active_mark),
    colour!("text", text),
    colour!("text_highlighted", text_highlighted),
    colour!("text_dim", text_dim),
    colour!("text_dim_highlighted", text_dim_highlighted),
    colour!("miniature", miniature),
    colour!("window", window),
    colour!("window_floating", window_floating),
    colour!("window_edge", window_edge),
];

// --- Fonts (FR-046) ---------------------------------------------------------

/// The family the pre-feature renderer asked pango for (FR-049a).
pub const DEFAULT_FONT_FAMILY: &str = "Sans";

/// The em size as a fraction of the row's text line — the pre-feature `FONT_FRACTION`.
pub const DEFAULT_TEXT_SIZE: f64 = 0.78;

/// `text_size`'s range, as `contracts/style-values.md` documents it. Kept in the same shape as
/// [`GEOMETRY`]'s rows so the catalogue test can walk fonts and geometry the same way.
pub const TEXT_SIZE: GeometrySetting = GeometrySetting {
    key: "text_size",
    min: 0.3,
    max: 1.0,
    integral: false,
    // Never used: `text_size` lives on `Style`, not on `Geometry`. Only the key and the range are
    // read, by `resolve` and by the catalogue test.
    read: |_| DEFAULT_TEXT_SIZE,
    write: |_, _| {},
};

/// `font_family`'s key. It has no range — any family name is accepted and an absent one is
/// substituted by the platform without a word (US4-AS5).
pub const FONT_FAMILY_KEY: &str = "font_family";

// --- Style (T011) -----------------------------------------------------------

/// The fully resolved appearance handed to the renderer: one palette plus the font and geometry
/// values.
///
/// This is all `ui/` ever sees. It never sees a theme name or an override, because [`resolve`]
/// has already applied the FR-050 chain — which is what leaves the renderer with no defaults of
/// its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub palette: Theme,
    pub font_family: String,
    /// Fraction of the row's text height (FR-046).
    pub text_size: f64,
    pub geometry: Geometry,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            palette: DARK,
            font_family: DEFAULT_FONT_FAMILY.to_owned(),
            text_size: DEFAULT_TEXT_SIZE,
            geometry: Geometry::default(),
        }
    }
}

// --- Resolution (T012, FR-050) ----------------------------------------------

/// One value as the configuration file held it, before this module has judged it.
///
/// Deliberately free of `toml` types: [`crate::config`] translates, `theme.rs` validates, and this
/// module stays pure (plan.md → Project Structure).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Integer(i64),
    Float(f64),
    /// Anything else, named by its type so a message can say what was found instead.
    Other(&'static str),
}

impl Value {
    /// The type name a diagnostic uses for "expected a string, found …".
    fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "a string",
            Self::Integer(_) => "an integer",
            Self::Float(_) => "a float",
            Self::Other(name) => name,
        }
    }

    /// A number, whichever numeric form the file spelled it in — `12` and `12.0` are the same
    /// value to a user and there is no reason for one to be an error.
    #[allow(clippy::cast_precision_loss)]
    fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }
}

/// What the user asked for, as written and not yet validated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Requested {
    /// The `theme = "…"` name, if one was given.
    pub theme: Option<String>,
    /// The `[style]` table, in file order.
    pub overrides: Vec<(String, Value)>,
}

/// The subject a style diagnostic is reported under.
fn subject(key: &str) -> String {
    format!("config.style.{key}")
}

/// Resolve everything the user wrote into the one [`Style`] the renderer is handed (FR-050).
///
/// The precedence chain is written **once**, here, and it is the same for every value:
///
/// ```text
/// explicit override  →  named theme (colours only)  →  default
/// ```
///
/// Every override is independent. An unparseable or out-of-range one is reported and that value
/// alone falls back or is clamped; every other setting still applies (FR-059, SC-022).
#[must_use]
pub fn resolve(requested: &Requested) -> (Style, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    // Link two of the chain: the named theme supplies the palette, and nothing else (FR-049).
    let mut style = Style {
        palette: named_theme(requested.theme.as_deref(), &mut diagnostics),
        ..Style::default()
    };

    // Link one: explicit overrides, each judged on its own.
    for (key, value) in &requested.overrides {
        apply_override(&mut style, key, value, &mut diagnostics);
    }

    (style, diagnostics)
}

/// The palette a name selects, falling back to the default and reporting an unknown one (FR-058).
fn named_theme(name: Option<&str>, diagnostics: &mut Vec<Diagnostic>) -> Theme {
    let default = BUILT_IN[0];
    let Some(name) = name else {
        return default;
    };
    if let Some(theme) = BUILT_IN.iter().find(|theme| theme.name == name) {
        return *theme;
    }

    let known = BUILT_IN
        .iter()
        .map(|theme| theme.name)
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::new(
        Condition::InvalidConfigValue,
        "config.theme",
        format!(
            "unknown theme {name:?}, using {:?} (built-in: {known})",
            default.name
        ),
    ));
    default
}

/// Apply one `[style]` key, or report why it was not applied.
fn apply_override(style: &mut Style, key: &str, value: &Value, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(colour) = COLOURS.iter().find(|setting| setting.key == key) {
        match value {
            Value::Text(text) => match Colour::parse(text) {
                Ok(parsed) => colour.write(&mut style.palette, parsed),
                Err(e) => diagnostics.push(Diagnostic::new(
                    Condition::InvalidConfigValue,
                    subject(key),
                    format!(
                        "{e}, got {text:?}; using {}",
                        colour.read(&style.palette).hex()
                    ),
                )),
            },
            other => diagnostics.push(Diagnostic::new(
                Condition::InvalidConfigValue,
                subject(key),
                format!(
                    "expected a string, found {}; using {}",
                    other.type_name(),
                    colour.read(&style.palette).hex()
                ),
            )),
        }
        return;
    }

    if let Some(geometry) = GEOMETRY.iter().find(|setting| setting.key == key) {
        match value.as_number() {
            Some(number) => {
                let (used, adjusted) = geometry.clamp(number);
                geometry.write(&mut style.geometry, used);
                diagnostics.extend(adjusted);
            }
            None => diagnostics.push(Diagnostic::new(
                Condition::InvalidConfigValue,
                subject(key),
                format!(
                    "expected a number, found {}; using {}",
                    value.type_name(),
                    geometry.show(geometry.read(&style.geometry))
                ),
            )),
        }
        return;
    }

    if key == TEXT_SIZE.key {
        match value.as_number() {
            Some(number) => {
                let (used, adjusted) = TEXT_SIZE.clamp(number);
                style.text_size = used;
                diagnostics.extend(adjusted);
            }
            None => diagnostics.push(Diagnostic::new(
                Condition::InvalidConfigValue,
                subject(key),
                format!(
                    "expected a number, found {}; using {}",
                    value.type_name(),
                    style.text_size
                ),
            )),
        }
        return;
    }

    if key == FONT_FAMILY_KEY {
        match value {
            // Any family name is accepted. An absent one is the platform's business to substitute
            // and is not something the user is told about (US4-AS5).
            Value::Text(family) => style.font_family.clone_from(family),
            other => diagnostics.push(Diagnostic::new(
                Condition::InvalidConfigValue,
                subject(key),
                format!(
                    "expected a string, found {}; using {:?}",
                    other.type_name(),
                    style.font_family
                ),
            )),
        }
        return;
    }

    diagnostics.push(Diagnostic::new(
        Condition::UnknownConfigKey,
        subject(key),
        "unknown key, ignored".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every channel of a colour, so a parse can be asserted in one line.
    fn channels(colour: Colour) -> [f64; 4] {
        [colour.red, colour.green, colour.blue, colour.alpha]
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    // --- T007: the colour parser (FR-045, research.md R25) ------------------

    #[test]
    fn the_three_accepted_forms_parse() {
        // `#rgb` doubles each digit, so `f` is 255 and not 15.
        let short = Colour::parse("#f80").expect("#rgb is accepted");
        assert!(close(short.red, 1.0), "{short:?}");
        assert!(close(short.green, 8.0 / 15.0), "{short:?}");
        assert!(close(short.blue, 0.0), "{short:?}");

        let long = Colour::parse("#ff8800").expect("#rrggbb is accepted");
        assert!(close(long.red, 1.0) && close(long.blue, 0.0), "{long:?}");
        assert!(close(long.green, 136.0 / 255.0), "{long:?}");

        let with_alpha = Colour::parse("#ff880080").expect("#rrggbbaa is accepted");
        assert!(close(with_alpha.alpha, 128.0 / 255.0), "{with_alpha:?}");
    }

    #[test]
    fn alpha_defaults_to_opaque_when_it_is_not_written() {
        for text in ["#fff", "#ffffff"] {
            let colour = Colour::parse(text).expect("a valid colour");
            assert!(close(colour.alpha, 1.0), "{text} should be opaque");
        }
        assert!(close(
            Colour::parse("#ffffff00").expect("a valid colour").alpha,
            0.0
        ));
    }

    #[test]
    fn parsing_is_case_insensitive() {
        let upper = channels(Colour::parse("#AbCdEf").expect("upper case is accepted"));
        let lower = channels(Colour::parse("#abcdef").expect("lower case is accepted"));
        assert!(
            upper.iter().zip(lower).all(|(a, b)| close(*a, b)),
            "{upper:?} vs {lower:?}"
        );
    }

    #[test]
    fn every_other_input_is_rejected_rather_than_partially_read() {
        // FR-059 turns on this being total: a form that half-parses would apply a colour the user
        // never asked for instead of falling back and saying so.
        for rejected in [
            "",           // empty
            "#",          // no digits
            "fff",        // missing #
            "#ff",        // two digits is not a form
            "#ffff",      // nor four
            "#fffff",     // nor five
            "#fffffff",   // nor seven
            "#fffffffff", // nor nine
            "#gggggg",    // not hex
            "#ff88zz",    // one bad digit is enough
            "rgb(1,2,3)", // a second notation, rejected by decision (research.md R25)
            "red",        // named colours, likewise
            " #ffffff",   // no leading whitespace is trimmed for us
            "#ffffff ",   // nor trailing
        ] {
            assert_eq!(
                Colour::parse(rejected),
                Err(ColourError),
                "{rejected:?} must not parse"
            );
        }
    }

    #[test]
    fn the_error_says_what_was_expected() {
        // The "what was wrong with it" half of FR-059's message.
        assert_eq!(
            ColourError.to_string(),
            "expected #rgb, #rrggbb or #rrggbbaa"
        );
    }

    #[test]
    fn hex_round_trips_a_parsed_colour() {
        for text in ["#000000", "#ffffff", "#336bb8", "#a8a8b3"] {
            assert_eq!(
                Colour::parse(text).expect("a valid colour").hex(),
                text,
                "{text} should survive a round trip"
            );
        }
    }

    // --- T010: geometry clamping (FR-054, FR-059) --------------------------

    #[test]
    fn every_geometry_value_is_clamped_at_both_ends_and_left_alone_between_them() {
        for setting in GEOMETRY {
            let below = setting.min - 1.0;
            let (used, reported) = setting.clamp(below);
            assert!(close(used, setting.min), "{} below min", setting.key);
            let reported =
                reported.unwrap_or_else(|| panic!("{} below min is reported", setting.key));
            assert_eq!(reported.subject, format!("config.style.{}", setting.key));
            assert!(
                reported
                    .message
                    .contains(&format!("using {}", setting.show(setting.min))),
                "{} names the value actually used: {}",
                setting.key,
                reported.message
            );

            let above = setting.max + 1.0;
            let (used, reported) = setting.clamp(above);
            assert!(close(used, setting.max), "{} above max", setting.key);
            let reported =
                reported.unwrap_or_else(|| panic!("{} above max is reported", setting.key));
            assert!(
                reported
                    .message
                    .contains(&format!("using {}", setting.show(setting.max))),
                "{} names the value actually used: {}",
                setting.key,
                reported.message
            );

            // In range: applied as written, silently.
            let middle = f64::midpoint(setting.min, setting.max);
            let (used, reported) = setting.clamp(middle);
            assert!(close(used, middle), "{} in range", setting.key);
            assert_eq!(reported, None, "{} in range is silent", setting.key);

            // The bounds themselves are inside the range, not outside it.
            assert_eq!(setting.clamp(setting.min).1, None, "{} min", setting.key);
            assert_eq!(setting.clamp(setting.max).1, None, "{} max", setting.key);
        }
    }

    #[test]
    fn a_clamp_names_the_setting_and_reads_as_the_contract_documents() {
        // contracts/config.md's worked example.
        let cell_width = GEOMETRY
            .iter()
            .find(|setting| setting.key == "grid_cell_width")
            .expect("grid_cell_width is a setting");
        let (used, reported) = cell_width.clamp(0.0);
        assert!(close(used, 40.0));
        let reported = reported.expect("0 is out of range");
        assert_eq!(reported.subject, "config.style.grid_cell_width");
        assert_eq!(reported.message, "0 is below the minimum 40; using 40");
        assert_eq!(reported.condition, Condition::InvalidConfigValue);
    }

    #[test]
    fn a_clamped_value_is_never_the_default_it_would_have_fallen_back_to() {
        // FR-054: out of range is brought within range, not rejected. `height_fraction = 5.0` has
        // to become the maximum, not the 0.8 default — "as large as possible" is what was meant.
        let cap = GEOMETRY
            .iter()
            .find(|setting| setting.key == "height_fraction")
            .expect("height_fraction is a setting");
        let (used, _) = cap.clamp(5.0);
        assert!(close(used, 1.0));
        assert!(!close(used, Geometry::default().height_fraction));
    }

    #[test]
    fn a_value_that_is_not_a_number_at_all_cannot_reach_the_layout() {
        // `NaN` compares false against both bounds, so it would slip through a naive clamp and
        // then propagate through every multiplication in ui/layout.rs.
        for setting in GEOMETRY {
            let (used, reported) = setting.clamp(f64::NAN);
            assert!(used.is_finite(), "{} must not stay NaN", setting.key);
            assert!(reported.is_some(), "{} reports it", setting.key);
        }
    }

    #[test]
    fn whole_unit_settings_are_spelled_without_a_decimal_point() {
        // A message reading "using 40" rather than "using 40.0" is the difference between one a
        // user can copy back into their file and one they cannot.
        for setting in GEOMETRY {
            for probe in [
                setting.min,
                setting.max,
                f64::midpoint(setting.min, setting.max),
            ] {
                let shown = setting.show(probe);
                assert!(
                    !setting.integral || !shown.contains('.'),
                    "{} is a whole-unit setting but spells {probe} as {shown}",
                    setting.key
                );
            }
        }
        // And a fractional setting keeps its fraction rather than being rounded into a message
        // that contradicts the value actually used.
        let corner = GEOMETRY
            .iter()
            .find(|setting| setting.key == "corner_radius")
            .expect("corner_radius is a setting");
        assert_eq!(corner.show(0.28), "0.28");
    }

    // --- T013: the FR-050 precedence chain ---------------------------------

    /// One `[style]` override, spelled as the file would hold it.
    fn text(key: &str, value: &str) -> (String, Value) {
        (key.to_owned(), Value::Text(value.to_owned()))
    }

    fn number(key: &str, value: f64) -> (String, Value) {
        (key.to_owned(), Value::Float(value))
    }

    #[test]
    fn with_nothing_written_every_value_is_the_default() {
        let (style, diagnostics) = resolve(&Requested::default());
        assert_eq!(style, Style::default());
        assert_eq!(style.palette, DARK);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_named_theme_supplies_the_palette_and_nothing_else() {
        let (style, diagnostics) = resolve(&Requested {
            theme: Some("dark".to_owned()),
            overrides: Vec::new(),
        });
        assert_eq!(style.palette, DARK);
        assert_eq!(style.geometry, Geometry::default());
        assert_eq!(style.font_family, DEFAULT_FONT_FAMILY);
        assert!(close(style.text_size, DEFAULT_TEXT_SIZE));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_override_beats_the_theme_it_sits_on() {
        let (style, diagnostics) = resolve(&Requested {
            theme: Some("dark".to_owned()),
            overrides: vec![text("highlight", "#123456")],
        });
        assert_eq!(
            style.palette.highlight,
            Colour::parse("#123456").expect("a valid colour")
        );
        // Every other colour still comes from the theme.
        assert_eq!(style.palette.backdrop, DARK.backdrop);
        assert_eq!(style.palette.text, DARK.text);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn overrides_without_a_theme_apply_over_the_default_theme() {
        // US4-AS2.
        let (style, diagnostics) = resolve(&Requested {
            theme: None,
            overrides: vec![text("text", "#010203")],
        });
        assert_eq!(
            style.palette.text,
            Colour::parse("#010203").expect("a valid colour")
        );
        assert_eq!(style.palette.backdrop, DARK.backdrop);
        assert_eq!(style.palette.name, DARK.name);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_invalid_override_falls_back_alone_and_leaves_every_other_value_intact() {
        // SC-022: one bad value must not discard the rest.
        let (style, diagnostics) = resolve(&Requested {
            theme: Some("dark".to_owned()),
            overrides: vec![
                text("highlight", "not-a-colour"),
                text("text", "#010203"),
                number("row_padding", 4.0),
                text("font_family", "JetBrains Mono"),
            ],
        });

        assert_eq!(
            style.palette.highlight, DARK.highlight,
            "the bad colour fell back to the theme's own value"
        );
        assert_eq!(
            style.palette.text,
            Colour::parse("#010203").expect("a valid colour"),
            "the good colour still applied"
        );
        assert_eq!(style.geometry.row_padding, 4);
        assert_eq!(style.font_family, "JetBrains Mono");

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let reported = &diagnostics[0];
        assert_eq!(reported.subject, "config.style.highlight");
        assert!(
            reported
                .message
                .contains("expected #rgb, #rrggbb or #rrggbbaa"),
            "{}",
            reported.message
        );
        assert!(
            reported.message.contains(r#"got "not-a-colour""#),
            "names what was wrong: {}",
            reported.message
        );
        assert!(
            reported
                .message
                .contains(&format!("using {}", DARK.highlight.hex())),
            "names the value used: {}",
            reported.message
        );
    }

    #[test]
    fn an_unknown_theme_name_is_reported_and_falls_back_with_everything_else_applied() {
        // FR-058.
        let (style, diagnostics) = resolve(&Requested {
            theme: Some("dracula".to_owned()),
            overrides: vec![number("row_padding", 4.0)],
        });
        assert_eq!(style.palette, DARK);
        assert_eq!(style.geometry.row_padding, 4, "the other setting applied");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].subject, "config.theme");
        assert!(
            diagnostics[0]
                .message
                .contains(r#"unknown theme "dracula""#),
            "{}",
            diagnostics[0].message
        );
        assert!(
            diagnostics[0].message.contains(r#"using "dark""#),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_is_reported_against_its_own_key() {
        let (style, diagnostics) = resolve(&Requested {
            theme: None,
            overrides: vec![
                ("highlight".to_owned(), Value::Integer(7)),
                ("row_padding".to_owned(), Value::Text("wide".to_owned())),
                ("font_family".to_owned(), Value::Other("a table")),
            ],
        });
        assert_eq!(style, Style::default(), "nothing was applied");
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.subject.as_str())
                .collect::<Vec<_>>(),
            vec![
                "config.style.highlight",
                "config.style.row_padding",
                "config.style.font_family"
            ]
        );
        assert!(
            diagnostics[0]
                .message
                .contains("expected a string, found an integer")
        );
        assert!(
            diagnostics[1]
                .message
                .contains("expected a number, found a string")
        );
        assert!(
            diagnostics[2]
                .message
                .contains("expected a string, found a table")
        );
    }

    #[test]
    fn an_integer_is_as_good_as_a_float_for_a_fractional_setting() {
        // `width_fraction = 1` and `width_fraction = 1.0` are the same value to a user.
        let (style, diagnostics) = resolve(&Requested {
            theme: None,
            overrides: vec![("width_fraction".to_owned(), Value::Integer(1))],
        });
        assert!(close(style.geometry.width_fraction, 1.0));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_unknown_style_key_is_reported_and_ignored() {
        let (style, diagnostics) = resolve(&Requested {
            theme: None,
            overrides: vec![text("shadow", "#000000")],
        });
        assert_eq!(style, Style::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].subject, "config.style.shadow");
        assert_eq!(diagnostics[0].condition, Condition::UnknownConfigKey);
        assert!(
            !diagnostics[0].condition.notifies(),
            "an ignored key needs no interruption"
        );
    }

    #[test]
    fn the_last_override_of_a_key_wins() {
        // TOML forbids a duplicate key, so this can only arise from a caller assembling the list;
        // last-wins is the only order that is not surprising.
        let (style, _) = resolve(&Requested {
            theme: None,
            overrides: vec![text("text", "#010203"), text("text", "#040506")],
        });
        assert_eq!(
            style.palette.text,
            Colour::parse("#040506").expect("a valid colour")
        );
    }

    #[test]
    fn a_theme_carries_no_font_and_no_geometry() {
        // SC-023, structurally: `Theme` has eleven colour fields and a name, so selecting one
        // cannot move the layout however many themes are added later (FR-049).
        for theme in BUILT_IN {
            let (style, _) = resolve(&Requested {
                theme: Some(theme.name.to_owned()),
                overrides: Vec::new(),
            });
            assert_eq!(style.geometry, Geometry::default(), "{}", theme.name);
            assert_eq!(style.font_family, DEFAULT_FONT_FAMILY, "{}", theme.name);
            assert!(close(style.text_size, DEFAULT_TEXT_SIZE), "{}", theme.name);
        }
    }

    // --- T057: what a built-in theme is, and is not -------------------------

    #[test]
    fn every_built_in_theme_is_selectable_by_its_name_without_complaint() {
        // FR-049: the set is "documented" and "selectable by name", so a theme that exists in the
        // slice but cannot be reached by the name it carries is a theme a user cannot have.
        for theme in BUILT_IN {
            let (style, diagnostics) = resolve(&Requested {
                theme: Some(theme.name.to_owned()),
                overrides: Vec::new(),
            });
            assert_eq!(
                style.palette, *theme,
                "{} did not resolve to itself",
                theme.name
            );
            assert!(
                diagnostics.is_empty(),
                "{} reported {diagnostics:?}",
                theme.name
            );
        }
    }

    #[test]
    fn built_in_theme_names_are_unique() {
        // `named_theme` takes the first match, so a duplicated name would make one theme
        // unreachable and the shadowing silent.
        let mut names: Vec<&str> = BUILT_IN.iter().map(|theme| theme.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate built-in theme name");
        assert!(
            names.contains(&"dark") && names.contains(&"light"),
            "FR-049 requires at least a dark and a light theme, found {names:?}"
        );
    }

    #[test]
    fn every_built_in_theme_defines_all_eleven_colours_and_nothing_else() {
        // FR-049, SC-023: a theme is a palette. `resolve` already proves it carries no font and
        // no geometry (below); this proves the palette half — every one of the eleven is a real
        // colour in every theme, so none was left at a placeholder.
        assert_eq!(COLOURS.len(), 11, "FR-045 names eleven colours");
        for theme in BUILT_IN {
            for setting in COLOURS {
                for channel in channels(setting.read(theme)) {
                    assert!(
                        (0.0..=1.0).contains(&channel),
                        "{}.{} has an out-of-range channel {channel}",
                        theme.name,
                        setting.key
                    );
                }
            }
        }
    }

    #[test]
    fn every_colour_setting_reaches_a_field_of_its_own() {
        // Two catalogue rows pointing at one field would make an override silently set the wrong
        // element — the kind of copy-and-paste slip the macro above invites. Writing a distinct
        // value through every key and reading them all back catches it.
        let mut palette = DARK;
        for (index, setting) in COLOURS.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let unique = Colour::new(index as f64 / 255.0, 0.0, 0.0, 1.0);
            setting.write(&mut palette, unique);
        }
        let mut written: Vec<String> = COLOURS
            .iter()
            .map(|setting| setting.read(&palette).hex_rgba())
            .collect();
        let total = written.len();
        written.sort();
        written.dedup();
        assert_eq!(written.len(), total, "two colour keys share one field");
    }

    #[test]
    fn the_light_theme_is_a_light_one() {
        // The one thing "a light theme" has to mean, asserted rather than assumed: its backdrop
        // is light and its primary text is dark, which is the reverse of the default (FR-049).
        let luminance = |colour: Colour| {
            0.2126f64.mul_add(
                colour.red,
                0.7152f64.mul_add(colour.green, 0.0722 * colour.blue),
            )
        };
        assert!(
            luminance(LIGHT.backdrop) > 0.5,
            "the light backdrop is not light"
        );
        assert!(
            luminance(LIGHT.text) < 0.5,
            "the light theme's text is not dark"
        );
        assert!(
            luminance(DARK.backdrop) < 0.5 && luminance(DARK.text) > 0.5,
            "the dark theme stopped being the dark one"
        );
    }

    #[test]
    fn every_catalogued_key_is_distinct() {
        // A key claimed by two catalogues would make `apply_override`'s first-match order the
        // silent arbiter of which one a user gets.
        let mut keys: Vec<&str> = COLOURS
            .iter()
            .map(|setting| setting.key)
            .chain(GEOMETRY.iter().map(|setting| setting.key))
            .chain([TEXT_SIZE.key, FONT_FAMILY_KEY])
            .collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "duplicate style key");
        assert_eq!(total, 11 + 10 + 2);
    }

    // --- T020: the default appearance is the pre-feature one ---------------

    /// The committed pre-feature baseline, which is the authority on what the overlay looked like
    /// before this feature existed (`tests/fixtures/baseline/README.md`).
    fn baseline() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("baseline")
            .join("style.json");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        serde_json::from_str(&source).expect("the baseline is valid JSON")
    }

    #[test]
    fn the_dark_theme_is_byte_for_byte_the_pre_feature_palette() {
        // FR-049a, SC-018. Compared against the recorded floats rather than the hex, because two
        // channels land on an 8-bit half-step and only the floats have no tie to break
        // (contracts/style-values.md).
        let baseline = baseline();
        let palette = baseline["palette"]
            .as_object()
            .expect("the baseline records a palette");
        assert_eq!(palette.len(), COLOURS.len(), "eleven colours, no more");

        for setting in COLOURS {
            let recorded = palette
                .get(setting.key)
                .unwrap_or_else(|| panic!("the baseline records {}", setting.key));
            let rgba = recorded["rgba"]
                .as_array()
                .expect("rgba is an array")
                .iter()
                .map(|value| value.as_f64().expect("a channel is a number"))
                .collect::<Vec<_>>();
            assert_eq!(
                channels(setting.read(&DARK)).to_vec(),
                rgba,
                "dark.{} has drifted from the pre-feature renderer",
                setting.key
            );
        }
    }

    #[test]
    fn the_default_geometry_is_byte_for_byte_the_pre_feature_geometry() {
        // FR-049a: the same guard for ui/layout.rs's former constants.
        let baseline = baseline();
        let recorded = baseline["geometry"]
            .as_object()
            .expect("the baseline records the geometry");
        assert_eq!(recorded.len(), GEOMETRY.len(), "ten values, no more");

        let defaults = Geometry::default();
        for setting in GEOMETRY {
            let value = recorded
                .get(setting.key)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_else(|| panic!("the baseline records {}", setting.key));
            assert!(
                close(setting.read(&defaults), value),
                "{} has drifted: {} vs the recorded {value}",
                setting.key,
                setting.read(&defaults)
            );
        }

        // The derived label height, and the font values the renderer used.
        assert_eq!(
            u64::from(defaults.grid_label_height()),
            baseline["derived"]["grid_label_height"]
                .as_u64()
                .expect("the baseline records the label height")
        );
        assert_eq!(
            DEFAULT_FONT_FAMILY,
            baseline["font_family"]
                .as_str()
                .expect("the baseline records the font family")
        );
        assert!(close(
            DEFAULT_TEXT_SIZE,
            baseline["render_scalars"]["text_size"]
                .as_f64()
                .expect("the baseline records the text size")
        ));
    }
}
