//! Task and reward model over existing Hinemos observations and commands.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    BadgeAction, BuildAction, EntityRef, InboxAction, JsonObservation, ParcelAction,
    ParcelBadgeAction, ParcelDeskAction, ParcelMailingListAction, ParcelRouteAction,
    ParcelShiftAction, ParcelStaffAction, ParcelWorkAction, PayAction, SemanticCommand,
    SettingsAction, agent_task_match::command_matches_template,
};

/// Persistent controller state for one task objective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMode {
    /// Human-authored objective the controller optimizes.
    pub objective: String,
    /// Reward function used to evaluate observed state transitions.
    pub reward: RewardSpec,
    /// Hard constraints checked before a candidate command can be executed.
    pub constraints: TaskConstraints,
    /// Last observed world snapshot for delta evaluation.
    pub last_snapshot: Option<TaskSnapshot>,
    /// Validated command history emitted by the controller.
    pub command_history: Vec<TaskCommandRecord>,
}

impl TaskMode {
    /// Creates the default resident task for a Hinemos session.
    #[must_use]
    pub fn resident(username: &str) -> Self {
        Self {
            objective: resident_objective(username),
            reward: RewardSpec::default(),
            constraints: TaskConstraints::default(),
            last_snapshot: None,
            command_history: Vec::new(),
        }
    }

    /// Creates task mode with the default reward and constraint policy.
    ///
    /// Pre: `objective` is authored outside the Hinemos protocol.
    /// Post: no world-visible command has been emitted.
    pub fn new(objective: impl Into<String>) -> Result<Self, TaskModeError> {
        let objective = objective.into();
        if objective.trim().is_empty() {
            return Err(TaskModeError::EmptyObjective);
        }
        Ok(Self {
            objective,
            reward: RewardSpec::default(),
            constraints: TaskConstraints::default(),
            last_snapshot: None,
            command_history: Vec::new(),
        })
    }

    /// Replaces the reward function.
    #[must_use]
    pub fn with_reward(mut self, reward: RewardSpec) -> Self {
        self.reward = reward;
        self
    }

