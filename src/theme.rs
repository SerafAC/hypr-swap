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
