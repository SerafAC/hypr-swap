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

    // ---------------------------------------------------------------------------------------
    // The gating half of FR-093's bounded acceptance (research.md R38).
    //
    // cargo-deny has no built-in expiry for an accepted advisory, so the time bound lives in the
    // `reason` string it does support and is enforced here — in `cargo test --lib`, where a
    // contributor meets it. `advisories` itself does not gate; this does. The rule and the date
    // arithmetic are pure functions so that the walk over `deny.toml` is the only part that
    // touches a file.
    // ---------------------------------------------------------------------------------------

    /// A calendar date, compared as a tuple so that ordering is lexicographic and correct.
    type Date = (i64, i64, i64);

    /// The expiry an acceptance's `reason` declares: it must begin `until YYYY-MM-DD:`, and what
    /// follows the colon is the human explanation, which this does not judge.
    ///
    /// The error is the message the failing test prints, so it is written for whoever has to fix
    /// `deny.toml` rather than for whoever wrote this.
    fn acceptance_expiry(reason: &str) -> Result<Date, String> {
        let Some(rest) = reason.strip_prefix("until ") else {
            return Err(format!("reason does not begin `until `: {reason:?}"));
        };
        let Some((date, _why)) = rest.split_once(':') else {
            return Err(format!("reason has no `:` after the date: {reason:?}"));
        };
        let parts: Vec<&str> = date.split('-').collect();
        let [year, month, day] = parts.as_slice() else {
            return Err(format!("`{date}` is not YYYY-MM-DD"));
        };
        if (year.len(), month.len(), day.len()) != (4, 2, 2) {
            return Err(format!("`{date}` is not YYYY-MM-DD"));
        }
        let number = |field: &str| {
            field
                .parse::<i64>()
                .map_err(|_| format!("`{date}` is not YYYY-MM-DD"))
        };
        let (year, month, day) = (number(year)?, number(month)?, number(day)?);
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return Err(format!("`{date}` is not a real date"));
        }
        Ok((year, month, day))
    }

    /// The civil date for a count of days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
    ///
    /// Here rather than from a dependency because the whole need is "what is today", and a date
    /// library for one comparison would not survive the constitution's Complexity Tracking table.
    fn civil_from_days(days: i64) -> Date {
        let shifted = days + 719_468;
        let era = if shifted >= 0 {
            shifted
        } else {
            shifted - 146_096
        } / 146_097;
        let day_of_era = shifted - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        (year + i64::from(month <= 2), month, day)
    }

    /// Today, in UTC. The one impure input, and the reason this is a test rather than a `const`.
    fn today() -> Date {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_secs();
        civil_from_days(i64::try_from(seconds / 86_400).expect("a plausible clock"))
    }

    #[test]
    fn the_expiry_form_is_exactly_what_deny_toml_documents() {
        assert_eq!(
            acceptance_expiry("until 2026-12-31: no fixed release exists upstream"),
            Ok((2026, 12, 31)),
        );
        // Everything else is a malformed acceptance, and each is a way someone would get it wrong.
        for wrong in [
            "no fixed release exists upstream",
            "until 2026-12-31 no fixed release exists upstream",
            "until 26-12-31: short year",
            "until 2026-1-1: unpadded",
            "until 2026/12/31: wrong separator",
            "until 2026-13-01: no such month",
            "until never: not a date",
            "Until 2026-12-31: wrong case",
        ] {
            assert!(
                acceptance_expiry(wrong).is_err(),
                "{wrong:?} should not be an acceptable reason",
            );
        }
    }

    #[test]
    fn the_calendar_arithmetic_is_right() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        // A leap day, and the day after it, so an off-by-one in February is caught.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_783), (2024, 3, 1));
        assert!(today() >= (2026, 1, 1), "today is {:?}", today());
    }

    #[test]
    fn advisory_acceptances_are_bounded_and_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("deny.toml");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document: toml::Table = source
            .parse()
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));

        let ignored = document
            .get("advisories")
            .and_then(|advisories| advisories.get("ignore"))
            .and_then(toml::Value::as_array)
            .unwrap_or_else(|| panic!("deny.toml has no [advisories] ignore list"));

        let today = today();
        for entry in ignored {
            // A bare string is cargo-deny's other accepted form, and it carries no reason at all,
            // so it cannot be bounded — which is the whole point (research.md R38).
            let table = entry.as_table().unwrap_or_else(|| {
                panic!(
                    "deny.toml: {entry} is a bare advisory id. Write it as \
                     {{ id = \"…\", reason = \"until YYYY-MM-DD: …\" }} so the acceptance expires."
                )
            });
            let id = table
                .get("id")
                .and_then(toml::Value::as_str)
                .unwrap_or("<no id>");
            let reason = table
                .get("reason")
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("deny.toml: the acceptance of {id} carries no reason"));

            let expiry = acceptance_expiry(reason).unwrap_or_else(|complaint| {
                panic!(
                    "deny.toml: the acceptance of {id} is not bounded — {complaint}. \
                     It must read `until YYYY-MM-DD: <why>` (FR-093)."
                )
            });
            assert!(
                expiry >= today,
                "deny.toml: the acceptance of {id} expired on {expiry:?} and today is {today:?}. \
                 Fix the advisory, or re-accept it with a new date and a reason that is still true \
                 (FR-093).",
            );
        }
    }

    #[test]
    fn the_supported_range_renders_as_the_documents_state_it() {
        // The form the README, the site's requirements page and the FR-118 diagnostic all carry.
        assert_eq!(SUPPORTED_HYPRLAND.to_string(), ">= 0.55");
        assert_eq!(SUPPORTED_HYPRLAND.minimum, (0, 55));
    }
}
