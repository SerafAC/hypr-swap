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