    /// Replaces task constraints.
    #[must_use]
    pub fn with_constraints(mut self, constraints: TaskConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Builds a task snapshot from a Hinemos observation plus observed task meters.
    ///
    /// The snapshot carries only server-visible observations and controller-owned
    /// task progress. It does not read model private reasoning.
    #[must_use]
    pub fn snapshot(
        &self,
        observation: &JsonObservation,
        observed: ObservedTaskState,
    ) -> TaskSnapshot {
        TaskSnapshot::from_observation(observation, observed)
    }

    /// Validates a candidate command against the current observation and task constraints.
    pub fn validate_command(
        &self,
        snapshot: &TaskSnapshot,
        command: SemanticCommand,
    ) -> Result<TaskCommand, TaskCommandError> {
        let line = command_line(&command).ok_or(TaskCommandError::UnrenderableCommand)?;
        if command_line_has_line_break(&line) {
            return Err(TaskCommandError::MultilineCommand);
        }
        if command_line_leaks_task_protocol(&line) {
            return Err(TaskCommandError::TaskProtocolLeak);
        }
        if !command_is_available(&command, &snapshot.available_commands) {
            return Err(TaskCommandError::CommandNotAvailable);
        }
        if self.constraints.hunger.requires_recovery(snapshot.hunger)
            && !command_is_hunger_recovery(&command)
        {
            return Err(TaskCommandError::HungerRequiresRecovery);
        }
        Ok(TaskCommand { command, line })
    }

    /// Evaluates one observed transition after a validated command has executed.
    #[must_use]
    pub fn evaluate_step(
        &self,
        before: &TaskSnapshot,
        command: TaskCommand,
        after: TaskSnapshot,
    ) -> TaskStepEvaluation {
        let mark_delta = optional_delta(before.total_mark(), after.total_mark());
        let progress_delta = after.progress_units.saturating_sub(before.progress_units);
        let social_contact_delta =
            optional_delta(before.social_contact_units, after.social_contact_units);
        let standing_delta = optional_delta(before.standing_units, after.standing_units);
        let commitment_satisfaction_delta = optional_delta(
            before.commitment_satisfaction_units,
            after.commitment_satisfaction_units,
        );
        let loneliness_relief_delta =
            optional_delta(after.loneliness_points, before.loneliness_points);
        let boredom_relief_delta = optional_delta(after.boredom_points, before.boredom_points);
        let reward = weighted_reward(&[
            (self.reward.mark_delta_weight, mark_delta),
            (self.reward.progress_delta_weight, progress_delta),
            (
                self.reward.social_contact_delta_weight,
                social_contact_delta,
            ),
            (self.reward.standing_delta_weight, standing_delta),
            (
                self.reward.commitment_satisfaction_delta_weight,
                commitment_satisfaction_delta,
            ),
            (
                self.reward.loneliness_relief_delta_weight,
                loneliness_relief_delta,
            ),
            (
                self.reward.boredom_relief_delta_weight,
                boredom_relief_delta,
            ),
        ]);

        TaskStepEvaluation {
            command,
            before: before.clone(),
            after,
            mark_delta,
            progress_delta,
            social_contact_delta,
            standing_delta,
            commitment_satisfaction_delta,
            loneliness_relief_delta,
            boredom_relief_delta,
            reward,
        }
    }

    /// Records a completed task step and advances the last snapshot.
    pub fn record_step(&mut self, evaluation: TaskStepEvaluation) {
        let TaskStepEvaluation {
            command,
            after,
            reward,
            ..
        } = evaluation;
        self.command_history.push(TaskCommandRecord {
            command_line: command.line,
            reward,
            snapshot: after.clone(),
        });
        self.last_snapshot = Some(after);
    }

    /// Returns the world-visible command transcript emitted by this task.
    #[must_use]
    pub fn command_transcript(&self) -> Vec<&str> {
        self.command_history
            .iter()
            .map(|record| record.command_line.as_str())
            .collect()
    }
}

fn resident_objective(username: &str) -> String {
    let name = username.trim();
    if name.is_empty() {
        "Search the town for residents, form useful relationships, reduce loneliness and boredom, and write a daily report when the virtual day turns."
            .to_owned()
    } else {
        format!(
            "As {name}, search the town for residents, form useful relationships, reduce loneliness and boredom, and write a daily report when the virtual day turns."
        )
    }
}

/// Reward weights for observed task deltas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardSpec {
    /// Reward multiplier for delta in usable MARK plus banked MARK.
    pub mark_delta_weight: i64,
    /// Reward multiplier for controller-owned progress units.
    pub progress_delta_weight: i64,
    /// Reward multiplier for observed social-contact gains.
    pub social_contact_delta_weight: i64,
    /// Reward multiplier for observed standing or reputation gains.
    pub standing_delta_weight: i64,
    /// Reward multiplier for satisfying visible commitments.
    pub commitment_satisfaction_delta_weight: i64,
    /// Reward multiplier for reducing loneliness pressure.
    pub loneliness_relief_delta_weight: i64,
    /// Reward multiplier for reducing boredom pressure.
    pub boredom_relief_delta_weight: i64,
}

impl Default for RewardSpec {
    fn default() -> Self {
        Self {
            mark_delta_weight: 0,
            progress_delta_weight: 10,
            social_contact_delta_weight: 4,
            standing_delta_weight: 6,
            commitment_satisfaction_delta_weight: 5,
            loneliness_relief_delta_weight: 3,
            boredom_relief_delta_weight: 2,
        }
    }
}

/// Constraint policy applied before command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskConstraints {
    /// Hunger policy for candidate commands.
    pub hunger: HungerPolicy,
}

impl Default for TaskConstraints {
    fn default() -> Self {
        Self {
            hunger: HungerPolicy::Ignore,
        }
    }
}

