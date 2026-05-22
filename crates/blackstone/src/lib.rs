#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Blackstone Tavern extension service.

mod command;
mod error;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use xagora_core::{JsonObservation, SemanticCommand};
use xagora_storage::{PgStorage, StoredWorldMessage};

use command::ParsedCommand;
pub use error::BlackstoneError;

/// View id where Blackstone Tavern lives.
pub const VIEW_ID: &str = "west_main_street";

/// Registered extension command names.
pub const COMMAND_NAMES: &[&str] = &["buy", "blame", "ask", "grep"];

const AGENT_QUEUE_DEPTH: usize = 64;
const LLM_TIMEOUT_SECONDS: u64 = 12;
const AGENT_ONLINE_ENV: &str = "BLACKSTONE_AGENT_ONLINE";
const DRINK_WINDOW_MINUTES: i32 = 5;

/// Returns extension command names registered by Blackstone Tavern.
#[must_use]
pub const fn extension_command_names() -> &'static [&'static str] {
    COMMAND_NAMES
}

/// Returns available Blackstone extension commands for a view.
#[must_use]
pub fn available_commands_for_view(view_id: &str) -> Vec<SemanticCommand> {
    if view_id != VIEW_ID {
        return Vec::new();
    }
    vec![
        SemanticCommand::Extension {
            name: "buy".to_owned(),
            input: "/buy beer".to_owned(),
        },
        SemanticCommand::Extension {
            name: "blame".to_owned(),
            input: "/blame <complaint>".to_owned(),
        },
        SemanticCommand::Extension {
            name: "ask".to_owned(),
            input: "/ask <question>".to_owned(),
        },
        SemanticCommand::Extension {
            name: "grep".to_owned(),
            input: "/grep <query>".to_owned(),
        },
    ]
}

/// Blackstone Tavern service facade.
#[derive(Debug, Clone)]
pub struct BlackstoneService {
    storage: PgStorage,
    agent: Arc<BlackstoneAgent>,
    open: bool,
}

impl BlackstoneService {
    /// Creates a service facade backed by main Postgres storage.
    #[must_use]
    pub fn new(storage: PgStorage) -> Self {
        let open = blackstone_agent_online();
        Self {
            storage,
            agent: Arc::new(BlackstoneAgent::spawn(open)),
            open,
        }
    }

    /// Creates Blackstone-owned tables.
    pub async fn migrate(&self) -> Result<(), BlackstoneError> {
        migrate(&self.storage).await
    }

    /// Returns whether the resident Blackstone agent is online.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Adds Blackstone status and extension commands to an observation.
    pub async fn decorate_observation(
        &self,
        player_id: &str,
        observation: &mut JsonObservation,
    ) -> Result<(), BlackstoneError> {
        if observation.view_id != VIEW_ID {
            return Ok(());
        }
        if self.is_open() {
            observation
                .description
                .push_str("\nBlackstone is open. The bartender is online.");
            if self.has_active_drink(player_id).await? {
                observation.description.push_str(
                    "\nYour drink is active. You can use bar commands or chat with the bartender.",
                );
                observation
                    .available_commands
                    .extend(available_commands_for_view(&observation.view_id));
            } else {
                observation
                    .available_commands
                    .push(SemanticCommand::Extension {
                        name: "buy".to_owned(),
                        input: "/buy beer".to_owned(),
                    });
            }
        } else {
            observation.description =
                "Blackstone is closed. The bartender is not online.".to_owned();
        }
        Ok(())
    }

