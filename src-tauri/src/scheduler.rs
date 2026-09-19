//! When to switch, and what the schedule expects right now.
//!
//! Everything here is a pure function of the schedules and a local time, so the behaviour that
//! matters is testable without a clock, a thread, or the Lenovo driver. Waiting is in
//! [`crate::timer`]; combining these decisions with the battery lives in `lib.rs`.
//!
//! An "occurrence" is one schedule on one date whose weekday is selected. Because every schedule
//! has at least one day, looking one week back or forward always covers the next or previous one.

use crate::config::{Action, Config, Day, Schedule};
use crate::lenovo::ChargingMode;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};

/// One week: the longest gap possible between two occurrences of the same schedule.
const SEARCH_DAYS: u64 = 7;

/// The action of the most recent occurrence at or before `now`, or `None` when the schedule has
/// no opinion yet (scheduling switched off, no schedules, or nothing has come round since).
///
/// This is what the app reconciles against: it answers "given the clock, which mode should be
/// active right now". It is equally valid at startup, after a resume, or mid-afternoon.
pub fn expected_action(config: &Config, now: NaiveDateTime) -> Option<Action> {
    expected_occurrence(config, now).map(|(_, action)| action)
}

/// As [`expected_action`], but also reports when that occurrence was. The instant matters: it is
/// what decides whether a manual switch the user made afterwards still stands.
pub fn expected_occurrence(config: &Config, now: NaiveDateTime) -> Option<(NaiveDateTime, Action)> {
    if !config.schedule_enabled {
        return None;
    }

    let today = now.date();
    let mut latest: Option<(NaiveDateTime, usize, Action)> = None;

    for offset in 0..=SEARCH_DAYS {
        let Some(date) = today.checked_sub_days(Days::new(offset)) else {
            continue;
        };
        for (index, schedule) in config.schedules.iter().enumerate() {
            let Some(at) = occurrence_on(schedule, date) else {
                continue;
            };
            if at > now {
                continue;
            }
            let is_later = match latest {
                None => true,
                // Two schedules can share a minute; when they do, the later entry in the list
                // wins, so the result never depends on iteration order.
                Some((when, position, _)) => at > when || (at == when && index > position),
            };
            if is_later {
                latest = Some((at, index, schedule.action));
            }
        }
    }

    latest.map(|(at, _, action)| (at, action))
}

/// The moment of the next occurrence strictly after `now`, or `None` when nothing is scheduled.
/// Used only to arm the timer: the action to apply is decided by [`expected_action`] on waking.
pub fn next_event_at(config: &Config, now: NaiveDateTime) -> Option<NaiveDateTime> {
    next_occurrence(config, now).map(|(at, _)| at)
}

/// The next occurrence strictly after `now`, with the action it will take. Used to arm the timer
/// and to tell the user what is coming.
pub fn next_occurrence(config: &Config, now: NaiveDateTime) -> Option<(NaiveDateTime, Action)> {
    if !config.schedule_enabled {
        return None;
    }

    let today = now.date();
    let mut soonest: Option<(NaiveDateTime, usize, Action)> = None;

    for offset in 0..=SEARCH_DAYS {
        let Some(date) = today.checked_add_days(Days::new(offset)) else {
            continue;
        };
        for (index, schedule) in config.schedules.iter().enumerate() {
            let Some(at) = occurrence_on(schedule, date) else {
                continue;
            };
            if at <= now {
                continue;
            }
            let is_sooner = match soonest {
                None => true,
                Some((when, position, _)) => at < when || (at == when && index > position),
            };
            if is_sooner {
                soonest = Some((at, index, schedule.action));
            }
        }
    }

    soonest.map(|(at, _, action)| (at, action))
}

/// Whether the mode has to be written: only when the schedule has an opinion and the current
/// mode disagrees with it. `Rapid` counts as "conservation off" and is never overwritten, so a
/// manual Rapid Charge or a manual toggle is not fought until the next occurrence.
pub fn requires_change(expected: Option<Action>, current: ChargingMode) -> bool {
    match expected {
        Some(action) => action.conservation() != current.is_conservation(),
        None => false,
    }
}

