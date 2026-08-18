//! Configuration: location, schema, defaults, and per-setting validation
//! (`contracts/config.md`, FR-023, FR-024).
//!
//! Read once at start-up; live reload is out of scope. The defaults documented by FR-023 live
//! here and nowhere else. Validation is deliberately per setting: one typo must not silently
//! reset the user's other choices.

use std::path::{Path, PathBuf};

use crate::diag::{self, Condition};

/// How workspaces are presented in the overlay (FR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Presentation {
    /// One row per workspace: its name followed by the titles of its windows.
    #[default]
    List,
    /// A miniature of each workspace's layout, its name underneath.
    Grid,
}

/// Where the overlay is shown (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// Only on the monitor holding the focused workspace.
    #[default]
    ActiveMonitor,
    /// On every connected monitor, all showing the same highlight.
    AllMonitors,
}

/// The order entries appear in (FR-008a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// Most recently active first; the highlight opens on the second entry.
    #[default]
    Mru,
    /// The compositor's stable order; the highlight opens on the active workspace.
    Compositor,
    /// Grouped by monitor, stable within each group; highlight on the active workspace.
    Monitor,
}

/// The three user settings. Key combinations are deliberately not here — they live in the
/// compositor's configuration (FR-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Configuration {
    pub presentation: Presentation,
    pub placement: Placement,
    pub order: Order,
}

/// One thing worth telling the user about the configuration file.
///
/// Parsing produces these rather than writing to stderr itself, so the whole schema — including
/// the exact subject each problem is reported under — is unit-testable without capturing output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub condition: Condition,
    pub subject: String,
    pub message: String,
}

/// Why a configuration file named explicitly with `--config` could not be used (FR-034).
#[derive(Debug)]
pub enum LoadError {
    NotFound(PathBuf),
    Unreadable(PathBuf, std::io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "{} does not exist", path.display()),
            Self::Unreadable(path, e) => write!(f, "{} could not be read: {e}", path.display()),
        }
    }
}

/// A setting's schema: its key, its accepted values, and its default.
///
/// Implemented once per setting so the "unknown value X, using default Y" path exists once.
trait Setting: Copy + Default + 'static {
    const KEY: &'static str;
    /// Accepted values, in the order they are listed to the user.
    const VALUES: &'static [(&'static str, Self)];

    fn name(self) -> &'static str
    where
        Self: PartialEq,
    {
        Self::VALUES
            .iter()
            .find(|(_, value)| *value == self)
            .map(|(name, _)| *name)
            .unwrap_or_default()
    }
}

impl Setting for Presentation {
    const KEY: &'static str = "presentation";
    const VALUES: &'static [(&'static str, Self)] = &[("list", Self::List), ("grid", Self::Grid)];
}

impl Setting for Placement {
    const KEY: &'static str = "placement";
    const VALUES: &'static [(&'static str, Self)] =
        &[("active", Self::ActiveMonitor), ("all", Self::AllMonitors)];
}

impl Setting for Order {
    const KEY: &'static str = "order";
    const VALUES: &'static [(&'static str, Self)] = &[
        ("mru", Self::Mru),
        ("compositor", Self::Compositor),
        ("monitor", Self::Monitor),
    ];
}

/// The default configuration file location: `$XDG_CONFIG_HOME/hypr-swap/config.toml`, falling
/// back to `~/.config/hypr-swap/config.toml`.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    resolve_default_path(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn resolve_default_path(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let base = match xdg_config_home {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(home?).join(".config"),
    };
    Some(base.join("hypr-swap").join("config.toml"))
}

/// Load the configuration, reporting every problem as it goes.
///
/// With no `--config`, a missing file at the default location is normal and silent (FR-023).
/// A file named explicitly and not found is an error instead (FR-034).
///
/// # Errors
/// [`LoadError`] only for a path named explicitly with `--config` that does not exist or cannot
/// be read. Every other problem is reported and recovered from, because FR-024 requires the
/// application to carry on.
pub fn load(explicit: Option<&Path>) -> Result<Configuration, LoadError> {
    match explicit {
        Some(path) => load_file(path, true),
        None => match default_path() {
            Some(path) => load_file(&path, false),
            None => Ok(Configuration::default()),
        },
    }
}