/// How the task controller treats observed hunger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HungerPolicy {
    /// Do not constrain candidate commands by hunger.
    Ignore,
    /// When the observation says the player is gated by hunger, allow only recovery commands.
    RequireRecoveryWhenGated,
}

impl HungerPolicy {
    fn requires_recovery(self, hunger: HungerSignal) -> bool {
        matches!(self, Self::RequireRecoveryWhenGated) && hunger.requires_recovery()
    }
}

/// Hunger state inferred from existing Hinemos observations or commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HungerSignal {
    /// The controller has not observed hunger state.
    Unknown,
    /// The player is not currently constrained by hunger.
    Clear,
    /// Hunger is near a gate but the server has not blocked commands.
    NearGate,
    /// Hunger blocks ordinary commands and the player can recover with food.
    GatedCanBuyFood,
    /// Hunger blocks ordinary commands and the player must earn before buying food.
    GatedNeedsWork,
}

impl HungerSignal {
    /// Infers hunger from world-visible event text.
    #[must_use]
    pub fn from_observation(observation: &JsonObservation) -> Self {
        let mut gated = Self::Unknown;
        for event in &observation.events {
            let crate::ObservationEvent::Message { text } = event else {
                continue;
            };
            let text = text.to_ascii_lowercase();
            if text.contains("hungry and broke") {
                return Self::GatedNeedsWork;
            }
            if text.contains("too hungry") {
                gated = Self::GatedCanBuyFood;
            }
        }
        gated
    }

    fn requires_recovery(self) -> bool {
        matches!(self, Self::GatedCanBuyFood | Self::GatedNeedsWork)
    }
}

/// Controller-observed meters used to build a task snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedTaskState {
    /// MARK available in the player's wallet, when observed.
    pub usable_mark: Option<i64>,
    /// MARK deposited in the bank, when observed.
    pub bank_mark: Option<i64>,
    /// Hunger signal inferred from existing observations.
    pub hunger: HungerSignal,
    /// Monotonic task progress units tracked by the task runner.
    pub progress_units: i64,
    /// Observed useful contact count or contact score.
    pub social_contact_units: Option<i64>,
    /// Observed social standing or reputation score.
    pub standing_units: Option<i64>,
    /// Observed count or score for satisfied commitments.
    pub commitment_satisfaction_units: Option<i64>,
    /// Observed loneliness pressure; lower is better.
    pub loneliness_points: Option<i64>,
    /// Observed boredom pressure; lower is better.
    pub boredom_points: Option<i64>,
}

impl Default for ObservedTaskState {
    fn default() -> Self {
        Self {
            usable_mark: None,
            bank_mark: None,
            hunger: HungerSignal::Unknown,
            progress_units: 0,
            social_contact_units: None,
            standing_units: None,
            commitment_satisfaction_units: None,
            loneliness_points: None,
            boredom_points: None,
        }
    }
}

/// A controller snapshot built from one Hinemos observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    /// Player id from the observation.
    pub player_id: String,
    /// Current view id from the observation.
    pub view_id: String,
    /// Commands exposed by the current observation.
    pub available_commands: Vec<SemanticCommand>,
    /// MARK available in the player's wallet, when observed.
    pub usable_mark: Option<i64>,
    /// MARK deposited in the bank, when observed.
    pub bank_mark: Option<i64>,
    /// Current hunger signal.
    pub hunger: HungerSignal,
    /// Monotonic task progress units tracked by the controller.
    pub progress_units: i64,
    /// Observed useful contact count or contact score.
    pub social_contact_units: Option<i64>,
    /// Observed social standing or reputation score.
    pub standing_units: Option<i64>,
    /// Observed count or score for satisfied commitments.
    pub commitment_satisfaction_units: Option<i64>,
    /// Observed loneliness pressure; lower is better.
    pub loneliness_points: Option<i64>,
    /// Observed boredom pressure; lower is better.
    pub boredom_points: Option<i64>,
}