/// Resolves a wall-clock time to a real instant.
///
/// During a spring-forward gap the requested time does not exist; the ladder walks forward to the
/// first instant that does, so such an occurrence fires as the clock jumps past it. During a
/// fall-back repeat the earliest interpretation is used.
pub fn instant(at: NaiveDateTime) -> Option<DateTime<Local>> {
    const LADDER_MINUTES: [i64; 7] = [0, 1, 5, 15, 30, 60, 120];
    LADDER_MINUTES.iter().find_map(|minutes| {
        let candidate = at.checked_add_signed(TimeDelta::minutes(*minutes))?;
        Local.from_local_datetime(&candidate).earliest()
    })
}

/// Whether a manual switch still stands: the user changed the mode after the occurrence that
/// would otherwise drive a write, so the schedule keeps out of the way until the next one.
pub fn manual_override_stands(
    occurrence_at: NaiveDateTime,
    manual_at: Option<NaiveDateTime>,
) -> bool {
    matches!(manual_at, Some(manual) if manual >= occurrence_at)
}

/// The instant this schedule fires on `date`, when the date's weekday is selected.
fn occurrence_on(schedule: &Schedule, date: NaiveDate) -> Option<NaiveDateTime> {
    if !schedule.enabled {
        return None;
    }
    let (hour, minute) = schedule.time_parts()?;
    if !schedule
        .days
        .contains(&Day::from_weekday(Datelike::weekday(&date)))
    {
        return None;
    }
    date.and_hms_opt(hour, minute, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY: (i32, u32, u32) = (2026, 9, 14);
    const TUESDAY: (i32, u32, u32) = (2026, 9, 15);
    const WEDNESDAY: (i32, u32, u32) = (2026, 9, 16);
    const SUNDAY: (i32, u32, u32) = (2026, 9, 20);
    const NEXT_MONDAY: (i32, u32, u32) = (2026, 9, 21);

    fn at(date: (i32, u32, u32), hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2)
            .expect("valid date")
            .and_hms_opt(hour, minute, 0)
            .expect("valid time")
    }

    fn every_day() -> Vec<Day> {
        Day::ALL.to_vec()
    }

    fn schedule(time: &str, days: Vec<Day>, action: Action) -> Schedule {
        Schedule::new(format!("s{time}"), time, days, action)
    }

    fn with_schedules(schedules: Vec<Schedule>) -> Config {
        Config {
            schedules,
            ..Config::default()
        }
    }

    /// The example from the project brief: weekdays 05:00 off, 09:00 on.
    fn commute_config() -> Config {
        let weekdays = vec![
            Day::Monday,
            Day::Tuesday,
            Day::Wednesday,
            Day::Thursday,
            Day::Friday,
        ];
        with_schedules(vec![
            schedule("05:00", weekdays.clone(), Action::ConservationOff),
            schedule("09:00", weekdays, Action::ConservationOn),
        ])
    }

    #[test]
    fn next_event_finds_the_next_occurrence() {
        let config = commute_config();

        assert_eq!(
            next_event_at(&config, at(MONDAY, 4, 0)),
            Some(at(MONDAY, 5, 0))
        );
        assert_eq!(
            next_event_at(&config, at(MONDAY, 6, 0)),
            Some(at(MONDAY, 9, 0))
        );
        assert_eq!(
            next_event_at(&config, at(MONDAY, 10, 0)),
            Some(at(TUESDAY, 5, 0)),
            "after the last event of the day, the next one is tomorrow"
        );
        assert_eq!(
            next_event_at(&config, at(SUNDAY, 23, 0)),
            Some(at(NEXT_MONDAY, 5, 0)),
            "the week wraps around"
        );
    }

    #[test]
    fn expected_action_is_the_most_recent_occurrence() {
        let config = commute_config();

        assert_eq!(
            expected_action(&config, at(MONDAY, 7, 30)),
            Some(Action::ConservationOff),
            "05:00 has passed, 09:00 has not"
        );
        assert_eq!(
            expected_action(&config, at(MONDAY, 10, 0)),
            Some(Action::ConservationOn)
        );
        assert_eq!(
            expected_action(&config, at(MONDAY, 4, 0)),
            Some(Action::ConservationOn),
            "before the first event of the day the previous day's state still stands"
        );
    }

    #[test]
    fn an_event_at_exactly_now_counts_as_fired() {
        let config = commute_config();

        assert_eq!(
            expected_action(&config, at(MONDAY, 5, 0)),
            Some(Action::ConservationOff)
        );
        assert_eq!(
            next_event_at(&config, at(MONDAY, 5, 0)),
            Some(at(MONDAY, 9, 0)),
            "and is not offered again"
        );
    }

    #[test]
    fn weekday_filtering_respects_the_selected_days() {
        let monday_only = with_schedules(vec![schedule(
            "05:00",
            vec![Day::Monday],
            Action::ConservationOn,
        )]);

        assert_eq!(
            expected_action(&monday_only, at(MONDAY, 12, 0)),
            Some(Action::ConservationOn)
        );
        assert_eq!(
            expected_action(&monday_only, at(WEDNESDAY, 12, 0)),
            Some(Action::ConservationOn),
            "Monday's state still stands on Wednesday"
        );
        assert_eq!(
            next_event_at(&monday_only, at(WEDNESDAY, 12, 0)),
            Some(at(NEXT_MONDAY, 5, 0)),
            "and the next event is next Monday"
        );
        assert_eq!(
            next_event_at(&monday_only, at(MONDAY, 6, 0)),
            Some(at(NEXT_MONDAY, 5, 0)),
            "a weekly schedule repeats exactly one week later"
        );
    }

    #[test]
    fn schedules_crossing_midnight_work() {
        let config = with_schedules(vec![
            schedule("23:30", every_day(), Action::ConservationOff),
            schedule("00:15", every_day(), Action::ConservationOn),
        ]);

        assert_eq!(
            expected_action(&config, at(TUESDAY, 0, 30)),
            Some(Action::ConservationOn),
            "just after midnight the 00:15 event has fired"
        );
        assert_eq!(
            expected_action(&config, at(TUESDAY, 0, 10)),
            Some(Action::ConservationOff),
            "before it, last night's 23:30 still stands"
        );
        assert_eq!(
            next_event_at(&config, at(TUESDAY, 23, 40)),
            Some(at(WEDNESDAY, 0, 15))
        );
    }

    #[test]
    fn multiple_schedules_in_one_day_use_the_latest() {
        let config = with_schedules(vec![
            schedule("05:00", every_day(), Action::ConservationOff),
            schedule("12:00", every_day(), Action::ConservationOn),
            schedule("18:00", every_day(), Action::ConservationOff),
        ]);

        assert_eq!(
            expected_action(&config, at(MONDAY, 11, 0)),
            Some(Action::ConservationOff)
        );
        assert_eq!(
            expected_action(&config, at(MONDAY, 13, 0)),
            Some(Action::ConservationOn)
        );
        assert_eq!(
            expected_action(&config, at(MONDAY, 19, 0)),
            Some(Action::ConservationOff)
        );
    }

    #[test]
    fn disabled_schedules_never_fire() {
        let mut disabled = schedule("05:00", every_day(), Action::ConservationOn);
        disabled.enabled = false;
        let config = with_schedules(vec![
            disabled.clone(),
            schedule("09:00", every_day(), Action::ConservationOff),
        ]);

        assert_eq!(
            expected_action(&config, at(MONDAY, 6, 0)),
            Some(Action::ConservationOff),
            "the disabled 05:00 must not fire, so yesterday's 09:00 still stands"
        );
        assert_eq!(
            next_event_at(&config, at(MONDAY, 6, 0)),
            Some(at(MONDAY, 9, 0)),
            "the disabled 05:00 is skipped"
        );

        let only_disabled = with_schedules(vec![disabled]);
        assert_eq!(expected_action(&only_disabled, at(MONDAY, 12, 0)), None);
        assert_eq!(next_event_at(&only_disabled, at(MONDAY, 12, 0)), None);
    }

    #[test]
    fn the_master_switch_disables_everything() {
        let mut config = commute_config();
        config.schedule_enabled = false;

        assert_eq!(expected_action(&config, at(MONDAY, 10, 0)), None);
        assert_eq!(next_event_at(&config, at(MONDAY, 10, 0)), None);
    }

    #[test]
    fn no_schedules_means_no_opinion_and_no_wakeups() {
        let config = with_schedules(vec![]);

        assert_eq!(expected_action(&config, at(MONDAY, 12, 0)), None);
        assert_eq!(next_event_at(&config, at(MONDAY, 12, 0)), None);
        assert!(!requires_change(
            expected_action(&config, at(MONDAY, 12, 0)),
            ChargingMode::Conservation
        ));
    }

    #[test]
    fn startup_after_a_scheduled_time_reconciles_immediately() {
        let config = commute_config();
        let started_at = at(MONDAY, 7, 30);
        let expected = expected_action(&config, started_at);

        assert_eq!(expected, Some(Action::ConservationOff));
        assert!(
            requires_change(expected, ChargingMode::Conservation),
            "the laptop was switched on after 05:00 while conservation was still on"
        );
        assert!(!requires_change(expected, ChargingMode::Normal));
    }

    #[test]
    fn waking_after_a_missed_event_reconciles_to_the_latest_state() {
        let config = commute_config();
        let expected = expected_action(&config, at(MONDAY, 10, 0));

        assert_eq!(
            expected,
            Some(Action::ConservationOn),
            "the laptop slept through 05:00 and 09:00 and woke at 10:00"
        );
        assert!(requires_change(expected, ChargingMode::Normal));
    }

    #[test]
    fn rapid_charge_is_left_alone() {
        assert!(!requires_change(
            Some(Action::ConservationOff),
            ChargingMode::Rapid
        ));
        assert!(!requires_change(
            Some(Action::ConservationOff),
            ChargingMode::Normal
        ));
        assert!(requires_change(
            Some(Action::ConservationOn),
            ChargingMode::Rapid
        ));
        assert!(!requires_change(
            Some(Action::ConservationOn),
            ChargingMode::Conservation
        ));
    }

    #[test]
    fn duplicate_schedules_are_harmless() {
        let duplicate = schedule("05:00", every_day(), Action::ConservationOn);
        let config = with_schedules(vec![duplicate.clone(), duplicate]);

        assert_eq!(
            expected_action(&config, at(MONDAY, 6, 0)),
            Some(Action::ConservationOn)
        );
        assert_eq!(
            next_event_at(&config, at(MONDAY, 4, 0)),
            Some(at(MONDAY, 5, 0))
        );
    }

    #[test]
    fn conflicting_schedules_at_the_same_minute_resolve_deterministically() {
        let config = with_schedules(vec![
            schedule("05:00", every_day(), Action::ConservationOn),
            schedule("05:00", every_day(), Action::ConservationOff),
        ]);

        assert_eq!(
            expected_action(&config, at(MONDAY, 6, 0)),
            Some(Action::ConservationOff),
            "the later entry in the list wins"
        );
    }

    #[test]
    fn a_hand_edited_bad_time_never_fires() {
        let config = with_schedules(vec![
            schedule("5:00", every_day(), Action::ConservationOn),
            schedule("25:00", every_day(), Action::ConservationOn),
        ]);

        assert_eq!(expected_action(&config, at(MONDAY, 12, 0)), None);
        assert_eq!(next_event_at(&config, at(MONDAY, 12, 0)), None);
    }

    #[test]
    fn occurrence_times_are_reported_for_the_override_rule() {
        let config = commute_config();

        assert_eq!(
            expected_occurrence(&config, at(MONDAY, 7, 30)),
            Some((at(MONDAY, 5, 0), Action::ConservationOff))
        );
        assert_eq!(
            next_occurrence(&config, at(MONDAY, 7, 30)),
            Some((at(MONDAY, 9, 0), Action::ConservationOn))
        );
    }

    #[test]
    fn a_manual_switch_stands_until_the_next_occurrence() {
        let occurrence = at(MONDAY, 9, 0);

        assert!(
            !manual_override_stands(occurrence, None),
            "no manual switch means the schedule drives"
        );
        assert!(
            manual_override_stands(occurrence, Some(at(MONDAY, 12, 0))),
            "switched by hand after the 09:00 occurrence: leave it alone"
        );
        assert!(
            !manual_override_stands(occurrence, Some(at(MONDAY, 8, 0))),
            "switched by hand before it: the 09:00 occurrence supersedes"
        );
        assert!(
            manual_override_stands(occurrence, Some(occurrence)),
            "the same minute counts as the user's later action"
        );
    }

    #[test]
    fn wall_clock_times_resolve_to_instants() {
        let resolved = instant(at(MONDAY, 5, 0)).expect("a normal local time resolves");
        assert_eq!(
            resolved.naive_local(),
            at(MONDAY, 5, 0),
            "resolution must not shift the wall-clock time"
        );

        let now = Local::now();
        let resolved = instant(now.naive_local()).expect("the current time resolves");
        assert!(
            (resolved.with_timezone(&chrono::Utc) - now.with_timezone(&chrono::Utc))
                .num_seconds()
                .abs()
                <= 1,
            "resolving now should land on now"
        );
    }
}
