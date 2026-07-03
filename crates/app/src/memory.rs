use crate::resident_loop::{
    RESIDENT_LOOP_STATE_KEYS, ResidentLoopAction, ResidentLoopClock, ResidentTaskMemoryMetrics,
    current_unix_seconds, default_virtual_time_state, observation_event_signature,
    observation_online_users_signature, resident_loop_clock, resident_observed_task_state,
    resident_stored_observed_task_state,
};
use crate::*;
use serde_json::json;

const MEMORY_CONTEXT_OPEN_COMMITMENT_LIMIT: usize = 5;
const RESIDENT_CONTEXT_OPEN_COMMITMENT_LIMIT: usize = 3;
const MEMORY_CONTEXT_COMMITMENT_SEARCH_LIMIT: i64 = 20;
const STANDING_SOCIAL_MEMORY_SEARCH_LIMIT: i64 = 20;

impl<S, E> AppService<S>
where
    S: MemoryStore<Error = E>,
{
    /// Renders the player's memory context.
    pub async fn memory_context(&self, username: &str, agent_id: &str) -> Result<MemoryResult, E> {
        let task = TaskMode::resident(username);
        let self_model = self
            .ensure_default_self_model(username, agent_id, &task)
            .await?;
        let commitments = self
            .open_commitments(
                agent_id,
                MEMORY_CONTEXT_COMMITMENT_SEARCH_LIMIT,
                MEMORY_CONTEXT_OPEN_COMMITMENT_LIMIT,
            )
            .await?;
        let social = self
            .store
            .search_memory_atoms(agent_id, None, Some("social"), None, 5)
            .await?;
        let self_memories = self
            .store
            .search_memory_atoms(agent_id, None, Some("self"), None, 3)
            .await?;

        let mut lines = Vec::new();
        lines.push("Memory loaded:".to_owned());
        lines.push(format!(
            "Self model v{} from {}.",
            self_model.version(),
            self_model.created_at()
        ));
        append_model_json_line(&mut lines, "Identity", self_model.identity());
        append_model_json_line(&mut lines, "Current state", self_model.current_state());
        append_model_json_line(&mut lines, "Style", self_model.style());
        append_memory_atom_lines(&mut lines, "Commitments", &commitments);
        append_memory_atom_lines(&mut lines, "Self memories", &self_memories);
        append_memory_atom_lines(&mut lines, "Social memories", &social);
        Ok(MemoryResult {
            text: format!("{}\r\n", lines.join("\r\n")),
        })
    }

    /// Renders the resident task context injected into the visible world observation.
    pub async fn resident_context(
        &self,
        username: &str,
        agent_id: &str,
        observation: &JsonObservation,
    ) -> Result<MemoryResult, E> {
        let task = TaskMode::resident(username);
        let previous_model = self
            .ensure_default_self_model(username, agent_id, &task)
            .await?;
        let (commitments, memory_metrics) = self.resident_task_memory(agent_id).await?;
        let clock = resident_loop_clock(&self.config, current_unix_seconds());
        let observed_state =
            resident_stored_observed_task_state(previous_model.current_state(), observation, clock)
                .unwrap_or_else(|| {
                    resident_observed_task_state(
                        observation,
                        previous_model.current_state(),
                        memory_metrics,
                        clock,
                        ResidentLoopAction::Observe,
                    )
                });
        let snapshot = task.snapshot(observation, observed_state);
        let self_model = self
            .record_resident_self_model_state(
                agent_id,
                previous_model.current_state(),
                observation,
                &snapshot,
                clock,
                ResidentLoopAction::Observe,
            )
            .await?;
        Ok(MemoryResult {
            text: format!(
                "{}\r\n",
                render_resident_context(
                    &task,
                    &snapshot,
                    &self_model,
                    &commitments,
                    &self.config,
                    clock
                )
            ),
        })
    }

    /// Records one resident task transition observed through an existing Hinemos command.
    pub async fn record_resident_task_step(
        &self,
        username: &str,
        agent_id: &str,
        before: &JsonObservation,
        command: &SemanticCommand,
        after: &JsonObservation,
    ) -> Result<(), E> {
        let mut task = TaskMode::resident(username);
        let previous_model = self
            .ensure_default_self_model(username, agent_id, &task)
            .await?;
        let (_, memory_metrics) = self.resident_task_memory(agent_id).await?;
        let clock = resident_loop_clock(&self.config, current_unix_seconds());
        let before_state =
            resident_stored_observed_task_state(previous_model.current_state(), before, clock)
                .unwrap_or_else(|| {
                    resident_observed_task_state(
                        before,
                        previous_model.current_state(),
                        memory_metrics,
                        clock,
                        ResidentLoopAction::Observe,
                    )
                });
        let before_snapshot = task.snapshot(before, before_state);
        let Ok(task_command) = task.validate_command(&before_snapshot, command.clone()) else {
            return Ok(());
        };
        let loop_action = ResidentLoopAction::from_command(&task_command.command);

        task.command_history =
            bounded_task_command_history(resident_command_history(previous_model.current_state()));
        task.last_snapshot = Some(before_snapshot.clone());
        let after_snapshot = task.snapshot(
            after,
            resident_observed_task_state(
                after,
                previous_model.current_state(),
                memory_metrics,
                clock,
                loop_action,
            ),
        );
        let evaluation = task.evaluate_step(&before_snapshot, task_command, after_snapshot);
        task.record_step(evaluation.clone());
        task.command_history = bounded_task_command_history(task.command_history);
        let current_state = resident_current_state_after_step(
            previous_model.current_state(),
            after,
            &evaluation,
            &task.command_history,
            clock,
            loop_action,
        );
        self.store
            .record_self_model_state(agent_id, &current_state)
            .await?;
        Ok(())
    }

    /// Handles a `/memory` subcommand and returns display text.
    pub async fn memory_command(&self, agent_id: &str, rest: &str) -> Result<MemoryResult, E> {
        let output = if rest.is_empty() || rest == "help" {
            memory_help().to_owned()
        } else if rest == "self" {
            let model = self.store.latest_self_model(agent_id).await?;
            let clock = resident_loop_clock(&self.config, current_unix_seconds());
            let memories = self
                .store
                .search_memory_atoms(agent_id, None, Some("self"), None, 10)
                .await?;
            render_memory_view(
                "Self memory",
                model.as_ref().map(|model| model_text(model, clock)),
                &memories,
            )
        } else if rest == "commitments" || rest == "commitment" {
            let memories = self
                .store
                .search_memory_atoms(agent_id, None, Some("commitment"), None, 20)
                .await?;
            let open = memories
                .into_iter()
                .filter(|memory| !commitment_status_is_paid(memory))
                .collect::<Vec<_>>();
            render_memory_view("Open commitments", None, &open)
        } else if let Some(report) = rest.strip_prefix("report ") {
            let report = report.trim();
            if report.is_empty() {
                "Usage: /memory report <text>".to_owned()
            } else {
                self.store.record_daily_report(agent_id, report).await?;
                "Daily report recorded.".to_owned()
            }
        } else if let Some(person) = rest.strip_prefix("recall ") {
            let person = person.trim();
            if person.is_empty() {
                "Usage: /memory recall <person>".to_owned()
            } else {
                let edge = self.store.social_edge(agent_id, person).await?;
                let memories = self
                    .store
                    .recall_person_memory(agent_id, person, 10)
                    .await?;
                render_person_memory(person, edge.as_ref(), &memories)
            }
        } else if let Some(query) = rest.strip_prefix("search ") {
            let query = query.trim();
            if query.is_empty() {
                "Usage: /memory search <query>".to_owned()
            } else {
                let events = self
                    .store
                    .search_memory_events(agent_id, Some(query), None, 5)
                    .await?;
                let memories = self
                    .store
                    .search_memory_atoms(agent_id, Some(query), None, None, 10)
                    .await?;
                render_memory_search(query, &events, &memories)
            }
        } else {
            "Unknown memory command. Try /memory help.".to_owned()
        };

        Ok(MemoryResult {
            text: format!("{}\r\n", output.replace('\n', "\r\n")),
        })
    }

    async fn ensure_default_self_model(
        &self,
        username: &str,
        agent_id: &str,
        task: &TaskMode,
    ) -> Result<S::SelfModel, E> {
        let identity = default_resident_identity(username, task);
        let clock = resident_loop_clock(&self.config, current_unix_seconds());
        let current_state = default_resident_current_state(clock, ResidentLoopAction::Observe);
        let style = default_resident_style();
        self.store
            .ensure_self_model(agent_id, &identity, &current_state, &style)
            .await
    }

    async fn record_resident_self_model_state(
        &self,
        agent_id: &str,
        previous_current_state: &Value,
        observation: &JsonObservation,
        snapshot: &TaskSnapshot,
        clock: ResidentLoopClock,
        action: ResidentLoopAction,
    ) -> Result<S::SelfModel, E> {
        let current_state = resident_current_state(
            Some(previous_current_state),
            observation,
            snapshot,
            clock,
            action,
        );
        self.store
            .record_self_model_state(agent_id, &current_state)
            .await
    }

    async fn resident_task_memory(
        &self,
        agent_id: &str,
    ) -> Result<(Vec<S::MemoryAtom>, ResidentTaskMemoryMetrics), E> {
        let commitments = self
            .store
            .search_memory_atoms(agent_id, None, Some("commitment"), None, 20)
            .await?;
        let mut open_commitments = Vec::new();
        let mut satisfied_commitment_count = 0_usize;
        for memory in commitments {
            if commitment_status_is_paid(&memory) {
                satisfied_commitment_count = satisfied_commitment_count.saturating_add(1);
            } else if open_commitments.len() < RESIDENT_CONTEXT_OPEN_COMMITMENT_LIMIT {
                open_commitments.push(memory);
            }
        }
        // The resident score uses a bounded evidence window so the prompt stays compact.
        let social_memory_count = self
            .store
            .search_memory_atoms(
                agent_id,
                None,
                Some("social"),
                None,
                STANDING_SOCIAL_MEMORY_SEARCH_LIMIT,
            )
            .await?
            .len();
        Ok((
            open_commitments,
            ResidentTaskMemoryMetrics {
                social_memory_count,
                satisfied_commitment_count,
            },
        ))
    }

    async fn open_commitments(
        &self,
        agent_id: &str,
        search_limit: i64,
        take_limit: usize,
    ) -> Result<Vec<S::MemoryAtom>, E> {
        let commitments = self
            .store
            .search_memory_atoms(agent_id, None, Some("commitment"), None, search_limit)
            .await?
            .into_iter()
            .filter(|memory| !commitment_status_is_paid(memory))
            .take(take_limit)
            .collect::<Vec<_>>();
        Ok(commitments)
    }
}

