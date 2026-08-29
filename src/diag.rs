//! Diagnostics: the stderr record format and the notification policy (FR-029–FR-032).
//!
//! The user has no terminal attached, so stderr is the complete record and notifications are
//! reserved for what the user must act on. Every diagnostic string in the application passes
//! through here, and the policy — which level a condition reports at, and whether it notifies —
//! lives in [`Condition`] and nowhere else.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::process::{Child, Command, Stdio};

/// Severity, as it appears at the start of every stderr record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
}

impl Level {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
        }
    }
}

/// Every condition the application reports.
///
/// This enum *is* the notification policy table from `contracts/diagnostics.md`: a condition's
/// level, whether it raises a desktop notification, and the notification summary it uses are all
/// answered here, so the policy cannot drift between call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// An invalid value for one configuration setting (FR-024).
    InvalidConfigValue,
    /// A key in the configuration file that the application does not recognise.
    UnknownConfigKey,
    /// A named global shortcut could not be registered with the compositor.
    ShortcutRegistrationFailed,
    /// Another instance already holds the named shortcuts (FR-025a).
    SecondInstance,
    /// The compositor could not be reached at start-up (FR-025).
    CompositorUnreachableAtStartup,
    /// The command line could not be parsed (FR-033). Never notified: the user is at the terminal
    /// they typed it in, and FR-030 reserves notifications for the three conditions listed there.
    UsageError,
    /// Connection lost, retrying, or reconnected — self-recovering, so never notified (FR-031).
    CompositorConnection,
    /// A swap failed and was rolled back (FR-013b).
    SwapRolledBack,
    /// The rollback of a failed swap itself failed (FR-013c).
    RollbackFailed,
    /// A committed selection was dropped because its target workspace had gone (FR-027).
    SelectionTargetVanished,
    /// The overlay could not take exclusive keyboard focus, so the session was abandoned.
    OverlayFocusRefused,
    /// Delivering a notification failed — reported on stderr only, never notified (FR-032).
    NotifyDeliveryFailed,
    /// An icon file exists but is malformed or unreadable (FR-044).
    ///
    /// Never notified: the overlay is perfectly usable with the placeholder in that slot, so
    /// FR-030's "the user must act on it" test is not met — but it is worth a record, because a
    /// broken file in an icon set is a real fault the user would otherwise only see as a
    /// mysteriously generic icon. Reported once per program and then cached, so the record cannot
    /// repeat on every overlay opening (FR-044).
    IconUnreadable,
}

impl Condition {
    #[must_use]
    pub fn level(self) -> Level {
        match self {
            Self::InvalidConfigValue
            | Self::UnknownConfigKey
            | Self::NotifyDeliveryFailed
            | Self::IconUnreadable => Level::Warn,
            Self::ShortcutRegistrationFailed
            | Self::SecondInstance
            | Self::CompositorUnreachableAtStartup
            | Self::UsageError
            | Self::SwapRolledBack
            | Self::RollbackFailed
            | Self::OverlayFocusRefused => Level::Error,
            Self::CompositorConnection | Self::SelectionTargetVanished => Level::Info,
        }
    }

    /// The notification summary for conditions the user has to act on (FR-030), or `None` for
    /// conditions the application recovers from on its own (FR-031).
    #[must_use]
    pub fn summary(self) -> Option<&'static str> {
        match self {
            Self::InvalidConfigValue => Some("hypr-swap: configuration problem"),
            Self::ShortcutRegistrationFailed | Self::SecondInstance => {
                Some("hypr-swap: shortcut not registered")
            }
            Self::CompositorUnreachableAtStartup => Some("hypr-swap: cannot reach Hyprland"),
            Self::SwapRolledBack | Self::RollbackFailed => Some("hypr-swap: swap failed"),
            Self::UsageError
            | Self::UnknownConfigKey
            | Self::CompositorConnection
            | Self::SelectionTargetVanished
            | Self::OverlayFocusRefused
            | Self::NotifyDeliveryFailed
            | Self::IconUnreadable => None,
        }
    }

    /// Whether this condition accompanies its stderr record with a desktop notification.
    #[must_use]
    pub fn notifies(self) -> bool {
        self.summary().is_some()
    }
}