/// Load one file. `required` is what separates FR-034's explicit path, where absence is an error,
/// from FR-023's default location, where absence is the normal case.
fn load_file(path: &Path, required: bool) -> Result<Configuration, LoadError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
            return Ok(Configuration::default());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(LoadError::NotFound(path.to_path_buf()));
        }
        Err(e) if required => return Err(LoadError::Unreadable(path.to_path_buf(), e)),
        Err(e) => {
            diag::report(
                Condition::InvalidConfigValue,
                "config",
                &format!("{} could not be read: {e}, using defaults", path.display()),
            );
            return Ok(Configuration::default());
        }
    };

    let (configuration, diagnostics) = parse(&source);
    for d in diagnostics {
        diag::report(d.condition, &d.subject, &d.message);
    }
    Ok(configuration)
}

/// Parse configuration text into settings plus whatever should be reported about it.
///
/// Never fails: an unusable file yields the documented defaults and a diagnostic, because
/// FR-024 requires the application to carry on.
#[must_use]
pub fn parse(source: &str) -> (Configuration, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    let table: toml::Table = match source.parse() {
        Ok(table) => table,
        Err(e) => {
            // A file that is not valid TOML cannot be attributed to one setting: report the parse
            // error with its position and fall back to every default.
            let position = e
                .span()
                .map(|span| format!(" at byte {}", span.start))
                .unwrap_or_default();
            diagnostics.push(Diagnostic {
                condition: Condition::InvalidConfigValue,
                subject: "config".to_owned(),
                message: format!(
                    "not valid TOML{position}: {}, using defaults for every setting",
                    e.message()
                ),
            });
            return (Configuration::default(), diagnostics);
        }
    };

    let configuration = Configuration {
        presentation: read(&table, &mut diagnostics),
        placement: read(&table, &mut diagnostics),
        order: read(&table, &mut diagnostics),
    };

    for key in table.keys() {
        if !matches!(
            key.as_str(),
            Presentation::KEY | Placement::KEY | Order::KEY
        ) {
            diagnostics.push(Diagnostic {
                condition: Condition::UnknownConfigKey,
                subject: format!("config.{key}"),
                message: "unknown key, ignored".to_owned(),
            });
        }
    }

    (configuration, diagnostics)
}

