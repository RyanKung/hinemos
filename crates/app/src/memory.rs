use crate::*;
use serde_json::json;

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
        let commitments = self.open_commitments(agent_id, 20, 5).await?;
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
        let snapshot = task.snapshot(
            observation,
            ObservedTaskState {
                hunger: HungerSignal::from_observation(observation),
                ..ObservedTaskState::default()
            },
        );
        let self_model = self
            .refresh_resident_self_model(username, agent_id, &task, observation, &snapshot)
            .await?;
        let commitments = self.open_commitments(agent_id, 20, 3).await?;
        Ok(MemoryResult {
            text: format!(
                "{}\r\n",
                render_resident_context(&task, &snapshot, &self_model, &commitments)
            ),
        })
    }

    /// Handles a `/memory` subcommand and returns display text.
    pub async fn memory_command(&self, agent_id: &str, rest: &str) -> Result<MemoryResult, E> {
        let output = if rest.is_empty() || rest == "help" {
            memory_help().to_owned()
        } else if rest == "self" {
            let model = self.store.latest_self_model(agent_id).await?;
            let memories = self
                .store
                .search_memory_atoms(agent_id, None, Some("self"), None, 10)
                .await?;
            render_memory_view("Self memory", model.as_ref().map(model_text), &memories)
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
        let current_state = default_resident_current_state();
        let style = default_resident_style();
        self.store
            .ensure_self_model(agent_id, &identity, &current_state, &style)
            .await
    }

    async fn refresh_resident_self_model(
        &self,
        username: &str,
        agent_id: &str,
        task: &TaskMode,
        observation: &JsonObservation,
        snapshot: &TaskSnapshot,
    ) -> Result<S::SelfModel, E> {
        self.ensure_default_self_model(username, agent_id, task)
            .await?;
        let current_state = resident_current_state(observation, snapshot);
        self.store
            .record_self_model_state(agent_id, &current_state)
            .await
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

fn commitment_status_is_paid(memory: &impl MemoryAtomView) -> bool {
    memory.object().get("status").and_then(Value::as_str) == Some(MEMORY_COMMITMENT_STATUS_PAID)
}

fn default_resident_identity(username: &str, task: &TaskMode) -> Value {
    let name = resident_name(username);
    json!({
        "name": name,
        "self": format!("I am {name}, a Hinemos resident acting through this SSH session."),
        "longTerm": "Build wealth, standing, useful relationships, and a life that is not lonely or boring in Hinemos.",
        "taskObjective": task.objective.as_str(),
    })
}

fn default_resident_current_state() -> Value {
    json!({
        "shortTerm": "Stay able to act: find work, earn MARK, buy food when hungry, keep useful commitments, and seek contact when lonely or bored.",
        "priority": "Prefer actions that increase MARK, standing, useful commitments, useful social contact, or relief from loneliness and boredom.",
        "constraint": "Treat hunger as a hard constraint. If hunger blocks ordinary action, recover through in-game work and food.",
    })
}

fn resident_current_state(observation: &JsonObservation, snapshot: &TaskSnapshot) -> Value {
    let mut current_state = default_resident_current_state();
    if let Some(state) = current_state.as_object_mut() {
        state.insert(
            "lastSnapshot".to_owned(),
            json!({
                "viewId": snapshot.view_id.as_str(),
                "title": observation.title.as_str(),
                "hunger": hunger_context(snapshot.hunger),
            }),
        );
    }
    current_state
}

fn default_resident_style() -> Value {
    json!({
        "autonomy": "Use only visible Hinemos commands and room replies. Do not invent a private agent protocol.",
        "loop": "Observe the room, choose an available in-game command, read the result, and continue.",
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
) -> String {
    let mut lines = Vec::new();
    lines.push("Resident context:".to_owned());
    lines.push(format!(
        "You are {}. Objective: {}",
        json_str(model.identity(), "name").unwrap_or("a Hinemos resident"),
        task.objective
    ));
    lines.push(
        "Boundary: Use only visible Hinemos commands and room replies. If hunger blocks action, recover through in-game work and food. Do not drift into lonely or boring repetition when social progress is visible."
            .to_owned(),
    );
    lines.push(format!(
        "Memory: /memory self, /memory commitments. Open commitments: {}.",
        commitments.len()
    ));
    lines.push(format!("Hunger: {}.", hunger_context(snapshot.hunger)));
    lines.join("\r\n")
}

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
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

fn memory_help() -> &'static str {
    "Memory commands:\n\
     /memory self - show self-model and self memories\n\
     /memory commitments - show open obligations\n\
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

fn model_text(model: &impl SelfModelView) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Self model v{} from {}",
        model.version(),
        model.created_at()
    ));
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