/// One thing worth telling the user about, produced rather than printed.
///
/// Validation builds these instead of writing to stderr itself, so a whole schema — including the
/// exact subject each problem is reported under — is unit-testable without capturing output. The
/// caller hands each one to [`report`]. It lives here rather than in [`crate::config`] because
/// [`crate::theme`] produces them too, and the record format has exactly one home (FR-029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub condition: Condition,
    pub subject: String,
    pub message: String,
}

impl Diagnostic {
    /// Build one, so the three fields are never assembled in a different order at a call site.
    #[must_use]
    pub fn new(
        condition: Condition,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            condition,
            subject: subject.into(),
            message: message.into(),
        }
    }

    /// Report it through the one policy-owning path.
    pub fn report(&self) {
        report(self.condition, &self.subject, &self.message);
    }
}

/// The one stderr record shape: `<LEVEL> <subject>: <message>`.
///
/// The level is padded to a fixed width so records line up in a journal; no timestamp, because
/// whatever supervises the process adds one.
#[must_use]
pub fn format_record(level: Level, subject: &str, message: &str) -> String {
    format!("{:<5} {subject}: {message}", level.as_str())
}

/// Report a condition: one stderr record, plus a desktop notification when the policy calls for
/// one.
pub fn report(condition: Condition, subject: &str, message: &str) {
    let record = format_record(condition.level(), subject, message);
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{record}");
    drop(stderr);

    if let Some(summary) = condition.summary() {
        // The body carries the subject as well as the message, because a notification is read on
        // its own: "unknown value \"tiles\"" without `config.presentation` in front of it would
        // not tell the user which setting to go and fix (US5-AS5).
        notify(summary, &format!("{subject}: {message}"));
    }
}

// --- Paint records (research.md R22) -----------------------------------------

/// The environment gate that turns on one record per painted entry.
///
/// This feature's requirements are almost entirely visual, and screenshot comparison was rejected
/// as brittle in feature 001's R14. Instead the daemon can be asked to say what it resolved and
/// drew, on the stderr that FR-029 already defines as its diagnostic interface, and the E2E suite
/// asserts on that. Unset — which is always the case in normal use — this costs one environment
/// read per painted entry and produces nothing at all.
///
/// The precedent is [`crate::hypr::ipc::FAULT_INJECTION_VAR`], the env-gated hook feature 001's
/// rollback tests use (plan.md → Complexity Tracking).
pub const PAINT_RECORDS_VAR: &str = "HYPR_SWAP_E2E_PAINT_RECORDS";

/// The subject every paint record is reported under, so a test can filter the daemon's stderr for
/// exactly these lines.
pub const PAINT_SUBJECT: &str = "paint";

