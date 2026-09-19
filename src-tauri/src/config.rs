//! Local configuration: schedules plus app options, stored as one small JSON file in the
//! app config directory. No database and no store plugin - a plain document the user can
//! read or hand-edit.
//!
//! This module deliberately has no Tauri dependency: the file path is passed in, which keeps
//! everything here unit-testable without an app handle.

use crate::i18n::{self, Lang, TimeFormat};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

/// Weekday a schedule can be restricted to. Serialised as lowercase (`"monday"`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Day {
    /// Monday to Sunday, the order the UI and tests expect.
    pub const ALL: [Day; 7] = [
        Day::Monday,
        Day::Tuesday,
        Day::Wednesday,
        Day::Thursday,
        Day::Friday,
        Day::Saturday,
        Day::Sunday,
    ];

    /// Turns a date's weekday into the day a schedule is filtered by.
    pub fn from_weekday(weekday: chrono::Weekday) -> Day {
        match weekday {
            chrono::Weekday::Mon => Day::Monday,
            chrono::Weekday::Tue => Day::Tuesday,
            chrono::Weekday::Wed => Day::Wednesday,
            chrono::Weekday::Thu => Day::Thursday,
            chrono::Weekday::Fri => Day::Friday,
            chrono::Weekday::Sat => Day::Saturday,
            chrono::Weekday::Sun => Day::Sunday,
        }
    }
}

/// What a schedule does when it fires.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    ConservationOn,
    ConservationOff,
}

impl Action {
    /// The conservation-mode state this action asks for.
    pub fn conservation(self) -> bool {
        matches!(self, Action::ConservationOn)
    }

    /// Short English text for logs. Anything the user sees comes from `i18n` instead.
    pub fn label(self) -> &'static str {
        match self {
            Action::ConservationOn => "Conservation Mode enabled",
            Action::ConservationOff => "Conservation Mode disabled",
        }
    }
}

/// One scheduled switch. Serialised in camelCase to match the frontend types.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    /// Stable identifier for the UI (React keys, edit/delete) and for logs.
    pub id: String,
    pub enabled: bool,
    /// Local time, 24-hour `"HH:MM"`.
    pub time: String,
    /// Days the schedule applies to; empty means it never fires.
    pub days: Vec<Day>,
    pub action: Action,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            time: "05:00".to_string(),
            days: Day::ALL.to_vec(),
            action: Action::ConservationOff,
        }
    }
}

impl Schedule {
    pub fn new(
        id: impl Into<String>,
        time: impl Into<String>,
        days: Vec<Day>,
        action: Action,
    ) -> Self {
        Self {
            id: id.into(),
            enabled: true,
            time: time.into(),
            days,
            action,
        }
    }

    /// `"05:00"` becomes `Some((5, 0))`. Anything that is not a 24-hour `HH:MM` value
    /// (including out-of-range hours/minutes) returns `None`, so a hand-edited file can
    /// never panic the scheduler.
    pub fn time_parts(&self) -> Option<(u32, u32)> {
        let (hours, minutes) = self.time.split_once(':')?;
        if hours.len() != 2 || minutes.len() != 2 {
            return None;
        }
        let hours: u32 = hours.parse().ok()?;
        let minutes: u32 = minutes.parse().ok()?;
        (hours < 24 && minutes < 60).then_some((hours, minutes))
    }

    /// A unique id derived from the clock; avoids a uuid dependency for a value only used as a key.
    pub fn generate_id() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("s{nanos:x}")
    }
}

/// The whole persisted document. Every field has a default, so partial or older files load.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Master switch for automatic switching; manual control always works.
    pub schedule_enabled: bool,
    /// Register the app to start with Windows.
    pub start_with_windows: bool,
    /// Show a Windows notification when a scheduled switch actually changes the mode.
    pub notify_on_change: bool,
    /// The language of the window, the tray and notifications.
    pub locale: Lang,
    /// How times are written in the window. Defaults to whatever Windows is set to.
    pub time_format: TimeFormat,
    pub schedules: Vec<Schedule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schedule_enabled: true,
            start_with_windows: true,
            notify_on_change: true,
            // A first run already speaks the language Windows is set to...
            locale: Lang::detect(),
            // ...and writes times the way Windows does.
            time_format: TimeFormat::detect(),
            schedules: Vec::new(),
        }
    }
}