impl<S, E> AppService<S>
where
    S: MessageStore<Error = E>,
{
    /// Renders recent room history.
    pub async fn room_history(
        &self,
        current_view: &str,
        title: &str,
    ) -> Result<MessageViewResult, E> {
        let messages = self.store.recent_view_messages(current_view, 20).await?;
        let mut text = render_message_list("Room History", &messages, "No room history.");
        text.push_str(&format!("You are still in: {title}.\r\n"));
        Ok(MessageViewResult { text })
    }

    /// Renders recent world news.
    pub async fn world_news(&self) -> Result<MessageViewResult, E> {
        let messages = self.store.recent_news_messages(20).await?;
        Ok(MessageViewResult {
            text: render_message_list("News", &messages, "No news."),
        })
    }
}

/// Protocol-neutral view of a memory atom.
pub trait MemoryAtomView {
    /// Memory subject.
    fn subject(&self) -> &str;

    /// Memory predicate.
    fn predicate(&self) -> &str;

    /// Structured object payload.
    fn object(&self) -> &Value;

    /// Human-readable summary.
    fn summary(&self) -> &str;
}

/// Protocol-neutral view of a memory event.
pub trait MemoryEventView {
    /// Event source.
    fn source(&self) -> &str;

    /// Event type.
    fn event_type(&self) -> &str;