/// Whether the gate is open, given what the environment holds.
///
/// Split from [`paint_records_enabled`] so "silent unless asked" is testable without mutating the
/// process environment out from under a parallel test.
#[must_use]
fn records_wanted(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// Whether paint records are being collected.
#[must_use]
pub fn paint_records_enabled() -> bool {
    records_wanted(std::env::var_os(PAINT_RECORDS_VAR).as_deref())
}

/// The record one painted entry produces: `entry <index> <presentation>: <detail>`.
///
/// Split from [`paint`] so its content is unit-testable without capturing stderr.
#[must_use]
pub fn paint_record(index: usize, presentation: &str, detail: &str) -> String {
    format!("entry {index} {presentation}: {detail}")
}

/// Record what was resolved and drawn for one entry — but only when the gate is open.
///
/// Never notifies and never raises the level above `INFO`: this is evidence for a test, not
/// something a user is meant to act on (FR-031).
pub fn paint(index: usize, presentation: &str, detail: &str) {
    if !paint_records_enabled() {
        return;
    }
    let record = format_record(
        Level::Info,
        PAINT_SUBJECT,
        &paint_record(index, presentation, detail),
    );
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{record}");
}

/// The record one paint's colours produce: `colours <presentation>: [#rrggbbaa …]`.
///
/// The colours are the ones actually handed to cairo over that paint, in the order they were
/// first used, so the record is evidence about the pixels rather than about the configuration
/// that was read (T058, research.md R22). A theme reaches the screen or it does not appear here.
#[must_use]
pub fn paint_colours_record(presentation: &str, colours: &[String]) -> String {
    format!("colours {presentation}: [{}]", colours.join(" "))
}

/// Record the colours one paint drew with — but only when the gate is open.
pub fn paint_colours(presentation: &str, colours: &[String]) {
    if !paint_records_enabled() {
        return;
    }
    let record = format_record(
        Level::Info,
        PAINT_SUBJECT,
        &paint_colours_record(presentation, colours),
    );
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{record}");
}

/// The record one paint's fonts produce:
/// `fonts <presentation>: requested=["…"] resolved=["…"]`.
///
/// `requested` is every family this paint asked pango for and `resolved` is every family pango
/// actually loaded, both distinct and in first-use order. The pair is what makes FR-046 testable
/// from outside: one requested family means every piece of text on the overlay was laid out in
/// the configured one, and a resolved family that differs from it is the platform substituting an
/// absent family without anything being reported (US4-AS3, US4-AS5).
#[must_use]
pub fn paint_fonts_record(presentation: &str, requested: &[String], resolved: &[String]) -> String {
    format!(
        "fonts {presentation}: requested=[{}] resolved=[{}]",
        quoted(requested),
        quoted(resolved)
    )
}

/// A list of families as the record spells it — quoted, because a family name has spaces in it.
fn quoted(families: &[String]) -> String {
    families
        .iter()
        .map(|family| format!("{family:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Record the fonts one paint laid its text out in — but only when the gate is open.
pub fn paint_fonts(presentation: &str, requested: &[String], resolved: &[String]) {
    if !paint_records_enabled() {
        return;
    }
    let record = format_record(
        Level::Info,
        PAINT_SUBJECT,
        &paint_fonts_record(presentation, requested, resolved),
    );
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{record}");
}

thread_local! {
    /// Notification children, kept only long enough to be reaped. The child is never waited on
    /// synchronously — a wedged notification daemon must not stall the event loop — so completed
    /// ones are collected opportunistically on the next spawn.
    static PENDING: RefCell<Vec<Child>> = const { RefCell::new(Vec::new()) };
    /// A failed spawn is reported at most once per process (FR-032); the underlying diagnostic
    /// still reaches stderr every time.
    static SPAWN_FAILURE_REPORTED: Cell<bool> = const { Cell::new(false) };
}

/// Raise a desktop notification by spawning `notify-send` **detached**.
fn notify(summary: &str, body: &str) {
    PENDING.with_borrow_mut(|pending| {
        pending.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_)) | Err(_)));

        let spawned = Command::new("notify-send")
            .arg("--app-name=hypr-swap")
            .arg("--")
            .arg(summary)
            .arg(body)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match spawned {
            Ok(child) => pending.push(child),
            Err(e) => report_spawn_failure(&e),
        }
    });
}

