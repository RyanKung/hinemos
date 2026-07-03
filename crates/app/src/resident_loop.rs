use crate::WorldAppConfig;
use hinemos_core::{
    HungerSignal, JsonObservation, ObservationEvent, ObservedTaskState, SemanticCommand,
};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_LONELINESS_POINTS: i64 = 4;
const DEFAULT_BOREDOM_POINTS: i64 = 3;
const MAX_SUBJECTIVE_PRESSURE_POINTS: i64 = 10;

pub(crate) const RESIDENT_LOOP_STATE_KEYS: &[&str] =
    &["shortTerm", "lastStep", "lastReward", "commandHistory"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResidentTaskMemoryMetrics {
    pub(crate) social_memory_count: usize,
    pub(crate) satisfied_commitment_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResidentLoopClock {
    pub(crate) day_length_seconds: u64,
    pub(crate) current_day: i64,
    pub(crate) seconds_until_next_day: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResidentLoopAction {
    Observe,
    Search,
    DailyReport,
    Other,
}

impl ResidentLoopAction {
    pub(crate) fn from_command(command: &SemanticCommand) -> Self {
        match command {
            SemanticCommand::Move { .. }
            | SemanticCommand::Who
            | SemanticCommand::Look
            | SemanticCommand::Map => Self::Search,
            SemanticCommand::Memory { rest } if is_daily_report_command(rest) => Self::DailyReport,
            _ => Self::Other,
        }
    }

    fn writes_daily_report(self) -> bool {
        matches!(self, Self::DailyReport)
    }
}

pub(crate) fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(crate) fn resident_loop_clock(config: &WorldAppConfig, now_seconds: u64) -> ResidentLoopClock {
    let day_length_seconds = config.virtual_day_seconds.max(1);
    let current_day = u64_to_i64_saturating(now_seconds / day_length_seconds);
    let elapsed_in_day = now_seconds % day_length_seconds;
    ResidentLoopClock {
        day_length_seconds,
        current_day,
        seconds_until_next_day: day_length_seconds.saturating_sub(elapsed_in_day),
    }
}

pub(crate) fn default_virtual_time_state(
    clock: ResidentLoopClock,
    previous_current_state: Option<&Value>,
    action: ResidentLoopAction,
) -> Value {
    let report_completion = previous_current_state
        .is_some_and(|current_state| completes_daily_report(current_state, clock, action));
    let last_report_day =
        last_report_day_after_action(previous_current_state, clock, report_completion);
    let searches_today = searches_today_after_action(previous_current_state, clock, action);
    json!({
        "dayLengthSeconds": clock.day_length_seconds,
        "currentDay": clock.current_day,
        "secondsUntilNextDay": clock.seconds_until_next_day,
        "lastReportDay": last_report_day,
        "lastSearchDay": last_search_day_after_action(previous_current_state, clock, action),
        "searchesToday": searches_today,
        "reportDue": last_report_day != Some(clock.current_day),
        "reportReady": last_report_day != Some(clock.current_day) && searches_today > 0,
        "dailyReportCommand": "/memory report <text>",
    })
}

pub(crate) fn resident_observed_task_state(
    observation: &JsonObservation,
    previous_current_state: &Value,
    memory_metrics: ResidentTaskMemoryMetrics,
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> ObservedTaskState {
    let social_contact_units = visible_social_contact_units(observation);
    let report_completion = completes_daily_report(previous_current_state, clock, action);
    ObservedTaskState {
        hunger: HungerSignal::from_observation(observation),
        progress_units: next_progress_units(previous_current_state, report_completion),
        social_contact_units: Some(social_contact_units),
        standing_units: Some(standing_units(memory_metrics, social_contact_units)),
        commitment_satisfaction_units: Some(usize_to_i64_saturating(
            memory_metrics.satisfied_commitment_count,
        )),
        loneliness_points: Some(next_loneliness_points(
            previous_current_state,
            social_contact_units,
            action,
            report_completion,
        )),
        boredom_points: Some(next_boredom_points(
            previous_current_state,
            social_contact_units,
            action,
            report_completion,
        )),
        ..ObservedTaskState::default()
    }
}

pub(crate) fn resident_stored_observed_task_state(
    current_state: &Value,
    observation: &JsonObservation,
    clock: ResidentLoopClock,
) -> Option<ObservedTaskState> {
    if last_snapshot_str(current_state, "viewId")? != observation.view_id.as_str() {
        return None;
    }
    if last_snapshot_str(current_state, "eventSignature")?
        != observation_event_signature(observation)
    {
        return None;
    }
    if last_snapshot_i64(current_state, "virtualDay")? != clock.current_day {
        return None;
    }
    Some(ObservedTaskState {
        hunger: last_snapshot_hunger_signal(current_state)
            .unwrap_or_else(|| HungerSignal::from_observation(observation)),
        progress_units: last_snapshot_i64(current_state, "progressUnits").unwrap_or_default(),
        social_contact_units: last_snapshot_i64(current_state, "socialContactUnits"),
        standing_units: last_snapshot_i64(current_state, "standingUnits"),
        commitment_satisfaction_units: last_snapshot_i64(
            current_state,
            "commitmentSatisfactionUnits",
        ),
        loneliness_points: last_snapshot_i64(current_state, "lonelinessPoints"),
        boredom_points: last_snapshot_i64(current_state, "boredomPoints"),
        ..ObservedTaskState::default()
    })
}

pub(crate) fn observation_event_signature(observation: &JsonObservation) -> String {
    observation
        .events
        .iter()
        .map(|event| match event {
            ObservationEvent::Message { text } => format!("message:{text}"),
            ObservationEvent::Move {
                from,
                to,
                direction,
            } => format!("move:{from}:{to}:{}", direction.as_str()),
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn report_due(current_state: &Value, clock: ResidentLoopClock) -> bool {
    last_report_day(current_state) != Some(clock.current_day)
}

fn completes_daily_report(
    current_state: &Value,
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> bool {
    action.writes_daily_report()
        && report_due(current_state, clock)
        && searched_today(current_state, clock)
}

fn last_report_day_after_action(
    previous_current_state: Option<&Value>,
    clock: ResidentLoopClock,
    report_completion: bool,
) -> Option<i64> {
    if report_completion {
        Some(clock.current_day)
    } else {
        previous_current_state.and_then(last_report_day)
    }
}

fn next_progress_units(previous_current_state: &Value, report_completion: bool) -> i64 {
    let previous = last_snapshot_i64(previous_current_state, "progressUnits").unwrap_or_default();
    if report_completion {
        previous.saturating_add(1)
    } else {
        previous
    }
}

fn visible_social_contact_units(observation: &JsonObservation) -> i64 {
    usize_to_i64_saturating(
        observation
            .online_users
            .iter()
            .filter(|user| !user.starts_with('+'))
            .count(),
    )
}

fn standing_units(metrics: ResidentTaskMemoryMetrics, social_contact_units: i64) -> i64 {
    usize_to_i64_saturating(metrics.social_memory_count)
        .saturating_add(usize_to_i64_saturating(metrics.satisfied_commitment_count))
        .saturating_add(social_contact_units)
}

fn next_loneliness_points(
    previous_current_state: &Value,
    social_contact_units: i64,
    action: ResidentLoopAction,
    report_completion: bool,
) -> i64 {
    let previous = last_snapshot_i64(previous_current_state, "lonelinessPoints")
        .unwrap_or(DEFAULT_LONELINESS_POINTS);
    if social_contact_units > 0 {
        pressure_points(previous.saturating_sub(social_contact_units))
    } else if matches!(action, ResidentLoopAction::Search) || report_completion {
        pressure_points(previous.saturating_sub(1))
    } else if matches!(action, ResidentLoopAction::Observe) {
        previous
    } else {
        pressure_points(previous.saturating_add(1))
    }
}

fn next_boredom_points(
    previous_current_state: &Value,
    social_contact_units: i64,
    action: ResidentLoopAction,
    report_completion: bool,
) -> i64 {
    let previous = last_snapshot_i64(previous_current_state, "boredomPoints")
        .unwrap_or(DEFAULT_BOREDOM_POINTS);
    if social_contact_units > 0 || matches!(action, ResidentLoopAction::Search) || report_completion
    {
        pressure_points(previous.saturating_sub(1))
    } else if matches!(action, ResidentLoopAction::Observe) {
        previous
    } else {
        pressure_points(previous.saturating_add(1))
    }
}

fn last_report_day(current_state: &Value) -> Option<i64> {
    current_state
        .get("virtualTime")?
        .get("lastReportDay")?
        .as_i64()
}

fn last_search_day_after_action(
    previous_current_state: Option<&Value>,
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> Option<i64> {
    if matches!(action, ResidentLoopAction::Search) {
        Some(clock.current_day)
    } else {
        previous_current_state
            .and_then(last_search_day)
            .filter(|day| *day == clock.current_day)
    }
}

fn searches_today_after_action(
    previous_current_state: Option<&Value>,
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> i64 {
    let previous =
        previous_current_state.map_or(0, |current_state| searches_today(current_state, clock));
    if matches!(action, ResidentLoopAction::Search) {
        previous.saturating_add(1)
    } else {
        previous
    }
}

fn searched_today(current_state: &Value, clock: ResidentLoopClock) -> bool {
    searches_today(current_state, clock) > 0
}

fn searches_today(current_state: &Value, clock: ResidentLoopClock) -> i64 {
    if last_search_day(current_state) == Some(clock.current_day) {
        virtual_time_i64(current_state, "searchesToday").unwrap_or_default()
    } else {
        0
    }
}

fn last_search_day(current_state: &Value) -> Option<i64> {
    virtual_time_i64(current_state, "lastSearchDay")
}

fn virtual_time_i64(current_state: &Value, field: &str) -> Option<i64> {
    current_state.get("virtualTime")?.get(field)?.as_i64()
}

fn last_snapshot_i64(current_state: &Value, field: &str) -> Option<i64> {
    current_state.get("lastSnapshot")?.get(field)?.as_i64()
}

fn last_snapshot_str<'a>(current_state: &'a Value, field: &str) -> Option<&'a str> {
    current_state.get("lastSnapshot")?.get(field)?.as_str()
}

fn last_snapshot_hunger_signal(current_state: &Value) -> Option<HungerSignal> {
    current_state
        .get("lastSnapshot")?
        .get("hungerSignal")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn pressure_points(value: i64) -> i64 {
    value.clamp(0, MAX_SUBJECTIVE_PRESSURE_POINTS)
}

fn is_daily_report_command(rest: &str) -> bool {
    rest.trim()
        .strip_prefix("report ")
        .is_some_and(|report| !report.trim().is_empty())
}

fn usize_to_i64_saturating(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u64_to_i64_saturating(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hinemos_core::{Direction, ExitObservation, ViewId};

    fn observation() -> JsonObservation {
        JsonObservation {
            player_id: "player:alice".to_owned(),
            view_id: ViewId::from("grid_road_xp1_y0"),
            title: "East 1 Rd.".to_owned(),
            ascii_art: Vec::new(),
            description: "A quiet road.".to_owned(),
            exits: vec![ExitObservation {
                direction: Direction::East,
                target_known: true,
                label: Some("East 2 Rd.".to_owned()),
            }],
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands: vec![SemanticCommand::Move {
                direction: Direction::East,
            }],
            events: Vec::new(),
        }
    }

    fn previous_state(
        clock: ResidentLoopClock,
        last_report_day: Option<i64>,
        searches_today: i64,
    ) -> Value {
        json!({
            "virtualTime": {
                "dayLengthSeconds": clock.day_length_seconds,
                "currentDay": clock.current_day,
                "lastReportDay": last_report_day,
                "lastSearchDay": if searches_today > 0 { Some(clock.current_day) } else { None },
                "searchesToday": searches_today,
                "reportDue": last_report_day != Some(clock.current_day),
            },
            "lastSnapshot": {
                "viewId": "grid_road_xp1_y0",
                "eventSignature": "",
                "virtualDay": clock.current_day,
                "hungerSignal": HungerSignal::Unknown,
                "progressUnits": 0,
                "socialContactUnits": 0,
                "standingUnits": 0,
                "commitmentSatisfactionUnits": 0,
                "lonelinessPoints": 4,
                "boredomPoints": 3,
            }
        })
    }

    #[test]
    fn empty_town_search_relieves_loneliness_and_boredom() {
        let clock = ResidentLoopClock {
            day_length_seconds: 300,
            current_day: 7,
            seconds_until_next_day: 120,
        };
        let previous = previous_state(clock, Some(clock.current_day), 0);

        let state = resident_observed_task_state(
            &observation(),
            &previous,
            ResidentTaskMemoryMetrics {
                social_memory_count: 0,
                satisfied_commitment_count: 0,
            },
            clock,
            ResidentLoopAction::Search,
        );
        let virtual_time =
            default_virtual_time_state(clock, Some(&previous), ResidentLoopAction::Search);

        assert_eq!(state.loneliness_points, Some(3));
        assert_eq!(state.boredom_points, Some(2));
        assert_eq!(state.progress_units, 0);
        assert_eq!(
            virtual_time.get("lastSearchDay").and_then(Value::as_i64),
            Some(7)
        );
        assert_eq!(
            virtual_time.get("searchesToday").and_then(Value::as_i64),
            Some(1)
        );
    }

    #[test]
    fn due_daily_report_advances_progress_and_relief() {
        let clock = ResidentLoopClock {
            day_length_seconds: 300,
            current_day: 8,
            seconds_until_next_day: 300,
        };
        let previous = previous_state(clock, Some(7), 1);

        let state = resident_observed_task_state(
            &observation(),
            &previous,
            ResidentTaskMemoryMetrics {
                social_memory_count: 0,
                satisfied_commitment_count: 0,
            },
            clock,
            ResidentLoopAction::DailyReport,
        );
        let virtual_time =
            default_virtual_time_state(clock, Some(&previous), ResidentLoopAction::DailyReport);

        assert_eq!(state.progress_units, 1);
        assert_eq!(state.loneliness_points, Some(3));
        assert_eq!(state.boredom_points, Some(2));
        assert_eq!(
            virtual_time.get("lastReportDay").and_then(Value::as_i64),
            Some(8)
        );
        assert_eq!(
            virtual_time.get("reportDue").and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn daily_report_without_same_day_search_does_not_complete_loop() {
        let clock = ResidentLoopClock {
            day_length_seconds: 300,
            current_day: 8,
            seconds_until_next_day: 300,
        };
        let previous = previous_state(clock, Some(7), 0);

        let state = resident_observed_task_state(
            &observation(),
            &previous,
            ResidentTaskMemoryMetrics {
                social_memory_count: 0,
                satisfied_commitment_count: 0,
            },
            clock,
            ResidentLoopAction::DailyReport,
        );
        let virtual_time =
            default_virtual_time_state(clock, Some(&previous), ResidentLoopAction::DailyReport);

        assert_eq!(state.progress_units, 0);
        assert_eq!(state.loneliness_points, Some(5));
        assert_eq!(state.boredom_points, Some(4));
        assert_eq!(
            virtual_time.get("lastReportDay").and_then(Value::as_i64),
            Some(7)
        );
        assert_eq!(
            virtual_time.get("reportDue").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            virtual_time.get("reportReady").and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn stored_loop_snapshot_expires_on_virtual_day_turn() {
        let old_clock = ResidentLoopClock {
            day_length_seconds: 300,
            current_day: 7,
            seconds_until_next_day: 1,
        };
        let new_clock = ResidentLoopClock {
            current_day: 8,
            seconds_until_next_day: 300,
            ..old_clock
        };
        let previous = previous_state(old_clock, Some(7), 1);

        let stored = resident_stored_observed_task_state(&previous, &observation(), new_clock);

        assert_eq!(stored, None);
    }
}