/// Read one setting, falling back to its own default and reporting it by name when the value is
/// missing a string or naming something that does not exist (FR-024).
fn read<T: Setting + PartialEq>(table: &toml::Table, diagnostics: &mut Vec<Diagnostic>) -> T {
    let default = T::default();
    let Some(value) = table.get(T::KEY) else {
        return default;
    };

    let problem = match value.as_str() {
        Some(raw) => match T::VALUES.iter().find(|(name, _)| *name == raw) {
            Some((_, parsed)) => return *parsed,
            None => format!("unknown value {raw:?}"),
        },
        None => format!("expected a string, found {}", value.type_str()),
    };

    let accepted = T::VALUES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic {
        condition: Condition::InvalidConfigValue,
        subject: format!("config.{}", T::KEY),
        message: format!(
            "{problem}, using default {:?} (accepted: {accepted})",
            default.name()
        ),
    });
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subjects(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.subject.as_str()).collect()
    }

    #[test]
    fn documented_defaults_are_list_active_monitor_and_mru() {
        // FR-023: flat list presentation, active monitor only, MRU order.
        let defaults = Configuration::default();
        assert_eq!(defaults.presentation, Presentation::List);
        assert_eq!(defaults.placement, Placement::ActiveMonitor);
        assert_eq!(defaults.order, Order::Mru);
    }

    #[test]
    fn an_empty_file_yields_defaults_with_no_diagnostic() {
        let (configuration, diagnostics) = parse("");
        assert_eq!(configuration, Configuration::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_missing_file_is_silent_by_default_but_an_error_when_named_explicitly() {
        let missing = Path::new("/nonexistent/hypr-swap/definitely-not-here.toml");
        // FR-023: no file at the default location is a normal, undiagnosed configuration.
        assert_eq!(
            load_file(missing, false).expect("a missing default file is not an error"),
            Configuration::default()
        );
        // FR-034: the same file named with --config is an error rather than a silent fallback.
        assert!(matches!(
            load_file(missing, true),
            Err(LoadError::NotFound(_))
        ));
    }

    #[test]
    fn the_default_path_prefers_xdg_config_home_over_the_home_fallback() {
        use std::ffi::OsString;
        assert_eq!(
            resolve_default_path(
                Some(OsString::from("/x/cfg")),
                Some(OsString::from("/home/u"))
            ),
            Some(PathBuf::from("/x/cfg/hypr-swap/config.toml"))
        );
        assert_eq!(
            resolve_default_path(None, Some(OsString::from("/home/u"))),
            Some(PathBuf::from("/home/u/.config/hypr-swap/config.toml"))
        );
        assert_eq!(
            resolve_default_path(Some(OsString::new()), Some(OsString::from("/home/u"))),
            Some(PathBuf::from("/home/u/.config/hypr-swap/config.toml")),
            "an empty XDG_CONFIG_HOME is treated as unset"
        );
        assert_eq!(resolve_default_path(None, None), None);
    }

    #[test]
    fn a_file_that_exists_is_read_from_disk() {
        let path = std::env::temp_dir().join("hypr-swap-config-load-test.toml");
        std::fs::write(&path, "presentation = \"grid\"\n").expect("write temp config");
        let configuration = load_file(&path, true).expect("an existing file loads");
        assert_eq!(configuration.presentation, Presentation::Grid);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_valid_value_is_accepted() {
        for (raw, expected) in [("list", Presentation::List), ("grid", Presentation::Grid)] {
            let (configuration, diagnostics) = parse(&format!("presentation = {raw:?}"));
            assert_eq!(configuration.presentation, expected);
            assert!(diagnostics.is_empty(), "{raw} should be accepted silently");
        }
        for (raw, expected) in [
            ("active", Placement::ActiveMonitor),
            ("all", Placement::AllMonitors),
        ] {
            let (configuration, diagnostics) = parse(&format!("placement = {raw:?}"));
            assert_eq!(configuration.placement, expected);
            assert!(diagnostics.is_empty());
        }
        for (raw, expected) in [
            ("mru", Order::Mru),
            ("compositor", Order::Compositor),
            ("monitor", Order::Monitor),
        ] {
            let (configuration, diagnostics) = parse(&format!("order = {raw:?}"));
            assert_eq!(configuration.order, expected);
            assert!(diagnostics.is_empty());
        }
    }

    #[test]
    fn all_three_settings_can_be_set_at_once() {
        let (configuration, diagnostics) = parse(
            r#"
            presentation = "grid"
            placement = "all"
            order = "monitor"
            "#,
        );
        assert_eq!(
            configuration,
            Configuration {
                presentation: Presentation::Grid,
                placement: Placement::AllMonitors,
                order: Order::Monitor,
            }
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn one_invalid_value_leaves_the_others_honoured() {
        // contracts/config.md worked example: presentation falls back, order is kept.
        let (configuration, diagnostics) = parse(
            r#"
            presentation = "tiles"
            order = "compositor"
            "#,
        );
        assert_eq!(
            configuration.presentation,
            Presentation::List,
            "fell back to its own default"
        );
        assert_eq!(
            configuration.order,
            Order::Compositor,
            "user's other choice survived"
        );
        assert_eq!(subjects(&diagnostics), vec!["config.presentation"]);
    }

    #[test]
    fn an_invalid_value_names_the_offending_setting_and_the_default_it_used() {
        let (_, diagnostics) = parse(r#"presentation = "tiles""#);
        let d = &diagnostics[0];
        assert_eq!(d.subject, "config.presentation");
        assert!(
            d.message.contains(r#"unknown value "tiles""#),
            "{}",
            d.message
        );
        assert!(
            d.message.contains(r#"using default "list""#),
            "{}",
            d.message
        );
        assert!(
            d.message.contains("list, grid"),
            "lists the accepted values: {}",
            d.message
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_falls_back_like_any_other_invalid_value() {
        let (configuration, diagnostics) = parse("order = 3");
        assert_eq!(configuration.order, Order::Mru);
        assert_eq!(subjects(&diagnostics), vec!["config.order"]);
        assert!(
            diagnostics[0].message.contains("expected a string"),
            "{:?}",
            diagnostics[0]
        );
    }

    #[test]
    fn an_unknown_key_is_reported_and_ignored() {
        let (configuration, diagnostics) = parse(
            r#"
            order = "monitor"
            theme = "dracula"
            "#,
        );
        assert_eq!(
            configuration.order,
            Order::Monitor,
            "the valid keys still apply"
        );
        assert_eq!(subjects(&diagnostics), vec!["config.theme"]);
        assert_eq!(diagnostics[0].condition, Condition::UnknownConfigKey);
    }

    #[test]
    fn invalid_toml_falls_back_to_all_defaults() {
        let (configuration, diagnostics) = parse("presentation = \nthis is not toml");
        assert_eq!(configuration, Configuration::default());
        assert_eq!(subjects(&diagnostics), vec!["config"]);
        assert!(
            diagnostics[0].message.contains("not valid TOML"),
            "{:?}",
            diagnostics[0]
        );
        assert!(
            diagnostics[0]
                .message
                .contains("using defaults for every setting"),
            "{:?}",
            diagnostics[0]
        );
    }

    // T083 — the exact stderr subjects and notify flags, matching contracts/diagnostics.md.

    #[test]
    fn each_settings_fallback_reports_under_its_own_subject() {
        let (_, diagnostics) = parse(
            r#"
            presentation = "tiles"
            placement = "everywhere"
            order = "alphabetical"
            "#,
        );
        assert_eq!(
            subjects(&diagnostics),
            vec!["config.presentation", "config.placement", "config.order"]
        );
    }

    #[test]
    fn invalid_values_notify_and_unknown_keys_do_not() {
        let (_, diagnostics) = parse(
            r#"
            presentation = "tiles"
            wallpaper = "nope"
            "#,
        );
        let invalid = &diagnostics[0];
        assert_eq!(invalid.condition, Condition::InvalidConfigValue);
        assert!(
            invalid.condition.notifies(),
            "FR-024/FR-030: the user must fix this"
        );
        assert_eq!(invalid.condition.level(), diag::Level::Warn);

        let unknown = &diagnostics[1];
        assert_eq!(unknown.condition, Condition::UnknownConfigKey);
        assert!(
            !unknown.condition.notifies(),
            "an ignored key needs no interruption"
        );
        assert_eq!(unknown.condition.level(), diag::Level::Warn);
    }

    #[test]
    fn a_whole_file_parse_error_notifies_once_under_the_config_subject() {
        let (_, diagnostics) = parse("=== nonsense ===");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].subject, "config");
        assert!(diagnostics[0].condition.notifies());
    }

    #[test]
    fn setting_names_round_trip_for_use_in_messages() {
        assert_eq!(Presentation::List.name(), "list");
        assert_eq!(Presentation::Grid.name(), "grid");
        assert_eq!(Placement::ActiveMonitor.name(), "active");
        assert_eq!(Placement::AllMonitors.name(), "all");
        assert_eq!(Order::Mru.name(), "mru");
        assert_eq!(Order::Compositor.name(), "compositor");
        assert_eq!(Order::Monitor.name(), "monitor");
    }
}