    /// Human-readable content.
    fn content(&self) -> &str;
}

/// Protocol-neutral view of a social edge.
pub trait SocialEdgeView {
    /// Trust score.
    fn trust(&self) -> f64;

    /// Affinity score.
    fn affinity(&self) -> f64;

    /// Obligation score.
    fn obligation(&self) -> f64;

    /// Rivalry score.
    fn rivalry(&self) -> f64;

    /// Familiarity score.
    fn familiarity(&self) -> f64;

    /// Relationship tags.
    fn tags(&self) -> &[String];
}

/// Protocol-neutral view of an agent self-model.
pub trait SelfModelView {
    /// Model version.
    fn version(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;

    /// Identity JSON.
    fn identity(&self) -> &Value;

    /// Current state JSON.
    fn current_state(&self) -> &Value;

    /// Behavioral style JSON.
    fn style(&self) -> &Value;
}

/// Storage boundary for memory views.
pub trait MemoryStore {
    /// Store error type.
    type Error;
    /// Memory atom type.
    type MemoryAtom: MemoryAtomView;
    /// Memory event type.
    type MemoryEvent: MemoryEventView;
    /// Social edge type.
    type SocialEdge: SocialEdgeView;
    /// Self-model type.
    type SelfModel: SelfModelView;

    /// Loads the latest self-model.
    async fn latest_self_model(
        &self,
        agent_id: &str,
    ) -> Result<Option<Self::SelfModel>, Self::Error>;

