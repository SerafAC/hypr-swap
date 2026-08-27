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
}

impl Condition {
    #[must_use]
    pub fn level(self) -> Level {
        match self {
            Self::InvalidConfigValue | Self::UnknownConfigKey | Self::NotifyDeliveryFailed => {
                Level::Warn
            }
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
            | Self::NotifyDeliveryFailed => None,
        }
    }

    /// Whether this condition accompanies its stderr record with a desktop notification.
    #[must_use]
    pub fn notifies(self) -> bool {
        self.summary().is_some()
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

    #[test]
    fn every_condition_has_a_level_and_a_consistent_notify_flag() {
        for condition in ALL {
            let _ = condition.level();
            assert_eq!(condition.notifies(), condition.summary().is_some());
        }
    }
}