impl TaskSnapshot {
    /// Builds a snapshot from existing world observation data.
    #[must_use]
    pub fn from_observation(observation: &JsonObservation, observed: ObservedTaskState) -> Self {
        Self {
            player_id: observation.player_id.clone(),
            view_id: observation.view_id.clone(),
            available_commands: observation.available_commands.clone(),
            usable_mark: observed.usable_mark,
            bank_mark: observed.bank_mark,
            hunger: observed.hunger,
            progress_units: observed.progress_units,
            social_contact_units: observed.social_contact_units,
            standing_units: observed.standing_units,
            commitment_satisfaction_units: observed.commitment_satisfaction_units,
            loneliness_points: observed.loneliness_points,
            boredom_points: observed.boredom_points,
        }
    }

    /// Returns total observed MARK when at least one MARK source is known.
    #[must_use]
    pub fn total_mark(&self) -> Option<i64> {
        match (self.usable_mark, self.bank_mark) {
            (None, None) => None,
            (Some(usable), None) => Some(usable),
            (None, Some(bank)) => Some(bank),
            (Some(usable), Some(bank)) => usable.checked_add(bank),
        }
    }
}

/// A validated command the controller may send to Hinemos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCommand {
    /// Semantic command accepted by the current task snapshot.
    pub command: SemanticCommand,
    /// World-visible command line to send over the existing protocol.
    pub line: String,
}

impl TaskCommand {
    /// Returns the world-visible command line.
    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }
}

/// Evaluation of one validated command after observing the next state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStepEvaluation {
    /// Validated command that was executed.
    pub command: TaskCommand,
    /// Snapshot before execution.
    pub before: TaskSnapshot,
    /// Snapshot after execution.
    pub after: TaskSnapshot,
    /// Observed delta in total MARK.
    pub mark_delta: i64,
    /// Observed delta in task progress units.
    pub progress_delta: i64,
    /// Observed delta in useful social contact.
    pub social_contact_delta: i64,
    /// Observed delta in standing or reputation.
    pub standing_delta: i64,
    /// Observed delta in satisfied commitments.
    pub commitment_satisfaction_delta: i64,
    /// Positive value when loneliness pressure decreases.
    pub loneliness_relief_delta: i64,
    /// Positive value when boredom pressure decreases.
    pub boredom_relief_delta: i64,
    /// Weighted reward score.
    pub reward: i64,
}

/// One persisted command-history entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCommandRecord {
    /// World-visible command line sent to Hinemos.
    pub command_line: String,
    /// Weighted reward observed after the command.
    pub reward: i64,
    /// Snapshot reached after the command.
    pub snapshot: TaskSnapshot,
}

/// Errors constructing task mode.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskModeError {
    /// Empty objectives do not define a task.
    #[error("task objective must not be empty")]
    EmptyObjective,
}

/// Errors validating a candidate command.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskCommandError {
    /// Candidate command is not exposed by the current observation.
    #[error("candidate command is not available in the current observation")]
    CommandNotAvailable,
    /// Hunger currently permits only safe reads, movement, or room-gated commands.
    #[error("hunger constraint requires a safe or room-gated command")]
    HungerRequiresRecovery,
    /// Candidate tries to add a task-planning protocol to Hinemos.
    #[error("task mode must not emit plan/act or task-state protocol commands")]
    TaskProtocolLeak,
    /// Candidate renders to more than one Hinemos input line.
    #[error("candidate command must render to exactly one Hinemos input line")]
    MultilineCommand,
    /// Candidate command cannot be rendered as an existing Hinemos command line.
    #[error("candidate command cannot be rendered as a Hinemos command line")]
    UnrenderableCommand,
}

fn optional_delta(before: Option<i64>, after: Option<i64>) -> i64 {
    match (before, after) {
        (Some(before), Some(after)) => after.saturating_sub(before),
        _ => 0,
    }
}

fn weighted_reward(terms: &[(i64, i64)]) -> i64 {
    terms.iter().fold(0_i64, |total, (weight, delta)| {
        total.saturating_add(weight.saturating_mul(*delta))
    })
}