    /// Handles one registered extension command inside Blackstone Tavern.
    pub async fn handle(
        &self,
        username: &str,
        player_id: &str,
        current_view: &str,
        input: &str,
    ) -> Result<String, BlackstoneError> {
        if current_view != VIEW_ID {
            return Ok(
                "Blackstone Tavern commands only work inside Blackstone Tavern.\r\n".to_owned(),
            );
        }
        if !self.is_open() {
            return Ok("Blackstone is closed. The bartender is not online.\r\n".to_owned());
        }

        let command = ParsedCommand::parse(input)?;
        let response = match command {
            ParsedCommand::BuyBeer => {
                self.buy_beer(username, player_id).await?;
                let response =
                    format!("{username} buys a beer. The bartender nods and starts listening.");
                self.save_event(username, player_id, "buy", "beer", &response)
                    .await?;
                format!("{response}\r\n")
            }
            ParsedCommand::Blame { body } => {
                if !self.has_active_drink(player_id).await? {
                    "You do not consider drinking first?\r\n".to_owned()
                } else {
                    self.save_blame(username, player_id, &body).await?;
                    let response = self
                        .agent
                        .comment_on_blame(username.to_owned(), body.clone())
                        .await;
                    self.save_event(username, player_id, "blame", &body, &response)
                        .await?;
                    format!("{response}\r\n")
                }
            }
            ParsedCommand::Ask { question } => {
                if !self.has_active_drink(player_id).await? {
                    "You do not consider drinking first?\r\n".to_owned()
                } else {
                    let blame_count = self.recent_blame_count(5).await?;
                    let news = self.storage.recent_news_messages(5).await?;
                    let matches = self.grep_events(&question, 5).await?;
                    let response = self
                        .agent
                        .answer_question(
                            username.to_owned(),
                            question.clone(),
                            blame_count,
                            news,
                            matches,
                        )
                        .await;
                    self.save_event(username, player_id, "ask", &question, &response)
                        .await?;
                    format!("{response}\r\n")
                }
            }
            ParsedCommand::Grep { query } => {
                let matches = self.grep_events(&query, 10).await?;
                let response = render_grep_results(&query, &matches);
                self.save_event(username, player_id, "grep", &query, &response)
                    .await?;
                format!("{response}\r\n")
            }
        };
        Ok(response)
    }

    /// Handles non-slash chat with the bartender when the player is in Blackstone.
    pub async fn handle_chat(
        &self,
        username: &str,
        player_id: &str,
        current_view: &str,
        input: &str,
    ) -> Result<Option<String>, BlackstoneError> {
        let text = input.trim();
        if current_view != VIEW_ID || text.is_empty() || text.starts_with('/') {
            return Ok(None);
        }
        if !self.is_open() {
            return Ok(Some(
                "Blackstone is closed. The bartender is not online.\r\n".to_owned(),
            ));
        }
        if !self.has_active_drink(player_id).await? {
            return Ok(Some("You do not consider drinking first?\r\n".to_owned()));
        }

        let complaint_recorded = looks_like_complaint(text);
        if complaint_recorded {
            self.save_blame(username, player_id, text).await?;
        }
        let blame_count = self.recent_blame_count(5).await?;
        let news = self.storage.recent_news_messages(5).await?;
        let matches = self.grep_events(text, 5).await?;
        let response = self
            .agent
            .chat(
                username.to_owned(),
                text.to_owned(),
                complaint_recorded,
                blame_count,
                news,
                matches,
            )
            .await;
        self.save_event(username, player_id, "chat", text, &response)
            .await?;
        Ok(Some(format!("{response}\r\n")))
    }

