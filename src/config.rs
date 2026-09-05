//! Configuration: location, schema, defaults, and per-setting validation
//! (`contracts/config.md`, FR-023, FR-024).
//!
//! Read once at start-up; live reload is out of scope. The defaults documented by FR-023 live
//! here and nowhere else. Validation is deliberately per setting: one typo must not silently
//! reset the user's other choices.

use std::path::{Path, PathBuf};

pub use crate::diag::Diagnostic;
use crate::diag::{self, Condition};
use crate::theme::{self, Requested, Style, Value};

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

/// The user settings. Key combinations are deliberately not here — they live in the compositor's
/// configuration (FR-022).
///
/// The three behaviour settings are feature 001's; the visual ones are feature 002's. Note what
/// `style` is: not what the user wrote, but the **resolved** appearance. Every default and every
/// precedence rule lives in [`crate::theme`], so this module parses and delegates and holds no
/// default of its own (FR-050, data-model.md).
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    pub presentation: Presentation,
    pub placement: Placement,
    pub order: Order,
    /// Whether program icons are drawn at all (FR-056).
    pub icons: bool,
    /// The icon set to draw from, or `None` to follow the desktop's configured set (FR-057).
    ///
    /// Note the vocabulary the spec keeps distinct: this is the *icon set*, whose artwork is
    /// drawn, and it is independent of the overlay theme inside [`Self::style`].
    pub icon_set: Option<String>,
    /// The resolved appearance, read once at start-up and never re-read (FR-060).
    pub style: Style,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            presentation: Presentation::default(),
            placement: Placement::default(),
            order: Order::default(),
            // FR-056: icons are on unless the user turns them off.
            icons: true,
            icon_set: None,
            style: Style::default(),
        }
    }
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
    for d in &diagnostics {
        d.report();
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
            diagnostics.push(Diagnostic::new(
                Condition::InvalidConfigValue,
                "config",
                format!(
                    "not valid TOML{position}: {}, using defaults for every setting",
                    e.message()
                ),
            ));
            return (Configuration::default(), diagnostics);
        }
    };

    // The visual settings are read as written and validated by `theme.rs`, which owns every
    // default and the FR-050 precedence chain. This module decides nothing about them.
    let requested = Requested {
        theme: read_theme_name(&table, &mut diagnostics),
        overrides: read_style_table(&table, &mut diagnostics),
    };
    let (style, style_diagnostics) = theme::resolve(&requested);
    diagnostics.extend(style_diagnostics);

    let configuration = Configuration {
        presentation: read(&table, &mut diagnostics),
        placement: read(&table, &mut diagnostics),
        order: read(&table, &mut diagnostics),
        icons: read_icons(&table, &mut diagnostics),
        icon_set: read_icon_set(&table, &mut diagnostics),
        style,
    };

    for key in table.keys() {
        if !ACCEPTED_KEYS.contains(&key.as_str()) {
            diagnostics.push(Diagnostic::new(
                Condition::UnknownConfigKey,
                format!("config.{key}"),
                "unknown key, ignored",
            ));
        }
    }

    (configuration, diagnostics)
}