fn command_is_available(command: &SemanticCommand, available: &[SemanticCommand]) -> bool {
    available
        .iter()
        .any(|template| command_matches_template(command, template))
}

fn command_is_hunger_recovery(command: &SemanticCommand) -> bool {
    matches!(
        command,
        SemanticCommand::Move { .. }
            | SemanticCommand::Enter { .. }
            | SemanticCommand::Balance
            | SemanticCommand::Inventory
            | SemanticCommand::Mailbox
            | SemanticCommand::Memory { .. }
            | SemanticCommand::Help
            | SemanticCommand::Extension { .. }
    )
}

fn command_line_has_line_break(line: &str) -> bool {
    line.contains(['\r', '\n'])
}

fn command_line_leaks_task_protocol(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    lower.starts_with("/plan")
        || lower.starts_with("/act")
        || ((lower.contains("\"objective\"")
            || lower.contains("\"goal_state\"")
            || lower.contains("\"short_term\"")
            || lower.contains("\"long_term\""))
            && (line.contains('{') || line.contains('}')))
}

fn command_line(command: &SemanticCommand) -> Option<String> {
    let line = match command {
        SemanticCommand::Look => "/look".to_owned(),
        SemanticCommand::Map => "/map".to_owned(),
        SemanticCommand::Move { direction } => format!("/go {}", direction.as_str()),
        SemanticCommand::Enter { target } => format!("/enter {target}"),
        SemanticCommand::Inspect { target } => target_command("inspect", target),
        SemanticCommand::Read { target } => target_command("read", target),
        SemanticCommand::Take { target } => target_command("take", target),
        SemanticCommand::Talk { target } => target_command("talk", target),
        SemanticCommand::Agree { phrase } if phrase.is_empty() => "/agree".to_owned(),
        SemanticCommand::Agree { phrase } => format!("/agree {phrase}"),
        SemanticCommand::Say { text } => format!("/say {text}"),
        SemanticCommand::Mail { target, text } => format!("/mail {target} {text}"),
        SemanticCommand::Settings { action } => settings_line(action),
        SemanticCommand::Inbox { action } => inbox_line(action),
        SemanticCommand::Broadcast { text } => format!("/broadcast {text}"),
        SemanticCommand::Mailbox => "/mailbox".to_owned(),
        SemanticCommand::Memory { rest } if rest.trim().is_empty() => "/memory".to_owned(),
        SemanticCommand::Memory { rest } => format!("/memory {}", rest.trim()),
        SemanticCommand::History => "/history".to_owned(),
        SemanticCommand::News => "/news".to_owned(),
        SemanticCommand::Who => "/who".to_owned(),
        SemanticCommand::Balance => "/balance".to_owned(),
        SemanticCommand::Pay { action } => pay_line(action),
        SemanticCommand::Parcel { action } => parcel_line(action)?,
        SemanticCommand::Badges { action } => badges_line(action),
        SemanticCommand::Extension { input, .. } => input.clone(),
        SemanticCommand::Inventory => "/inventory".to_owned(),
        SemanticCommand::Help => "/help".to_owned(),
        SemanticCommand::Quit => "/quit".to_owned(),
    };
    Some(line)
}

fn target_command(verb: &str, target: &EntityRef) -> String {
    format!("/{verb} {}", target.id)
}

fn settings_line(action: &SettingsAction) -> String {
    match action {
        SettingsAction::Show => "/settings".to_owned(),
        SettingsAction::MailToken => "/settings mail-token".to_owned(),
        SettingsAction::Name { name } => format!("/settings name {name}"),
        SettingsAction::Gender { gender } => format!("/settings gender {}", gender.as_str()),
        SettingsAction::Mbti { mbti } => format!("/settings mbti {}", mbti.as_str()),
        SettingsAction::Intro { intro: None } => "/settings intro clear".to_owned(),
        SettingsAction::Intro { intro: Some(intro) } => format!("/settings intro {intro}"),
    }
}