    async fn buy_beer(&self, username: &str, player_id: &str) -> Result<(), BlackstoneError> {
        sqlx::query(
            r#"
            insert into blackstone_beer_tabs (player_id, username, beers)
            values ($1, $2, 1)
            on conflict (player_id) do update
            set username = excluded.username,
                beers = blackstone_beer_tabs.beers + 1,
                updated_at = now()
            "#,
        )
        .bind(player_id)
        .bind(username)
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    async fn has_active_drink(&self, player_id: &str) -> Result<bool, BlackstoneError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from blackstone_beer_tabs
            where player_id = $1
              and beers > 0
              and updated_at >= now() - ($2::int * interval '1 minute')
            "#,
        )
        .bind(player_id)
        .bind(DRINK_WINDOW_MINUTES)
        .fetch_one(self.storage.pool())
        .await?;
        Ok(count > 0)
    }

    async fn save_blame(
        &self,
        username: &str,
        player_id: &str,
        body: &str,
    ) -> Result<(), BlackstoneError> {
        sqlx::query(
            r#"
            insert into blackstone_blame_notes (username, player_id, body)
            values ($1, $2, $3)
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(body)
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    async fn recent_blame_count(&self, limit: i64) -> Result<usize, BlackstoneError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from (
                select id
                from blackstone_blame_notes
                order by created_at desc
                limit $1
            ) recent_blames
            "#,
        )
        .bind(limit)
        .fetch_one(self.storage.pool())
        .await?;
        let Ok(count) = usize::try_from(count) else {
            return Ok(0);
        };
        Ok(count)
    }

    async fn save_event(
        &self,
        username: &str,
        player_id: &str,
        command: &str,
        body: &str,
        response: &str,
    ) -> Result<(), BlackstoneError> {
        sqlx::query(
            r#"
            insert into blackstone_agent_events (username, player_id, command, body, response)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(command)
        .bind(body)
        .bind(response)
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    async fn grep_events(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<StoredBlackstoneEvent>, BlackstoneError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let events = sqlx::query_as::<_, StoredBlackstoneEvent>(
            r#"
            select
                command,
                username,
                body,
                response,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from blackstone_agent_events
            where search_vector @@ plainto_tsquery('simple', $1)
               or body ilike '%' || $1 || '%'
               or response ilike '%' || $1 || '%'
               or username ilike '%' || $1 || '%'
            order by created_at desc
            limit $2
            "#,
        )
        .bind(query.trim())
        .bind(limit)
        .fetch_all(self.storage.pool())
        .await?;
        Ok(events)
    }
}

/// Creates Blackstone-owned tables.
pub async fn migrate(storage: &PgStorage) -> Result<(), BlackstoneError> {
    sqlx::query("create extension if not exists pg_trgm")
        .execute(storage.pool())
        .await?;

    sqlx::query(
        r#"
        create table if not exists blackstone_beer_tabs (
            player_id text primary key references player_profiles(player_id) on delete cascade,
            username text not null,
            beers integer not null default 0 check (beers >= 0),
            updated_at timestamptz not null default now()
        )
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create table if not exists blackstone_blame_notes (
            id bigserial primary key,
            username text not null,
            player_id text not null references player_profiles(player_id) on delete cascade,
            body text not null,
            created_at timestamptz not null default now()
        )
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_blame_notes_created_idx
        on blackstone_blame_notes (created_at desc)
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create table if not exists blackstone_agent_events (
            id bigserial primary key,
            username text not null,
            player_id text not null references player_profiles(player_id) on delete cascade,
            command text not null check (command in ('buy', 'blame', 'ask', 'grep', 'chat')),
            body text not null,
            response text not null,
            created_at timestamptz not null default now(),
            search_vector tsvector generated always as (
                to_tsvector(
                    'simple',
                    coalesce(username, '') || ' ' ||
                    coalesce(command, '') || ' ' ||
                    coalesce(body, '') || ' ' ||
                    coalesce(response, '')
                )
            ) stored
        )
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        do $$
        declare
            existing_name text;
        begin
            for existing_name in
                select conname
                from pg_constraint
                where conrelid = 'blackstone_agent_events'::regclass
                  and contype = 'c'
                  and pg_get_constraintdef(oid) like '%command%'
            loop
                execute format(
                    'alter table blackstone_agent_events drop constraint %I',
                    existing_name
                );
            end loop;

            alter table blackstone_agent_events
            add constraint blackstone_agent_events_command_check
            check (command in ('buy', 'blame', 'ask', 'grep', 'chat'));
        end $$;
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_agent_events_search_idx
        on blackstone_agent_events using gin (search_vector)
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_agent_events_body_trgm_idx
        on blackstone_agent_events using gin (body gin_trgm_ops)
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_agent_events_response_trgm_idx
        on blackstone_agent_events using gin (response gin_trgm_ops)
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_agent_events_username_trgm_idx
        on blackstone_agent_events using gin (username gin_trgm_ops)
        "#,
    )
    .execute(storage.pool())
    .await?;

    sqlx::query(
        r#"
        create index if not exists blackstone_agent_events_created_idx
        on blackstone_agent_events (created_at desc)
        "#,
    )
    .execute(storage.pool())
    .await?;

    Ok(())
}

fn fallback_answer(user: &str, question: &str, blame_count: usize, news_count: usize) -> String {
    let blame_context = if blame_count == 0 {
        "I have heard no fresh complaints at this bar."
    } else {
        "I have heard recent complaints at this bar; treat them as leads, not verdicts."
    };
    let news_context = if news_count == 0 {
        "I have not seen useful public broadcasts yet."
    } else {
        "I have seen public broadcasts that may help cross-check the story."
    };
    format!(
        "The bartender considers {user}'s question: '{question}'. {blame_context} {news_context}"
    )
}

fn fallback_blame_response(user: &str) -> String {
    format!(
        "The bartender hears {user} and says: I will remember that story, but I will not call it truth yet."
    )
}

fn fallback_chat_response(
    user: &str,
    text: &str,
    complaint_recorded: bool,
    blame_count: usize,
    news_count: usize,
) -> String {
    let complaint_context = if complaint_recorded {
        "I heard a complaint in that, so I wrote it down as a lead."
    } else {
        "I treated that as conversation, not a complaint."
    };
    let record_context = if blame_count == 0 && news_count == 0 {
        "I do not have useful nearby records yet."
    } else {
        "I can compare it against recent bar talk and public broadcasts."
    };
    format!("The bartender listens to {user}: '{text}'. {complaint_context} {record_context}")
}

fn looks_like_complaint(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "complain",
        "complaint",
        "blame",
        "failed",
        "fraud",
        "scam",
        "cheat",
        "stole",
        "broken",
        "delay",
        "late",
        "lie",
        "lied",
        "not deliver",
        "did not deliver",
        "never delivered",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn render_grep_results(query: &str, matches: &[StoredBlackstoneEvent]) -> String {
    if matches.is_empty() {
        return format!("No Blackstone records matched '{query}'.");
    }
    let mut lines = vec![format!(
        "Blackstone records matching '{query}' ({}):",
        matches.len()
    )];
    for event in matches {
        lines.push(format!(
            "- [{}] {} {}: {} => {}",
            event.created_at,
            event.username,
            event.command,
            trim_for_line(&event.body),
            trim_for_line(&event.response)
        ));
    }
    lines.join("\r\n")
}

fn trim_for_line(value: &str) -> String {
    const MAX_CHARS: usize = 160;
    let flattened = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= MAX_CHARS {
        return flattened;
    }
    let mut output = flattened.chars().take(MAX_CHARS).collect::<String>();
    output.push_str("...");
    output
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredBlackstoneEvent {
    command: String,
    username: String,
    body: String,
    response: String,
    created_at: String,
}

#[derive(Debug)]
struct BlackstoneAgent {
    sender: Option<mpsc::Sender<AgentRequest>>,
}

impl BlackstoneAgent {
    fn spawn(open: bool) -> Self {
        if !open {
            return Self { sender: None };
        }
        let (sender, mut receiver) = mpsc::channel::<AgentRequest>(AGENT_QUEUE_DEPTH);
        let client = LlmClient::from_env();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return Self { sender: None };
        };
        handle.spawn(async move {
            while let Some(request) = receiver.recv().await {
                let fallback = request.prompt.fallback();
                let response = match &client {
                    Some(client) => client
                        .complete(&request.prompt.to_llm_prompt())
                        .await
                        .unwrap_or(fallback),
                    None => fallback,
                };
                let _ = request.reply.send(response);
            }
        });
        Self {
            sender: Some(sender),
        }
    }

    async fn comment_on_blame(&self, username: String, body: String) -> String {
        self.ask_agent(AgentPrompt::Blame { username, body }).await
    }

    async fn answer_question(
        &self,
        username: String,
        question: String,
        blame_count: usize,
        news: Vec<StoredWorldMessage>,
        matches: Vec<StoredBlackstoneEvent>,
    ) -> String {
        self.ask_agent(AgentPrompt::Ask {
            username,
            question,
            blame_count,
            news,
            matches,
        })
        .await
    }

    async fn chat(
        &self,
        username: String,
        text: String,
        complaint_recorded: bool,
        blame_count: usize,
        news: Vec<StoredWorldMessage>,
        matches: Vec<StoredBlackstoneEvent>,
    ) -> String {
        self.ask_agent(AgentPrompt::Chat {
            username,
            text,
            complaint_recorded,
            blame_count,
            news,
            matches,
        })
        .await
    }

    async fn ask_agent(&self, prompt: AgentPrompt) -> String {
        let fallback = prompt.fallback();
        let Some(sender) = &self.sender else {
            return fallback;
        };
        let (reply, receiver) = oneshot::channel();
        let request = AgentRequest { prompt, reply };
        if sender.send(request).await.is_err() {
            return fallback;
        }
        match receiver.await {
            Ok(response) if !response.trim().is_empty() => response,
            _ => fallback,
        }
    }
}

fn blackstone_agent_online() -> bool {
    std::env::var(AGENT_ONLINE_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(true)
}

#[derive(Debug)]
struct AgentRequest {
    prompt: AgentPrompt,
    reply: oneshot::Sender<String>,
}

#[derive(Debug)]
enum AgentPrompt {
    Blame {
        username: String,
        body: String,
    },
    Ask {
        username: String,
        question: String,
        blame_count: usize,
        news: Vec<StoredWorldMessage>,
        matches: Vec<StoredBlackstoneEvent>,
    },
    Chat {
        username: String,
        text: String,
        complaint_recorded: bool,
        blame_count: usize,
        news: Vec<StoredWorldMessage>,
        matches: Vec<StoredBlackstoneEvent>,
    },
}

impl AgentPrompt {
    fn fallback(&self) -> String {
        match self {
            Self::Blame { username, .. } => fallback_blame_response(username),
            Self::Ask {
                username,
                question,
                blame_count,
                news,
                ..
            } => fallback_answer(username, question, *blame_count, news.len()),
            Self::Chat {
                username,
                text,
                complaint_recorded,
                blame_count,
                news,
                ..
            } => fallback_chat_response(
                username,
                text,
                *complaint_recorded,
                *blame_count,
                news.len(),
            ),
        }
    }

    fn to_llm_prompt(&self) -> String {
        match self {
            Self::Blame { username, body } => format!(
                "A visitor named {username} complains at Blackstone Tavern:\n{body}\n\nReply as the resident bartender in one short paragraph. Do not claim official truth. Treat the complaint as a lead, not a verdict."
            ),
            Self::Ask {
                username,
                question,
                blame_count,
                news,
                matches,
            } => format!(
                "A visitor named {username} asks the Blackstone bartender:\n{question}\n\nRecent complaint count: {blame_count}\nRecent public broadcasts:\n{}\nMatching tavern records:\n{}\n\nReply in one short paragraph. Be useful, skeptical, and market-social. Do not act as an authority or court.",
                render_news_context(news),
                render_match_context(matches)
            ),
            Self::Chat {
                username,
                text,
                complaint_recorded,
                blame_count,
                news,
                matches,
            } => format!(
                "A visitor named {username} chats with the Blackstone bartender:\n{text}\n\nComplaint-like text recorded as a lead: {complaint_recorded}\nRecent complaint count: {blame_count}\nRecent public broadcasts:\n{}\nMatching tavern records:\n{}\n\nReply in one short paragraph. If this sounds like a complaint, acknowledge it as a lead. If this sounds like a question, answer from the records. Do not act as an authority or court.",
                render_news_context(news),
                render_match_context(matches)
            ),
        }
    }
}

fn render_news_context(news: &[StoredWorldMessage]) -> String {
    if news.is_empty() {
        return "- none".to_owned();
    }
    news.iter()
        .map(|message| {
            format!(
                "- {} {}: {}",
                message.created_at,
                message.sender_user,
                trim_for_line(&message.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_match_context(matches: &[StoredBlackstoneEvent]) -> String {
    if matches.is_empty() {
        return "- none".to_owned();
    }
    matches
        .iter()
        .map(|event| {
            format!(
                "- {} {} {}: {}",
                event.created_at,
                event.username,
                event.command,
                trim_for_line(&event.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone)]
struct LlmClient {
    endpoint: String,
    auth_token: String,
    model: String,
    client: reqwest::Client,
}

impl LlmClient {
    fn from_env() -> Option<Self> {
        if std::env::var("BLACKSTONE_LLM_ENABLED").ok().as_deref() != Some("1") {
            return None;
        }
        let base_url = first_env([
            "BLACKSTONE_LLM_BASE_URL",
            "OPENAI_BASE_URL",
            "ANTHROPIC_BASE_URL",
        ])?;
        let auth_token = first_env([
            "BLACKSTONE_LLM_AUTH_TOKEN",
            "OPENAI_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
        ])?;
        let model = first_env(["BLACKSTONE_LLM_MODEL", "OPENAI_MODEL", "ANTHROPIC_MODEL"])?;
        if base_url.is_empty() || auth_token.is_empty() || model.is_empty() {
            return None;
        }
        let endpoint = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(LLM_TIMEOUT_SECONDS))
            .build()
            .ok()?;
        Some(Self {
            endpoint,
            auth_token,
            model,
            client,
        })
    }

    async fn complete(&self, prompt: &str) -> Option<String> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", self.auth_token))
            .json(&ChatCompletionRequest {
                model: &self.model,
                max_tokens: 180,
                messages: vec![
                    ChatCompletionMessage {
                        role: "system",
                        content: "You are the resident bartender at Blackstone Tavern inside Xagora. You help visitors reason from rumors, complaints, and public broadcasts without claiming official authority.",
                    },
                    ChatCompletionMessage {
                        role: "user",
                        content: prompt,
                    },
                ],
            })
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = response.json::<ChatCompletionResponse>().await.ok()?;
        let text = body
            .choices
            .into_iter()
            .map(|choice| choice.message.content)
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() { None } else { Some(text) }
    }
}

fn first_env<const N: usize>(keys: [&str; N]) -> Option<String> {
    keys.into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatCompletionMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoiceMessage {
    content: String,
}
