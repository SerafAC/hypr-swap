//! The desktop-entry index, and the rule that decides which entry owns a window class.
//!
//! Only four keys are read per entry — `Icon`, `StartupWMClass`, `Name` and `NoDisplay` — so this
//! is a minimal INI reader rather than a dependency (research.md R21). The matching ladder is the
//! five ordered steps in `contracts/icon-lookup.md`, expressed as a pure function so it is
//! testable without a filesystem (FR-040).