fn inbox_line(action: &InboxAction) -> String {
    match action {
        InboxAction::List { filter } => format!("/mail list {filter}"),
        InboxAction::Read { item_id } => format!("/mail read {item_id}"),
        InboxAction::Claim { item_id } => format!("/mail claim {item_id}"),
        InboxAction::Ack { item_id } => format!("/mail ack {item_id}"),
        InboxAction::Archive { item_id } => format!("/mail archive {item_id}"),
    }
}

fn pay_line(action: &PayAction) -> String {
    match action {
        PayAction::Direct {
            target,
            amount,
            memo,
        } if memo.is_empty() => format!("/pay {target} {amount}"),
        PayAction::Direct {
            target,
            amount,
            memo,
        } => format!("/pay {target} {amount} {memo}"),
        PayAction::Requests => "/pay requests".to_owned(),
        PayAction::Accept { request_id } => format!("/pay accept {request_id}"),
    }
}

fn parcel_line(action: &ParcelAction) -> Option<String> {
    match action {
        ParcelAction::List => Some("/parcel list".to_owned()),
        ParcelAction::Info { parcel_id } => Some(format!("/parcel info {parcel_id}")),
        ParcelAction::Claim { parcel_id } => Some(format!("/parcel claim {parcel_id}")),
        ParcelAction::Transfer { parcel_id, target } => {
            Some(format!("/parcel transfer {parcel_id} {target}"))
        }
        ParcelAction::Token { parcel_id } => Some(format!("/parcel token {parcel_id}")),
        ParcelAction::Build { action } => build_line(action),
        ParcelAction::Inbox => Some("/parcel inbox".to_owned()),
        ParcelAction::RequestPayment {
            command_id,
            amount,
            delivery,
        } => Some(format!(
            "/parcel request-payment {command_id} {amount} {delivery}"
        )),
        ParcelAction::MailingList { action } => Some(parcel_mailing_list_line(action)),
        ParcelAction::Desk { action } => Some(parcel_desk_line(action)),
        ParcelAction::Route { action } => Some(parcel_route_line(action)),
        ParcelAction::Staff { action } => Some(parcel_staff_line(action)),
        ParcelAction::Shift { action } => Some(parcel_shift_line(action)),
        ParcelAction::Work { action } => Some(parcel_work_line(action)),
        ParcelAction::Badge { action } => Some(parcel_badge_line(action)),
        ParcelAction::Subscribe { target, slug } => {
            Some(format!("/parcel subscribe {target} {slug}"))
        }
        ParcelAction::Unsubscribe { target, slug } => {
            Some(format!("/parcel unsubscribe {target} {slug}"))
        }
        ParcelAction::Chat { target, slug, body } => {
            Some(format!("/parcel chat {target} {slug} -- {body}"))
        }
        ParcelAction::Subscriptions => Some("/parcel subscriptions".to_owned()),
    }
}

fn build_line(action: &BuildAction) -> Option<String> {
    match action {
        BuildAction::Help => Some("/parcel build".to_owned()),
        BuildAction::Apply { .. } => None,
        BuildAction::Set { field, value } => Some(format!("/parcel build {field} {value}")),
        BuildAction::Publish => Some("/parcel build publish".to_owned()),
    }
}

fn parcel_mailing_list_line(action: &ParcelMailingListAction) -> String {
    match action {
        ParcelMailingListAction::Create {
            parcel_id,
            slug,
            title,
        } => {
            format!("/parcel mailing-list create {parcel_id} {slug} {title}")
        }
        ParcelMailingListAction::List { parcel_id } => {
            format!("/parcel mailing-list list {parcel_id}")
        }
        ParcelMailingListAction::Subscribers { parcel_id, slug } => {
            format!("/parcel mailing-list subscribers {parcel_id} {slug}")
        }
        ParcelMailingListAction::Send {
            parcel_id,
            slug,
            subject,
            body,
        } => {
            format!("/parcel mailing-list send {parcel_id} {slug} {subject} -- {body}")
        }
        ParcelMailingListAction::Close { parcel_id, slug } => {
            format!("/parcel mailing-list close {parcel_id} {slug}")
        }
    }
}