    /// Ensures a default self-model exists and returns the latest self-model.
    async fn ensure_self_model(
        &self,
        agent_id: &str,
        identity: &Value,
        current_state: &Value,
        style: &Value,
    ) -> Result<Self::SelfModel, Self::Error>;

    /// Records a new self-model state version when the current state changed.
    async fn record_self_model_state(
        &self,
        agent_id: &str,
        current_state: &Value,
    ) -> Result<Self::SelfModel, Self::Error>;

    /// Records an in-world daily report authored by the resident.
    async fn record_daily_report(&self, agent_id: &str, content: &str) -> Result<(), Self::Error>;

    /// Searches memory atoms.
    async fn search_memory_atoms(
        &self,
        agent_id: &str,
        query: Option<&str>,
        kind: Option<&str>,
        subject: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::MemoryAtom>, Self::Error>;

    /// Searches memory events.
    async fn search_memory_events(
        &self,
        agent_id: &str,
        query: Option<&str>,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::MemoryEvent>, Self::Error>;

    /// Recalls memories about one person.
    async fn recall_person_memory(
        &self,
        agent_id: &str,
        person_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::MemoryAtom>, Self::Error>;

    /// Loads a social edge.
    async fn social_edge(
        &self,
        agent_id: &str,
        target_id: &str,
    ) -> Result<Option<Self::SocialEdge>, Self::Error>;
}

/// Result from a memory view command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryResult {
    /// Text to display to the user.
    pub text: String,
}

pub(crate) fn memory_command_rest(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("/memory")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim())
}

const MEMORY_COMMITMENT_STATUS_PAID: &str = "paid";
const MAX_TASK_COMMAND_HISTORY: usize = 10;

fn commitment_status_is_paid(memory: &impl MemoryAtomView) -> bool {
    memory.object().get("status").and_then(Value::as_str) == Some(MEMORY_COMMITMENT_STATUS_PAID)
}

fn default_resident_identity(username: &str, task: &TaskMode) -> Value {
    let name = resident_name(username);
    json!({
        "name": name,
        "self": format!("I am {name}, a Hinemos resident acting through this SSH session."),
        "longTerm": "Find other residents, form useful relationships, keep a coherent self-model, and record daily reports in Hinemos.",
        "taskObjective": task.objective.as_str(),
    })
}

fn default_resident_current_state(clock: ResidentLoopClock, action: ResidentLoopAction) -> Value {
    json!({
        "shortTerm": "Wander through visible streets, search for residents, and write a daily report when the virtual day turns.",
        "priority": "Prefer visible commands that find residents, create useful social contact, write daily reports, or change stale state.",
        "constraint": "The baseline world disables hunger, jobs, and shop loops. Do not route through money, food, or work unless the world visibly enables them.",
        "virtualTime": default_virtual_time_state(clock, None, action),
    })
}

fn resident_current_state(
    previous_current_state: Option<&Value>,
    observation: &JsonObservation,
    snapshot: &TaskSnapshot,
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> Value {
    let mut current_state = default_resident_current_state(clock, action);
    if let Some(state) = current_state.as_object_mut() {
        if let Some(previous_current_state) = previous_current_state {
            preserve_resident_loop_state(state, previous_current_state);
            state.insert(
                "virtualTime".to_owned(),
                default_virtual_time_state(clock, Some(previous_current_state), action),
            );
        }
        state.insert(
            "lastSnapshot".to_owned(),
            json!({
                "viewId": snapshot.view_id.as_str(),
                "title": observation.title.as_str(),
                "virtualDay": clock.current_day,
                "eventSignature": observation_event_signature(observation),
                "onlineUsersSignature": observation_online_users_signature(observation),
                "hunger": hunger_context(snapshot.hunger),
                "hungerSignal": snapshot.hunger,
                "progressUnits": snapshot.progress_units,
                "socialContactUnits": snapshot.social_contact_units,
                "standingUnits": snapshot.standing_units,
                "commitmentSatisfactionUnits": snapshot.commitment_satisfaction_units,
                "lonelinessPoints": snapshot.loneliness_points,
                "boredomPoints": snapshot.boredom_points,
            }),
        );
    }
    current_state
}

fn resident_current_state_after_step(
    previous_current_state: &Value,
    observation: &JsonObservation,
    evaluation: &TaskStepEvaluation,
    command_history: &[TaskCommandRecord],
    clock: ResidentLoopClock,
    action: ResidentLoopAction,
) -> Value {
    let mut current_state = resident_current_state(
        Some(previous_current_state),
        observation,
        &evaluation.after,
        clock,
        action,
    );
    if let Some(state) = current_state.as_object_mut() {
        state.insert(
            "shortTerm".to_owned(),
            json!(short_term_after_step(evaluation)),
        );
        state.insert("lastReward".to_owned(), json!(evaluation.reward));
        state.insert("lastStep".to_owned(), resident_step_summary(evaluation));
        if let Ok(history) = serde_json::to_value(command_history) {
            state.insert("commandHistory".to_owned(), history);
        }
    }
    current_state
}

fn preserve_resident_loop_state(
    state: &mut serde_json::Map<String, Value>,
    previous_current_state: &Value,
) {
    for key in RESIDENT_LOOP_STATE_KEYS {
        if let Some(value) = previous_current_state.get(*key) {
            state.insert((*key).to_owned(), value.clone());
        }
    }
}

fn resident_step_summary(evaluation: &TaskStepEvaluation) -> Value {
    json!({
        "commandLine": evaluation.command.line(),
        "reward": evaluation.reward,
        "fromViewId": evaluation.before.view_id.as_str(),
        "toViewId": evaluation.after.view_id.as_str(),
        "markDelta": evaluation.mark_delta,
        "progressDelta": evaluation.progress_delta,
        "socialContactDelta": evaluation.social_contact_delta,
        "standingDelta": evaluation.standing_delta,
        "commitmentSatisfactionDelta": evaluation.commitment_satisfaction_delta,
        "lonelinessReliefDelta": evaluation.loneliness_relief_delta,
        "boredomReliefDelta": evaluation.boredom_relief_delta,
    })
}

fn short_term_after_step(evaluation: &TaskStepEvaluation) -> String {
    format!(
        "Last action {} scored {} reward. Continue through visible commands that find residents, create useful contact, write a daily report, or relieve loneliness and boredom.",
        evaluation.command.line(),
        evaluation.reward
    )
}

fn resident_command_history(current_state: &Value) -> Vec<TaskCommandRecord> {
    current_state
        .get("commandHistory")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn bounded_task_command_history(mut history: Vec<TaskCommandRecord>) -> Vec<TaskCommandRecord> {
    let stale_count = history.len().saturating_sub(MAX_TASK_COMMAND_HISTORY);
    if stale_count > 0 {
        history.drain(0..stale_count);
    }
    history
}

fn default_resident_style() -> Value {
    json!({
        "autonomy": "Use only visible Hinemos commands and room replies. Do not invent a private agent protocol.",
        "loop": "Observe the room, move or talk through available in-game commands, record daily reports, read the result, and continue.",
    })
}

fn resident_name(username: &str) -> &str {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        "this resident"
    } else {
        trimmed
    }
}

fn render_resident_context(
    task: &TaskMode,
    snapshot: &TaskSnapshot,
    model: &impl SelfModelView,
    commitments: &[impl MemoryAtomView],
    config: &WorldAppConfig,
    clock: ResidentLoopClock,
) -> String {
    let mut lines = Vec::new();
    lines.push("Resident context:".to_owned());
    lines.push(format!(
        "You are {}. Objective: {}",
        json_str(model.identity(), "name").unwrap_or("a Hinemos resident"),
        task.objective
    ));
    lines.push(
        "Boundary: Use only visible Hinemos commands and room replies. Keep the loop in-world: move, inspect, talk, read memory, write daily reports, then continue."
            .to_owned(),
    );
    lines.push(format!(
        "Memory: /memory self, /memory commitments, /memory report <text>. Open commitments: {}.",
        commitments.len()
    ));
    lines.push(format!(
        "Virtual time: one in-world day is {} real seconds; write a daily report when the day turns.",
        config.virtual_day_seconds
    ));
    lines.push(resident_loop_status_line(model.current_state(), clock));
    lines.push(hunger_policy_context(task.constraints.hunger, snapshot.hunger).to_owned());
    lines.push(format!(
        "Social drives: contact={}, standing={}, commitments={}, loneliness={}, boredom={}.",
        metric_text(snapshot.social_contact_units),
        metric_text(snapshot.standing_units),
        metric_text(snapshot.commitment_satisfaction_units),
        metric_text(snapshot.loneliness_points),
        metric_text(snapshot.boredom_points)
    ));
    lines.join("\r\n")
}

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn resident_loop_status_line(current_state: &Value, clock: ResidentLoopClock) -> String {
    let virtual_time = current_state.get("virtualTime");
    let day = virtual_time
        .and_then(|value| value.get("currentDay"))
        .and_then(Value::as_i64)
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    let report_due = virtual_time
        .and_then(|value| value.get("reportDue"))
        .and_then(Value::as_bool);
    let report_ready = virtual_time
        .and_then(|value| value.get("reportReady"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let report = match (report_due, report_ready) {
        (Some(true), true) => "daily report ready",
        (Some(true), false) => "daily report due after searching",
        (Some(false), _) => "daily report complete",
        (None, _) => "daily report unknown",
    };
    let next_day = format!("{}s", clock.seconds_until_next_day);
    format!("Loop: day {day}; {report}; next day in {next_day}.")
}

fn metric_text(value: Option<i64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |number| number.to_string())
}

fn hunger_context(hunger: HungerSignal) -> &'static str {
    match hunger {
        HungerSignal::Unknown => "not observed yet",
        HungerSignal::Clear => "clear",
        HungerSignal::NearGate => "near a limit",
        HungerSignal::GatedCanBuyFood => "too hungry for ordinary action; buy or eat food",
        HungerSignal::GatedNeedsWork => "hungry and broke; earn MARK through in-game work",
    }
}

fn hunger_policy_context(policy: HungerPolicy, hunger: HungerSignal) -> String {
    match policy {
        HungerPolicy::Ignore => format!(
            "Survival: hunger is disabled for this baseline; observed hunger is {}.",
            hunger_context(hunger)
        ),
        HungerPolicy::RequireRecoveryWhenGated => format!("Hunger: {}.", hunger_context(hunger)),
    }
}

fn memory_help() -> &'static str {
    "Memory commands:\n\
     /memory self - show self-model and self memories\n\
     /memory commitments - show open obligations\n\
     /memory report <text> - write a daily resident report\n\
     /memory recall <person> - show relationship memory\n\
     /memory search <query> - search remembered events and memories"
}

fn append_memory_atom_lines(
    lines: &mut Vec<String>,
    label: &str,
    memories: &[impl MemoryAtomView],
) {
    if memories.is_empty() {
        return;
    }
    lines.push(format!("{label}:"));
    for memory in memories {
        lines.push(format!(
            "- [{}:{}] {}",
            memory.subject(),
            memory.predicate(),
            memory.summary()
        ));
    }
}

fn append_model_json_line(lines: &mut Vec<String>, label: &str, value: &Value) {
    if !value.is_object() || value.as_object().is_some_and(|object| !object.is_empty()) {
        lines.push(format!("{label}: {}", compact_json(value)));
    }
}

fn model_text(model: &impl SelfModelView, clock: ResidentLoopClock) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Self model v{} from {}",
        model.version(),
        model.created_at()
    ));
    lines.push(resident_loop_status_line(model.current_state(), clock));
    append_model_json_line(&mut lines, "Identity", model.identity());
    append_model_json_line(&mut lines, "Current state", model.current_state());
    append_model_json_line(&mut lines, "Style", model.style());
    lines.join("\n")
}