/// The settings that differ from their defaults, as `key = value` in the file's own key names
/// (FR-116, FR-071).
///
/// Read from the **resolved** configuration rather than from the file, which is what keeps
/// `--environment` safe to paste into a public issue: the file's own text never reaches it, so a
/// key this program does not recognise, a comment, or a path a user wrote in one cannot leak. A
/// setting whose value was rejected and fell back does not appear either, because it did not in
/// fact differ from its default once the daemon had judged it.
///
/// The order is the order [`contracts/config.md`] lists the settings in: behaviour, then icons,
/// then the theme, then the `[style]` overrides.
///
/// [`contracts/config.md`]: ../../specs/002-overlay-visuals/contracts/config.md
#[must_use]
pub fn differences(configuration: &Configuration) -> Vec<String> {
    let default = Configuration::default();
    let mut out = Vec::new();

    named(configuration.presentation, default.presentation, &mut out);
    named(configuration.placement, default.placement, &mut out);
    named(configuration.order, default.order, &mut out);

    if configuration.icons != default.icons {
        out.push(format!("{ICONS_KEY} = {}", configuration.icons));
    }
    if let Some(set) = &configuration.icon_set {
        out.push(format!("{ICON_SET_KEY} = {set:?}"));
    }

    let style = &configuration.style;
    if style.palette.name != default.style.palette.name {
        out.push(format!("{THEME_KEY} = {:?}", style.palette.name));
    }

    // A `[style]` override is a difference from *the theme in effect*, not from the default
    // palette: with `theme = "light"` every colour differs from the default one, and listing all
    // eleven would bury the one the user actually wrote.
    let base = theme::BUILT_IN
        .iter()
        .find(|theme| theme.name == style.palette.name)
        .copied()
        .unwrap_or(theme::BUILT_IN[0]);
    for colour in theme::COLOURS {
        let written = colour.read(&style.palette);
        if written != colour.read(&base) {
            // `#rrggbbaa`: a colour override can carry opacity — the backdrop's does — and a
            // report that dropped it would say the wrong thing about the one colour that is not
            // opaque.
            out.push(format!(
                "{STYLE_KEY}.{} = {:?}",
                colour.key,
                written.hex_rgba()
            ));
        }
    }
    if style.font_family != default.style.font_family {
        out.push(format!(
            "{STYLE_KEY}.{} = {:?}",
            theme::FONT_FAMILY_KEY,
            style.font_family
        ));
    }
    if (style.text_size - default.style.text_size).abs() > f64::EPSILON {
        out.push(format!(
            "{STYLE_KEY}.{} = {}",
            theme::TEXT_SIZE.key,
            theme::TEXT_SIZE.show(style.text_size)
        ));
    }
    for geometry in theme::GEOMETRY {
        let written = geometry.read(&style.geometry);
        if (written - geometry.read(&default.style.geometry)).abs() > f64::EPSILON {
            out.push(format!(
                "{STYLE_KEY}.{} = {}",
                geometry.key,
                geometry.show(written)
            ));
        }
    }

    out
}

/// One `key = "value"` line for a setting whose values are a fixed vocabulary, or nothing when it
/// is at its default.
fn named<T: Setting + PartialEq>(value: T, default: T, out: &mut Vec<String>) {
    if value != default {
        out.push(format!("{} = {:?}", T::KEY, value.name()));
    }
}

/// Every top-level key the configuration file accepts, in one place (FR-024, FR-079).
///
/// `load` reports anything absent from this list as an unknown key, and the catalogue walk in
/// `theme.rs` checks it against the published configuration contracts — so a key added here
/// without being documented fails `cargo test --lib`, and a key documented but never accepted
/// fails the same way (FR-083).
pub const ACCEPTED_KEYS: &[&str] = &[
    Presentation::KEY,
    Placement::KEY,
    Order::KEY,
    ICONS_KEY,
    ICON_SET_KEY,
    THEME_KEY,
    STYLE_KEY,
];

/// The keys feature 002 adds (`contracts/config.md`).
const ICONS_KEY: &str = "icons";
const ICON_SET_KEY: &str = "icon_set";
const THEME_KEY: &str = "theme";
const STYLE_KEY: &str = "style";

/// `icons` — a boolean, defaulting to shown (FR-056).
fn read_icons(table: &toml::Table, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let default = Configuration::default().icons;
    let Some(value) = table.get(ICONS_KEY) else {
        return default;
    };
    if let Some(icons) = value.as_bool() {
        return icons;
    }
    diagnostics.push(Diagnostic::new(
        Condition::InvalidConfigValue,
        format!("config.{ICONS_KEY}"),
        format!(
            "expected true or false, found {}; using {default}",
            value.type_str()
        ),
    ));
    default
}

/// `icon_set` — a name, or absent to follow the desktop's configured set (FR-057).
///
/// Whether the named set is *installed* is not decided here: that is a filesystem question and it
/// belongs to `icons/iconset.rs`, which reports and falls back on its own.
fn read_icon_set(table: &toml::Table, diagnostics: &mut Vec<Diagnostic>) -> Option<String> {
    let value = table.get(ICON_SET_KEY)?;
    if let Some(name) = value.as_str() {
        return Some(name.to_owned());
    }
    diagnostics.push(Diagnostic::new(
        Condition::InvalidConfigValue,
        format!("config.{ICON_SET_KEY}"),
        format!(
            "expected a string, found {}; following the desktop's configured set",
            value.type_str()
        ),
    ));
    None
}

