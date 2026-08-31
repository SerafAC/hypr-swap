//! Alt-Tab style workspace switcher with cross-monitor swapping for Hyprland.
//!
//! The modules below are the seam that matters for testing: `config`, `model`, `state`,
//! `ordering`, `actions`, `session`, `theme` and `ui::layout` are I/O-free and unit-tested
//! directly; `hypr::ipc`, `hypr::events` and `icons::*` do I/O but keep their decision rules
//! separable and are unit-tested too — the icon modules against a fixture root rather than
//! whatever the developer has installed. Only `main.rs` and `ui::{mod, shortcuts, render}` are the
//! thin Wayland/cairo shell, covered by the nested-compositor E2E suite instead (plan.md →
//! Complexity Tracking).

use std::sync::LazyLock;

pub mod actions;
pub mod config;
pub mod diag;
pub mod hypr;
pub mod icons;
pub mod model;
pub mod ordering;
pub mod session;
pub mod state;
pub mod theme;
pub mod ui;

/// The application's own name, used as the global-shortcut `app_id`, the layer-shell namespace,
/// and the notification application name.
pub const APP_ID: &str = "hypr-swap";

/// The version the program reports, everywhere it reports one: `--version`, the usage text and
/// the start record all read this, so the three cannot disagree (Principle III,
/// [contracts/cli.md]).
///
/// `CARGO_PKG_VERSION` alone for a build made from the release tag, and
/// `CARGO_PKG_VERSION+<git describe>` for anything else, so a bug report identifies the exact
/// source it was built from (FR-103, FR-104).
///
/// [contracts/cli.md]: ../../specs/003-oss-release-readiness/contracts/cli.md
#[must_use]
pub fn version() -> &'static str {
    static VERSION: LazyLock<String> = LazyLock::new(|| {
        compose_version(
            env!("CARGO_PKG_VERSION"),
            option_env!("HYPR_SWAP_GIT_DESCRIBE"),
        )
    });
    &VERSION
}

/// Compose that string from the package version and whatever `build.rs` got out of git.
///
/// A function rather than a `const` because it is the whole of FR-104's decision and a test can
/// only ever observe the one form the test binary was itself built as. Three inputs mean "no
/// suffix": no git (`None`), git with nothing to say (`Some("")`), and a describe that is exactly
/// the release tag for this version — the four forms of [contracts/cli.md].
///
/// [contracts/cli.md]: ../../specs/003-oss-release-readiness/contracts/cli.md
#[must_use]
pub fn compose_version(package: &str, describe: Option<&str>) -> String {
    let describe = describe.unwrap_or_default();
    let built_from_the_release_tag = describe.strip_prefix('v') == Some(package);
    if describe.is_empty() || built_from_the_release_tag {
        package.to_owned()
    } else {
        format!("{package}+{describe}")
    }
}

/// The compositor versions this release supports, and the only definition of them: the README's
/// requirements section, the site's requirements page and the FR-118 diagnostic all state this
/// one value rather than repeating a number (data-model.md → Supported version range).
pub const SUPPORTED_HYPRLAND: SupportedRange = SupportedRange { minimum: (0, 55) };

/// A supported compositor range: a minimum `MAJOR.MINOR`, open above.
///
/// There is no maximum field because there is no maximum — a newer compositor is assumed to work
/// until it does not (Principle II). Its [`Display`] is the exact wording the README, the site and
/// the diagnostic use, so the rendered range has one source too.
///
/// [`Display`]: std::fmt::Display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedRange {
    pub minimum: (u32, u32),
}

impl std::fmt::Display for SupportedRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (major, minor) = self.minimum;
        write!(f, ">= {major}.{minor}")
    }
}

#[cfg(test)]
mod tests {
    use super::{SUPPORTED_HYPRLAND, compose_version};

    #[test]
    fn a_describe_of_exactly_the_release_tag_carries_no_suffix() {
        // The build FR-103 is about: the tag, the reported version and the changelog entry all
        // say `1.0.0`, so the describe adds nothing.
        assert_eq!(compose_version("1.0.0", Some("v1.0.0")), "1.0.0");
    }

    #[test]
    fn any_other_describe_becomes_the_suffix() {
        // The three remaining forms of contracts/cli.md, verbatim (FR-104).
        for describe in [
            "v1.0.0-14-gabc1234",
            "v1.0.0-14-gabc1234-dirty",
            "v1.0.0-dirty",
        ] {
            assert_eq!(
                compose_version("1.0.0", Some(describe)),
                format!("1.0.0+{describe}"),
            );
        }
    }

    #[test]
    fn a_tag_for_a_different_version_is_a_suffix_not_a_release() {
        // `v0.9.0` is a prefix match on nothing: only the tag for *this* package version is the
        // release build, or a version bump would silently claim to be a release.
        assert_eq!(compose_version("1.0.0", Some("v0.9.0")), "1.0.0+v0.9.0");
        assert_eq!(compose_version("1.0.0", Some("1.0.0")), "1.0.0+1.0.0");
    }

    #[test]
    fn no_git_falls_back_to_the_package_version() {
        // A source archive or a distribution build: `build.rs` emitted nothing, or git had
        // nothing to say.
        assert_eq!(compose_version("1.0.0", None), "1.0.0");
        assert_eq!(compose_version("1.0.0", Some("")), "1.0.0");
    }

    #[test]
    fn the_supported_range_renders_as_the_documents_state_it() {
        // The form the README, the site's requirements page and the FR-118 diagnostic all carry.
        assert_eq!(SUPPORTED_HYPRLAND.to_string(), ">= 0.55");
        assert_eq!(SUPPORTED_HYPRLAND.minimum, (0, 55));
    }
}