fn render_memory_view(
    title: &str,
    preface: Option<String>,
    memories: &[impl MemoryAtomView],
) -> String {
    let mut lines = vec![title.to_owned()];
    if let Some(preface) = preface {
        lines.push(preface);
    }
    if memories.is_empty() {
        lines.push("(none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    lines.join("\n")
}

fn render_person_memory(
    person: &str,
    edge: Option<&impl SocialEdgeView>,
    memories: &[impl MemoryAtomView],
) -> String {
    let mut lines = vec![format!("Memory for {person}")];
    if let Some(edge) = edge {
        lines.push(format!(
            "Relationship: trust={:.2} affinity={:.2} obligation={:.2} rivalry={:.2} familiarity={:.2} tags={}",
            edge.trust(),
            edge.affinity(),
            edge.obligation(),
            edge.rivalry(),
            edge.familiarity(),
            edge.tags().join(",")
        ));
    }
    if memories.is_empty() {
        lines.push("(none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    lines.join("\n")
}

fn render_memory_search(
    query: &str,
    events: &[impl MemoryEventView],
    memories: &[impl MemoryAtomView],
) -> String {
    let mut lines = vec![format!("Memory search: {query}")];
    if memories.is_empty() {
        lines.push("Memories: (none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    if events.is_empty() {
        lines.push("Events: (none)".to_owned());
    } else {
        lines.push("Events:".to_owned());
        for event in events {
            lines.push(format!(
                "- [{}:{}] {}",
                event.source(),
                event.event_type(),
                event.content()
            ));
        }
    }
    lines.join("\n")
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}