/// Never recurses: this condition does not notify.
fn report_spawn_failure(error: &std::io::Error) {
    if SPAWN_FAILURE_REPORTED.replace(true) {
        return;
    }
    report(
        Condition::NotifyDeliveryFailed,
        "notify",
        &format!("notify-send unavailable ({error}), diagnostics continue on stderr only"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Condition; 12] = [
        Condition::InvalidConfigValue,
        Condition::UnknownConfigKey,
        Condition::ShortcutRegistrationFailed,
        Condition::SecondInstance,
        Condition::CompositorUnreachableAtStartup,
        Condition::UsageError,
        Condition::CompositorConnection,
        Condition::SwapRolledBack,
        Condition::RollbackFailed,
        Condition::SelectionTargetVanished,
        Condition::OverlayFocusRefused,
        Condition::NotifyDeliveryFailed,
    ];

    #[test]
    fn a_notification_body_names_the_subject_as_well_as_the_message() {
        // US5-AS5: the offending setting is named on stderr *and* in the notification. The body
        // is built where the notification is raised, so this asserts the shape it is built from.
        let record = format_record(
            Condition::InvalidConfigValue.level(),
            "config.presentation",
            r#"unknown value "tiles", using default "list""#,
        );
        let body = record
            .split_once(' ')
            .map(|(_, rest)| rest.trim_start())
            .expect("the record has a level prefix");
        assert!(body.starts_with("config.presentation: "), "{body}");
    }

    #[test]
    fn record_shape_is_level_subject_message() {
        assert_eq!(
            format_record(
                Level::Warn,
                "config.presentation",
                r#"unknown value "tiles""#
            ),
            r#"WARN  config.presentation: unknown value "tiles""#
        );
        assert_eq!(
            format_record(Level::Error, "compositor", "cannot connect at start-up"),
            "ERROR compositor: cannot connect at start-up"
        );
        assert_eq!(
            format_record(Level::Info, "compositor", "reconnected"),
            "INFO  compositor: reconnected"
        );
    }

    #[test]
    fn record_has_no_timestamp_and_exactly_one_line() {
        let record = format_record(Level::Info, "compositor", "connection lost");
        assert!(!record.contains('\n'));
        assert!(record.starts_with("INFO "));
    }

    #[test]
    fn levels_match_the_documented_policy_table() {
        // contracts/diagnostics.md, one row per condition.
        assert_eq!(Condition::InvalidConfigValue.level(), Level::Warn);
        assert_eq!(Condition::UnknownConfigKey.level(), Level::Warn);
        assert_eq!(Condition::NotifyDeliveryFailed.level(), Level::Warn);
        assert_eq!(Condition::ShortcutRegistrationFailed.level(), Level::Error);
        assert_eq!(Condition::SecondInstance.level(), Level::Error);
        assert_eq!(
            Condition::CompositorUnreachableAtStartup.level(),
            Level::Error
        );
        assert_eq!(Condition::UsageError.level(), Level::Error);
        assert_eq!(Condition::SwapRolledBack.level(), Level::Error);
        assert_eq!(Condition::RollbackFailed.level(), Level::Error);
        assert_eq!(Condition::OverlayFocusRefused.level(), Level::Error);
        assert_eq!(Condition::CompositorConnection.level(), Level::Info);
        assert_eq!(Condition::SelectionTargetVanished.level(), Level::Info);
    }

    #[test]
    fn only_conditions_the_user_must_act_on_notify() {
        let notifying: Vec<_> = ALL.into_iter().filter(|c| c.notifies()).collect();
        assert_eq!(
            notifying,
            vec![
                Condition::InvalidConfigValue,
                Condition::ShortcutRegistrationFailed,
                Condition::SecondInstance,
                Condition::CompositorUnreachableAtStartup,
                Condition::SwapRolledBack,
                Condition::RollbackFailed,
            ]
        );
    }

    #[test]
    fn a_usage_error_is_reported_but_never_notified() {
        // FR-030 names exactly three notifying conditions and a bad command line is none of them.
        // Reporting it as CompositorUnreachableAtStartup — which is what it used to share — put
        // "cannot reach Hyprland" on the user's screen for a mistyped flag.
        assert!(!Condition::UsageError.notifies());
        assert_ne!(
            Condition::UsageError.summary(),
            Condition::CompositorUnreachableAtStartup.summary()
        );
    }

    #[test]
    fn self_recovering_conditions_never_notify() {
        // FR-031: reconnection is stderr-only. FR-032: a failed notification never recurses.
        for condition in [
            Condition::CompositorConnection,
            Condition::SelectionTargetVanished,
            Condition::UnknownConfigKey,
            Condition::NotifyDeliveryFailed,
            Condition::OverlayFocusRefused,
        ] {
            assert!(!condition.notifies(), "{condition:?} must not notify");
            assert!(condition.summary().is_none());
        }
    }

    #[test]
    fn notification_summaries_are_the_four_documented_ones() {
        let mut summaries: Vec<_> = ALL.into_iter().filter_map(Condition::summary).collect();
        summaries.sort_unstable();
        summaries.dedup();
        assert_eq!(
            summaries,
            vec![
                "hypr-swap: cannot reach Hyprland",
                "hypr-swap: configuration problem",
                "hypr-swap: shortcut not registered",
                "hypr-swap: swap failed",
            ]
        );
    }

    // T022 — the env-gated paint records (research.md R22).

    #[test]
    fn a_paint_record_names_the_entry_the_presentation_and_what_was_drawn() {
        assert_eq!(
            paint_record(0, "list", "icon /usr/share/icons/Set/48x48/apps/foot.png"),
            "entry 0 list: icon /usr/share/icons/Set/48x48/apps/foot.png"
        );
        assert_eq!(
            paint_record(3, "grid", "placeholder"),
            "entry 3 grid: placeholder"
        );
        // The whole record, as it reaches stderr: one INFO line under the one subject a test
        // filters on.
        assert_eq!(
            format_record(
                Level::Info,
                PAINT_SUBJECT,
                &paint_record(2, "grid", "shed title")
            ),
            "INFO  paint: entry 2 grid: shed title"
        );
    }

    #[test]
    fn a_colour_record_names_the_presentation_and_every_colour_drawn() {
        // T058: the tape is a list of `#rrggbbaa` values in first-use order, and it carries alpha
        // because the backdrop is the one themed colour that is not opaque.
        assert_eq!(
            paint_colours_record("list", &["#17171ced".to_owned(), "#336bb8ff".to_owned()]),
            "colours list: [#17171ced #336bb8ff]"
        );
        // A paint that drew nothing at all still says so, rather than looking like a paint that
        // never happened.
        assert_eq!(paint_colours_record("grid", &[]), "colours grid: []");
        assert_eq!(
            format_record(
                Level::Info,
                PAINT_SUBJECT,
                &paint_colours_record("grid", &["#292930ff".to_owned()])
            ),
            "INFO  paint: colours grid: [#292930ff]"
        );
    }

    #[test]
    fn a_font_record_names_what_was_asked_for_and_what_was_loaded() {
        // T069: the evidence for FR-046. One requested family means every layout of that paint
        // was given the configured one; a resolved family that differs is the substitution
        // US4-AS5 allows, recorded rather than reported.
        assert_eq!(
            paint_fonts_record(
                "list",
                &["JetBrains Mono".to_owned()],
                &["JetBrains Mono".to_owned()]
            ),
            r#"fonts list: requested=["JetBrains Mono"] resolved=["JetBrains Mono"]"#
        );
        assert_eq!(
            paint_fonts_record(
                "grid",
                &["No Such Family".to_owned()],
                &["DejaVu Sans".to_owned()]
            ),
            r#"fonts grid: requested=["No Such Family"] resolved=["DejaVu Sans"]"#
        );
        // A paint with no text at all still says so, as the colour record does.
        assert_eq!(
            paint_fonts_record("list", &[], &[]),
            "fonts list: requested=[] resolved=[]"
        );
        assert_eq!(
            format_record(
                Level::Info,
                PAINT_SUBJECT,
                &paint_fonts_record("grid", &["Sans".to_owned()], &["Sans".to_owned()])
            ),
            r#"INFO  paint: fonts grid: requested=["Sans"] resolved=["Sans"]"#
        );
    }

    #[test]
    fn paint_records_are_silent_unless_the_gate_is_set() {
        // Unset, empty, and whitespace-free presence are the three cases the gate distinguishes.
        assert!(!records_wanted(None), "no variable means no records");
        assert!(
            !records_wanted(Some(std::ffi::OsStr::new(""))),
            "an empty value is as good as unset"
        );
        assert!(records_wanted(Some(std::ffi::OsStr::new("1"))));

        // And in an ordinary run — which is what every test process is — nothing is collected.
        assert!(
            !paint_records_enabled() || std::env::var_os(PAINT_RECORDS_VAR).is_some(),
            "records are only ever enabled by {PAINT_RECORDS_VAR}"
        );
    }

    #[test]
    fn a_paint_record_is_never_a_notifying_condition() {
        // FR-031: test evidence must not put anything on the user's screen. The paint path does
        // not go through `Condition` at all, which is what makes that structural.
        assert_eq!(Level::Info.as_str(), "INFO");
        assert!(!paint_record(0, "list", "placeholder").contains('\n'));
    }

    #[test]
    fn every_condition_has_a_level_and_a_consistent_notify_flag() {
        for condition in ALL {
            let _ = condition.level();
            assert_eq!(condition.notifies(), condition.summary().is_some());
        }
    }
}