impl Config {
    /// Rejects what the UI must not save. Returns a message in the configured language.
    pub fn validate(&self) -> Result<(), String> {
        let strings = self.locale.strings();
        let mut seen = HashSet::new();
        for schedule in &self.schedules {
            if schedule.id.trim().is_empty() {
                return Err(strings.config_missing_id.to_string());
            }
            if !seen.insert(schedule.id.as_str()) {
                return Err(strings.config_duplicate_id.to_string());
            }
            if schedule.time_parts().is_none() {
                return Err(i18n::fill(strings.config_invalid_time, &[&schedule.time]));
            }
            if schedule.days.is_empty() {
                return Err(strings.config_needs_a_day.to_string());
            }
        }
        Ok(())
    }

    /// Gives every schedule a usable, unique id (used after loading a hand-edited file).
    fn ensure_schedule_ids(&mut self) {
        let prefix = Schedule::generate_id();
        let mut seen: HashSet<String> = HashSet::new();
        for (index, schedule) in self.schedules.iter_mut().enumerate() {
            if schedule.id.trim().is_empty() || seen.contains(&schedule.id) {
                let mut candidate = format!("{prefix}-{index}");
                let mut suffix = 0;
                while seen.contains(&candidate) {
                    suffix += 1;
                    candidate = format!("{prefix}-{index}-{suffix}");
                }
                schedule.id = candidate;
            }
            seen.insert(schedule.id.clone());
        }
    }
}

/// Outcome of reading the config file. `warning` carries anything worth logging or showing,
/// so a damaged file is never reset silently.
#[derive(Debug)]
pub struct LoadResult {
    pub config: Config,
    pub warning: Option<String>,
}

/// Reads the config file. A missing file yields defaults; a damaged file is preserved next to
/// the original as `<name>.corrupt` and defaults are returned.
pub fn load(path: &Path) -> LoadResult {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return LoadResult {
                config: Config::default(),
                warning: None,
            };
        }
        Err(error) => {
            // The file could not be read, so its language is unknown: speak the OS language.
            let strings = Lang::detect().strings();
            return LoadResult {
                config: Config::default(),
                warning: Some(i18n::fill(
                    strings.config_unreadable,
                    &[&path.display().to_string(), &error.to_string()],
                )),
            };
        }
    };

    match serde_json::from_str::<Config>(&text) {
        Ok(mut config) => {
            config.ensure_schedule_ids();
            LoadResult {
                config,
                warning: None,
            }
        }
        Err(error) => {
            // Nothing could be parsed, so the language is unknown: speak the OS language.
            let strings = Lang::detect().strings();
            let backup = path.with_extension("corrupt");
            let warning = match fs::rename(path, &backup) {
                Ok(()) => i18n::fill(
                    strings.config_damaged,
                    &[
                        &path.display().to_string(),
                        &error.to_string(),
                        &backup.display().to_string(),
                    ],
                ),
                Err(rename_error) => i18n::fill(
                    strings.config_damaged_unkept,
                    &[
                        &path.display().to_string(),
                        &error.to_string(),
                        &rename_error.to_string(),
                    ],
                ),
            };
            LoadResult {
                config: Config::default(),
                warning: Some(warning),
            }
        }
    }
}

