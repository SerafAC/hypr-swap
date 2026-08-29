//! The freedesktop icon-set lookup: which file on disk is a given icon name at a given size.
//!
//! Implemented directly rather than taken as a dependency (research.md R20): the search path, the
//! `index.theme` directory list with each directory's `Size`, `Scale`, `Type`, `MinSize`,
//! `MaxSize` and `Threshold`, and `Inherits` followed in order until the standard default set.
//!
//! Directory choice for a requested size is a pure function over that parsed metadata, so the
//! scoring rule is unit-testable on its own (FR-040).
//!
//! Note the vocabulary, which the spec keeps distinct: this is the *icon set* (FR-057), not the
//! overlay theme in [`crate::theme`]. The two are independent settings.