/// `theme` — a built-in theme's name (FR-049). Whether the name is known is `theme.rs`'s call.
fn read_theme_name(table: &toml::Table, diagnostics: &mut Vec<Diagnostic>) -> Option<String> {
    let value = table.get(THEME_KEY)?;
    if let Some(name) = value.as_str() {
        return Some(name.to_owned());
    }
    diagnostics.push(Diagnostic::new(
        Condition::InvalidConfigValue,
        format!("config.{THEME_KEY}"),
        format!(
            "expected a string, found {}; using the default theme",
            value.type_str()
        ),
    ));
    None
}

/// The `[style]` table, translated out of `toml` into the neutral values `theme.rs` judges.
///
/// The translation is the whole of this module's involvement with style: every question of what a
/// value means, whether it is in range and what happens when it is not is `theme.rs`'s, so a
/// default cannot come to exist in two places (Principle III).
fn read_style_table(
    table: &toml::Table,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(String, Value)> {
    let Some(value) = table.get(STYLE_KEY) else {
        return Vec::new();
    };
    let Some(style) = value.as_table() else {
        diagnostics.push(Diagnostic::new(
            Condition::InvalidConfigValue,
            format!("config.{STYLE_KEY}"),
            format!(
                "expected a table of style values, found {}; using the theme's values throughout",
                value.type_str()
            ),
        ));
        return Vec::new();
    };

    style
        .iter()
        .map(|(key, value)| (key.clone(), neutral(value)))
        .collect()
}

/// One TOML value as the form `theme.rs` takes. Anything that is not a scalar keeps only its type
/// name, which is all a "found a table" message needs.
fn neutral(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::Text(text.clone()),
        toml::Value::Integer(number) => Value::Integer(*number),
        toml::Value::Float(number) => Value::Float(*number),
        other => Value::Other(match other.type_str() {
            "boolean" => "a boolean",
            "table" => "a table",
            "array" => "an array",
            "datetime" => "a datetime",
            _ => "another kind of value",
        }),
    }
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
    diagnostics.push(Diagnostic::new(
        Condition::InvalidConfigValue,
        format!("config.{}", T::KEY),
        format!(
            "{problem}, using default {:?} (accepted: {accepted})",
            default.name()
        ),
    ));
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
                ..Configuration::default()
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
            wallpaper = "dracula"
            "#,
        );
        assert_eq!(
            configuration.order,
            Order::Monitor,
            "the valid keys still apply"
        );
        assert_eq!(subjects(&diagnostics), vec!["config.wallpaper"]);
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

    // T015 — feature 002's four visual settings (contracts/config.md).

    #[test]
    fn the_visual_defaults_are_icons_on_the_desktops_icon_set_and_the_dark_theme() {
        // FR-056, FR-057, FR-049, and FR-049a for everything the style carries.
        let (configuration, diagnostics) = parse("");
        assert!(configuration.icons, "FR-056: icons default to shown");
        assert_eq!(
            configuration.icon_set, None,
            "FR-057: absent means follow the desktop"
        );
        assert_eq!(configuration.style, theme::Style::default());
        assert_eq!(configuration.style.palette, theme::DARK);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_feature_001_configuration_file_is_still_valid_and_still_means_the_same_thing() {
        // The compatibility promise at the end of contracts/config.md.
        let (configuration, diagnostics) = parse(
            r#"
            presentation = "grid"
            placement = "all"
            order = "monitor"
            "#,
        );
        assert!(diagnostics.is_empty());
        assert_eq!(configuration.style, theme::Style::default());
        assert!(configuration.icons);
    }

    #[test]
    fn each_visual_setting_is_parsed() {
        let (configuration, diagnostics) = parse(
            r##"
            icons = false
            icon_set = "Papirus-Dark"
            theme = "dark"

            [style]
            highlight = "#123456"
            font_family = "JetBrains Mono"
            text_size = 0.9
            text_line_height = 28
            width_fraction = 0.9
            "##,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(!configuration.icons);
        assert_eq!(configuration.icon_set.as_deref(), Some("Papirus-Dark"));
        assert_eq!(
            configuration.style.palette.highlight,
            theme::Colour::parse("#123456").expect("a valid colour")
        );
        assert_eq!(configuration.style.font_family, "JetBrains Mono");
        assert!((configuration.style.text_size - 0.9).abs() < 1e-9);
        assert_eq!(configuration.style.geometry.text_line_height, 28);
        assert!((configuration.style.geometry.width_fraction - 0.9).abs() < 1e-9);
    }

    #[test]
    fn one_invalid_visual_setting_falls_back_alone_while_the_rest_still_apply() {
        // SC-022, and FR-059's "one bad value must not discard the rest".
        let (configuration, diagnostics) = parse(
            r##"
            presentation = "grid"
            icons = "yes"

            [style]
            highlight = "not-a-colour"
            text = "#010203"
            grid_cell_width = 0
            "##,
        );

        assert_eq!(configuration.presentation, Presentation::Grid);
        assert!(
            configuration.icons,
            "the bad boolean fell back to its default"
        );
        assert_eq!(
            configuration.style.palette.highlight,
            theme::DARK.highlight,
            "the bad colour fell back alone"
        );
        assert_eq!(
            configuration.style.palette.text,
            theme::Colour::parse("#010203").expect("a valid colour"),
            "the good colour still applied"
        );
        assert_eq!(
            configuration.style.geometry.grid_cell_width, 40,
            "FR-054: out of range is clamped, not rejected"
        );

        let mut reported = subjects(&diagnostics);
        reported.sort_unstable();
        assert_eq!(
            reported,
            vec![
                "config.icons",
                "config.style.grid_cell_width",
                "config.style.highlight"
            ]
        );
    }

    #[test]
    fn every_geometry_key_is_readable_under_the_style_table_as_written() {
        // T076, FR-047: `theme.rs` proves the ten settings resolve; this proves they are reachable
        // from a file, which is the half that lives here. The catalogue is walked rather than the
        // keys spelled out, so a setting added to `theme::GEOMETRY` without a way to write it
        // fails here rather than shipping inert (FR-061, SC-025).
        let mut wanted = theme::Geometry::DEFAULT;
        let rows: Vec<String> = theme::GEOMETRY
            .iter()
            .map(|setting| {
                // Mid-range, so the value is in range without being any setting's default — a key
                // silently dropped on the way through leaves the default behind and is caught.
                let value = f64::midpoint(setting.min, setting.max);
                let value = if setting.integral {
                    value.round()
                } else {
                    value
                };
                setting.write(&mut wanted, value);
                format!("{} = {value}", setting.key)
            })
            .collect();

        let (configuration, diagnostics) = parse(&format!("[style]\n{}\n", rows.join("\n")));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(configuration.style.geometry, wanted);
        assert_ne!(
            configuration.style.geometry,
            theme::Geometry::DEFAULT,
            "the table must move the geometry, or it proves nothing"
        );
    }

    #[test]
    fn an_unknown_theme_or_icon_set_name_is_reported_and_falls_back() {
        // FR-058 for the theme. An unknown *icon set* name is a filesystem question and is
        // reported by `icons/iconset.rs`, so the name is carried through here unjudged (FR-057).
        let (configuration, diagnostics) = parse(
            r#"
            theme = "dracula"
            icon_set = "NoSuchSet"
            order = "compositor"
            "#,
        );
        assert_eq!(configuration.style.palette, theme::DARK);
        assert_eq!(configuration.icon_set.as_deref(), Some("NoSuchSet"));
        assert_eq!(
            configuration.order,
            Order::Compositor,
            "every other setting still applies"
        );
        assert_eq!(subjects(&diagnostics), vec!["config.theme"]);
        assert!(
            diagnostics[0]
                .message
                .contains(r#"unknown theme "dracula""#),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn a_visual_setting_of_the_wrong_type_is_reported_against_its_own_key() {
        let (configuration, diagnostics) = parse(
            r#"
            icons = 1
            icon_set = 2
            theme = 3
            style = "dark"
            "#,
        );
        assert_eq!(configuration, Configuration::default());
        assert_eq!(
            subjects(&diagnostics),
            vec![
                "config.theme",
                "config.style",
                "config.icons",
                "config.icon_set"
            ]
        );
    }

    #[test]
    fn an_unknown_style_key_is_reported_under_the_style_table() {
        let (configuration, diagnostics) = parse(
            r##"
            [style]
            shadow = "#000000"
            text = "#010203"
            "##,
        );
        assert_eq!(
            configuration.style.palette.text,
            theme::Colour::parse("#010203").expect("a valid colour")
        );
        assert_eq!(subjects(&diagnostics), vec!["config.style.shadow"]);
        assert_eq!(diagnostics[0].condition, Condition::UnknownConfigKey);
    }

    #[test]
    fn the_visual_keys_are_not_reported_as_unknown_top_level_keys() {
        let (_, diagnostics) = parse(
            r##"
            icons = true
            icon_set = "hicolor"
            theme = "dark"

            [style]
            text = "#010203"
            "##,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    // The `settings` line of the `--environment` report (FR-116, FR-071).

    /// The differences the report would list, given a configuration file.
    fn listed(source: &str) -> Vec<String> {
        differences(&parse(source).0)
    }

    #[test]
    fn an_unconfigured_daemon_differs_from_its_defaults_in_nothing() {
        assert!(listed("").is_empty());
        // Settings written out at exactly their default values are still not differences: the
        // report says what is unusual about this installation, not what the file happens to hold.
        assert!(
            listed(
                "presentation = \"list\"\nplacement = \"active\"\norder = \"mru\"\nicons = true\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn each_changed_setting_is_listed_under_the_key_the_file_uses() {
        assert_eq!(
            listed("presentation = \"grid\"\n"),
            vec![r#"presentation = "grid""#]
        );
        assert_eq!(
            listed("placement = \"all\"\norder = \"monitor\"\n"),
            vec![r#"placement = "all""#, r#"order = "monitor""#]
        );
        assert_eq!(listed("icons = false\n"), vec!["icons = false"]);
        assert_eq!(
            listed("icon_set = \"Papirus-Dark\"\n"),
            vec![r#"icon_set = "Papirus-Dark""#]
        );
        assert_eq!(listed("theme = \"light\"\n"), vec![r#"theme = "light""#]);
    }

    #[test]
    fn a_style_override_is_measured_against_the_theme_it_sits_on() {
        // With `theme = "light"` every colour differs from the *default* palette, and listing all
        // eleven would bury the one thing the user actually wrote. Only the override appears.
        assert_eq!(
            listed("theme = \"light\"\n\n[style]\nhighlight = \"#ff0000\"\n"),
            vec![r#"theme = "light""#, r##"style.highlight = "#ff0000ff""##]
        );
        // And on the default palette, one override is still exactly one line — carrying the
        // opacity it was written with.
        assert_eq!(
            listed("[style]\nbackdrop = \"#00000080\"\n"),
            vec![r##"style.backdrop = "#00000080""##]
        );
    }

    #[test]
    fn font_and_geometry_overrides_are_listed_as_written() {
        assert_eq!(
            listed(
                "[style]\nfont_family = \"JetBrains Mono\"\ntext_size = 0.9\nrow_padding = 14\n"
            ),
            vec![
                r#"style.font_family = "JetBrains Mono""#,
                "style.text_size = 0.9",
                "style.row_padding = 14",
            ]
        );
    }

    #[test]
    fn a_rejected_value_is_not_a_difference_and_the_file_is_never_echoed() {
        // FR-071: the report is safe to paste into a public issue. A value that was rejected fell
        // back to its default, so it did not in fact differ from it; a key this program does not
        // recognise never reaches the report at all, whatever the user wrote beside it.
        assert!(listed("presentation = \"tiles\"\n").is_empty());
        let reported = listed("secret_path = \"/home/someone/private\"\norder = \"compositor\"\n");
        assert_eq!(reported, vec![r#"order = "compositor""#]);
        assert!(
            !reported.iter().any(|line| line.contains("private")),
            "nothing the file holds outside the schema is reported: {reported:?}"
        );
    }
}