/// Writes the config file, replacing the previous one in a single rename so a crash mid-write
/// cannot leave a half-written file behind.
pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    fn workdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lenovo-conservation-scheduler-config-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    fn weekdays() -> Vec<Day> {
        vec![
            Day::Monday,
            Day::Tuesday,
            Day::Wednesday,
            Day::Thursday,
            Day::Friday,
        ]
    }

    #[test]
    fn missing_file_yields_defaults_without_warning() {
        let path = workdir("missing").join("config.json");
        let loaded = load(&path);
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.warning.is_none());
        assert!(!path.exists(), "loading must not create the file");
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let path = workdir("roundtrip").join("config.json");
        let config = Config {
            schedule_enabled: false,
            start_with_windows: false,
            notify_on_change: true,
            locale: Lang::Id,
            time_format: TimeFormat::TwelveHour,
            schedules: vec![
                Schedule::new("morning", "05:00", weekdays(), Action::ConservationOff),
                Schedule::new("work", "09:00", weekdays(), Action::ConservationOn),
            ],
        };
        save(&path, &config).expect("save");
        let loaded = load(&path);
        assert_eq!(loaded.warning, None);
        assert_eq!(loaded.config, config);
    }

    #[test]
    fn file_matches_the_documented_json_shape() {
        let path = workdir("shape").join("config.json");
        save(
            &path,
            &Config {
                schedules: vec![Schedule::new(
                    "mon-fri-off",
                    "05:00",
                    weekdays(),
                    Action::ConservationOff,
                )],
                ..Config::default()
            },
        )
        .expect("save");

        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"scheduleEnabled\": true"), "{text}");
        assert!(text.contains("\"locale\":"), "{text}");
        assert!(text.contains("\"time\": \"05:00\""), "{text}");
        assert!(text.contains("\"action\": \"conservation_off\""), "{text}");
        assert!(text.contains("\"monday\""), "{text}");
        assert!(text.contains("\"friday\""), "{text}");
    }

    #[test]
    fn damaged_file_is_preserved_and_reported() {
        let dir = workdir("damaged");
        let path = dir.join("config.json");
        fs::write(&path, "{ this is not json").expect("write");

        let loaded = load(&path);
        assert_eq!(loaded.config, Config::default());
        let warning = loaded.warning.expect("warning about damaged file");
        // The wording follows the OS language, so assert on what must always be there.
        assert!(warning.contains(&path.display().to_string()), "{warning}");
        assert!(
            warning.contains(&dir.join("config.corrupt").display().to_string()),
            "{warning}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("config.corrupt")).expect("backup exists"),
            "{ this is not json"
        );
        assert!(!path.exists());
    }

    #[test]
    fn partial_file_keeps_defaults_for_missing_fields() {
        let path = workdir("partial").join("config.json");
        fs::write(
            &path,
            r#"{"schedules":[{"id":"x","time":"09:00","days":["monday","saturday"],"action":"conservation_on"}]}"#,
        )
        .expect("write");

        let loaded = load(&path);
        assert!(loaded.warning.is_none());
        assert!(loaded.config.schedule_enabled);
        assert!(loaded.config.start_with_windows);
        assert!(loaded.config.notify_on_change);

        let schedule = &loaded.config.schedules[0];
        assert!(
            schedule.enabled,
            "a schedule without \"enabled\" runs by default"
        );
        assert_eq!(schedule.days, vec![Day::Monday, Day::Saturday]);
        assert_eq!(schedule.action, Action::ConservationOn);
        assert!(schedule.action.conservation());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let path = workdir("unknown").join("config.json");
        fs::write(&path, r#"{"schedules":[],"somethingFromTheFuture":42}"#).expect("write");
        let loaded = load(&path);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn ids_are_filled_in_and_made_unique() {
        let path = workdir("ids").join("config.json");
        fs::write(
            &path,
            r#"{"schedules":[
                {"time":"05:00","days":["monday"],"action":"conservation_off"},
                {"id":"dup","time":"06:00","days":["monday"],"action":"conservation_on"},
                {"id":"dup","time":"07:00","days":["monday"],"action":"conservation_on"}
            ]}"#,
        )
        .expect("write");

        let loaded = load(&path);
        let ids: Vec<&str> = loaded
            .config
            .schedules
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| !id.is_empty()));
        let unique: HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), 3, "ids must be unique, got {ids:?}");
        assert_eq!(
            loaded.config.validate(),
            Ok(()),
            "repaired config must be saveable"
        );
    }

    #[test]
    fn time_parsing_accepts_only_24_hour_hh_mm() {
        let parse =
            |time: &str| Schedule::new("a", time, weekdays(), Action::ConservationOn).time_parts();

        assert_eq!(parse("00:00"), Some((0, 0)));
        assert_eq!(parse("05:00"), Some((5, 0)));
        assert_eq!(parse("23:59"), Some((23, 59)));
        assert_eq!(parse("5:00"), None);
        assert_eq!(parse("05:0"), None);
        assert_eq!(parse("24:00"), None);
        assert_eq!(parse("23:60"), None);
        assert_eq!(parse("-1:00"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("noon"), None);
    }

    #[test]
    fn validation_rejects_what_the_ui_must_not_save() {
        let valid = Schedule::new("a", "05:00", weekdays(), Action::ConservationOff);
        // Pinned to English so the expected fragments below stay meaningful.
        let english = Lang::EnUs;
        assert_eq!(
            Config {
                locale: english,
                schedules: vec![valid.clone()],
                ..Config::default()
            }
            .validate(),
            Ok(())
        );

        let cases: Vec<(Schedule, &str)> = vec![
            (
                Schedule {
                    id: "  ".into(),
                    ..valid.clone()
                },
                "id",
            ),
            (
                Schedule {
                    time: "5:00".into(),
                    ..valid.clone()
                },
                "valid time",
            ),
            (
                Schedule {
                    days: vec![],
                    ..valid.clone()
                },
                "at least one day",
            ),
        ];
        for (schedule, expected) in cases {
            let error = Config {
                locale: english,
                schedules: vec![schedule],
                ..Config::default()
            }
            .validate()
            .expect_err("must be rejected");
            assert!(
                error.contains(expected),
                "{error} should mention {expected}"
            );
        }

        let duplicate = Config {
            locale: english,
            schedules: vec![valid.clone(), valid],
            ..Config::default()
        };
        assert!(
            duplicate.validate().is_err(),
            "duplicate ids must be rejected"
        );
    }

    #[test]
    fn validation_answers_in_the_configured_language() {
        let broken = Config {
            locale: Lang::Id,
            schedules: vec![Schedule::new(
                "a",
                "5:00",
                weekdays(),
                Action::ConservationOn,
            )],
            ..Config::default()
        };
        let message = broken.validate().expect_err("must be rejected");
        assert!(message.contains("bukan waktu"), "{message}");
        assert!(
            message.contains("5:00"),
            "the offending value must be named: {message}"
        );
    }

    #[test]
    fn a_loaded_file_drives_the_schedule_decisions() {
        // The documented example, end to end: JSON on disk -> config -> what the scheduler decides.
        let path = workdir("decisions").join("config.json");
        fs::write(
            &path,
            r#"{
                "scheduleEnabled": true,
                "startWithWindows": false,
                "notifyOnChange": true,
                "schedules": [
                    {"id": "off", "enabled": true, "time": "05:00",
                     "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
                     "action": "conservation_off"},
                    {"id": "on", "enabled": true, "time": "09:00",
                     "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
                     "action": "conservation_on"},
                    {"id": "weekend", "enabled": false, "time": "07:00",
                     "days": ["saturday", "sunday"], "action": "conservation_on"}
                ]
            }"#,
        )
        .expect("write");

        let loaded = load(&path);
        assert_eq!(loaded.warning, None);
        let config = loaded.config;
        assert!(!config.start_with_windows);

        let monday_0730 = NaiveDate::from_ymd_opt(2026, 9, 14)
            .expect("date")
            .and_hms_opt(7, 30, 0)
            .expect("time");
        assert_eq!(
            crate::scheduler::expected_action(&config, monday_0730),
            Some(Action::ConservationOff),
            "05:00 has passed and 09:00 has not"
        );
        assert_eq!(
            crate::scheduler::next_event_at(&config, monday_0730),
            Some(
                NaiveDate::from_ymd_opt(2026, 9, 14)
                    .expect("date")
                    .and_hms_opt(9, 0, 0)
                    .expect("time")
            )
        );

        let saturday_0800 = NaiveDate::from_ymd_opt(2026, 9, 19)
            .expect("date")
            .and_hms_opt(8, 0, 0)
            .expect("time");
        assert_eq!(
            crate::scheduler::expected_action(&config, saturday_0800),
            Some(Action::ConservationOn),
            "the disabled weekend entry must not fire, so Friday's 09:00 still stands"
        );

        // The file's master switch is what the scheduler honours.
        let mut switched_off = config.clone();
        switched_off.schedule_enabled = false;
        assert_eq!(
            crate::scheduler::expected_action(&switched_off, monday_0730),
            None
        );
        assert_eq!(
            crate::scheduler::next_event_at(&switched_off, monday_0730),
            None
        );
    }

    #[test]
    fn save_replaces_the_previous_file() {
        let path = workdir("replace").join("config.json");
        save(&path, &Config::default()).expect("first save");
        save(
            &path,
            &Config {
                schedules: vec![Schedule::new(
                    "a",
                    "05:00",
                    weekdays(),
                    Action::ConservationOff,
                )],
                ..Config::default()
            },
        )
        .expect("second save");

        let loaded = load(&path);
        assert_eq!(loaded.config.schedules.len(), 1);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temporary file must not be left behind"
        );
    }
}