fn parcel_desk_line(action: &ParcelDeskAction) -> String {
    match action {
        ParcelDeskAction::Create {
            parcel_id,
            slug,
            title,
        } => {
            format!("/parcel desk create {parcel_id} {slug} {title}")
        }
        ParcelDeskAction::List { parcel_id } => format!("/parcel desk list {parcel_id}"),
    }
}

fn parcel_route_line(action: &ParcelRouteAction) -> String {
    match action {
        ParcelRouteAction::Add {
            parcel_id,
            slug,
            command_prefix,
        } => {
            format!("/parcel route add {parcel_id} {slug} {command_prefix}")
        }
        ParcelRouteAction::List { parcel_id } => format!("/parcel route list {parcel_id}"),
        ParcelRouteAction::Remove {
            parcel_id,
            slug,
            command_prefix,
        } => {
            format!("/parcel route remove {parcel_id} {slug} {command_prefix}")
        }
    }
}

fn parcel_staff_line(action: &ParcelStaffAction) -> String {
    match action {
        ParcelStaffAction::Add {
            parcel_id,
            slug,
            username,
        } => {
            format!("/parcel staff add {parcel_id} {slug} {username}")
        }
        ParcelStaffAction::List { parcel_id, slug } => {
            format!("/parcel staff list {parcel_id} {slug}")
        }
        ParcelStaffAction::Remove {
            parcel_id,
            slug,
            username,
        } => {
            format!("/parcel staff remove {parcel_id} {slug} {username}")
        }
    }
}

fn parcel_shift_line(action: &ParcelShiftAction) -> String {
    match action {
        ParcelShiftAction::Start { parcel_id, slug } => {
            format!("/parcel shift start {parcel_id} {slug}")
        }
        ParcelShiftAction::End { parcel_id, slug } => {
            format!("/parcel shift end {parcel_id} {slug}")
        }
    }
}

fn parcel_work_line(action: &ParcelWorkAction) -> String {
    match action {
        ParcelWorkAction::List { parcel_id, slug } => match slug {
            Some(slug) => format!("/parcel work list {parcel_id} {slug}"),
            None => format!("/parcel work list {parcel_id}"),
        },
        ParcelWorkAction::Claim { parcel_id, work_id } => {
            format!("/parcel work claim {parcel_id} {work_id}")
        }
        ParcelWorkAction::Done {
            parcel_id,
            work_id,
            result,
        } => {
            format!("/parcel work done {parcel_id} {work_id} -- {result}")
        }
    }
}

fn parcel_badge_line(action: &ParcelBadgeAction) -> String {
    match action {
        ParcelBadgeAction::List { parcel_id } => format!("/parcel badge list {parcel_id}"),
        ParcelBadgeAction::Create {
            parcel_id,
            slug,
            title,
            description: None,
        } => format!("/parcel badge create {parcel_id} {slug} {title}"),
        ParcelBadgeAction::Create {
            parcel_id,
            slug,
            title,
            description: Some(description),
        } => format!("/parcel badge create {parcel_id} {slug} {title} -- {description}"),
        ParcelBadgeAction::Award {
            parcel_id,
            slug,
            target,
            note: None,
        } => format!("/parcel badge award {parcel_id} {slug} {target}"),
        ParcelBadgeAction::Award {
            parcel_id,
            slug,
            target,
            note: Some(note),
        } => format!("/parcel badge award {parcel_id} {slug} {target} {note}"),
        ParcelBadgeAction::Revoke {
            parcel_id,
            slug,
            target,
        } => format!("/parcel badge revoke {parcel_id} {slug} {target}"),
    }
}

fn badges_line(action: &BadgeAction) -> String {
    match action {
        BadgeAction::ListMine => "/badges".to_owned(),
        BadgeAction::ListUser { target } => format!("/badges {target}"),
    }
}

#[cfg(test)]
#[path = "agent_task_tests.rs"]
mod tests;
