use gloo_net::http::Request;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlInputElement, InputEvent, SubmitEvent};
use yew::prelude::*;

const PROMPT: &str = "anonymous@hinemos:~$";
const DEFAULT_TERMINAL_COLS: u16 = 68;
const DEFAULT_TERMINAL_ROWS: u16 = 18;
const READONLY_SSH_GUIDANCE: &str = "This web demo is read-only: anonymous visitors can look around, but admission, chat, jobs, payments, shops, and account setup require SSH identity.\nNext: ssh -T hinemos.ai\nThen run: /read agreement, /agree, /enter workers, /position list";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    cols: u16,
    rows: u16,
}

#[derive(Clone, PartialEq, Eq, Properties)]
struct WorldSketchProps {
    size: TerminalSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    view_id: String,
    title: String,
    ascii_art: Vec<String>,
    description: String,
    exits: Vec<Exit>,
    entities: Vec<Entity>,
    events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entity {
    id: String,
    name: String,
    actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Exit {
    direction: Direction,
    label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Event {
    Message { text: String },
    Move { direction: Direction },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandRequest {
    Look,
    Map,
    Inventory,
    Help,
    Move(Direction),
    Inspect(String),
    Read(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalLine {
    Prompt(String),
    Output(String),
    Error(String),
}

#[function_component(WorldSketch)]
fn world_sketch(props: &WorldSketchProps) -> Html {
    let observation = use_state(|| None::<Observation>);
    let history = use_state(Vec::<TerminalLine>::new);
    let input = use_state(String::new);
    let loading = use_state(|| true);
    let should_refocus = use_state(|| false);
    let screen_ref = use_node_ref();
    let input_ref = use_node_ref();

    {
        let observation = observation.clone();
        let history = history.clone();
        let loading = loading.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match fetch_observation(None, None).await {
                    Ok(next) => {
                        history.set(initial_terminal_history(&next));
                        observation.set(Some(next));
                    }
                    Err(error) => history.set(vec![TerminalLine::Error(error)]),
                }
                loading.set(false);
            });
            || ()
        });
    }

    {
        let screen_ref = screen_ref.clone();
        let history_len = history.len();
        use_effect_with(history_len, move |_| {
            if let Some(screen) = screen_ref.cast::<Element>() {
                screen.set_scroll_top(screen.scroll_height());
            }
            || ()
        });
    }

    {
        let input_ref = input_ref.clone();
        let should_refocus = should_refocus.clone();
        let needs_focus = *should_refocus;
        let loading_now = *loading;
        use_effect_with((needs_focus, loading_now), move |_| {
            if needs_focus && !loading_now {
                focus_input_ref(&input_ref);
                should_refocus.set(false);
            }
            || ()
        });
    }

    let submit = {
        let input = input.clone();
        let observation = observation.clone();
        let history = history.clone();
        let loading = loading.clone();
        let input_ref = input_ref.clone();
        let should_refocus = should_refocus.clone();
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();
            let raw = (*input).trim().to_owned();
            if raw.is_empty() || *loading {
                focus_input_ref(&input_ref);
                return;
            }
            should_refocus.set(true);
            input.set(String::new());
            dispatch_command(raw, &observation, &history, &loading);
            focus_input_ref(&input_ref);
        })
    };

    let input_changed = {
        let input = input.clone();
        Callback::from(move |event: InputEvent| {
            let target: HtmlInputElement = event.target_unchecked_into();
            input.set(target.value());
        })
    };

    let focus_input = {
        let input_ref = input_ref.clone();
        Callback::from(move |_| {
            focus_input_ref(&input_ref);
        })
    };

    html! {
        <div class="terminal-window" style={terminal_style(props.size)} onclick={focus_input}>
            <div class="terminal-header">
                <span class="terminal-title">{"Hinemos demo"}</span>
                <span class="terminal-state">{if *loading { "syncing" } else { "readonly" }}</span>
            </div>
            <div ref={screen_ref} class="terminal-screen" role="log" aria-live="polite">
                { for history.iter().map(render_line) }
            </div>
            <form class="terminal-input" onsubmit={submit}>
                <span>{PROMPT}</span>
                <input
                    ref={input_ref}
                    value={(*input).clone()}
                    oninput={input_changed}
                    autocomplete="off"
                    spellcheck="false"
                    aria-label="anonymous command"
                />
            </form>
        </div>
    }
}

fn dispatch_command(
    raw: String,
    observation: &UseStateHandle<Option<Observation>>,
    history: &UseStateHandle<Vec<TerminalLine>>,
    loading: &UseStateHandle<bool>,
) {
    let current_observation = (**observation).clone();
    let command = parse_command(&raw, current_observation.as_ref());
    let view_id = current_observation.map(|observation| observation.view_id);
    let mut next_history = (**history).clone();
    next_history.push(TerminalLine::Prompt(format!("{PROMPT} {raw}")));
    let Some(command) = command else {
        next_history.push(TerminalLine::Error(READONLY_SSH_GUIDANCE.to_owned()));
        history.set(next_history);
        return;
    };

    let pending_history = trim_history(next_history);
    history.set(pending_history.clone());
    loading.set(true);
    let observation = observation.clone();
    let history = history.clone();
    let loading = loading.clone();
    spawn_local(async move {
        let mut next_history = pending_history;
        match fetch_observation(Some(command), view_id).await {
            Ok(next) => {
                next_history.push(TerminalLine::Output(render_observation(&next)));
                observation.set(Some(next));
            }
            Err(error) => next_history.push(TerminalLine::Error(error)),
        }
        history.set(trim_history(next_history));
        loading.set(false);
    });
}

async fn fetch_observation(
    command: Option<CommandRequest>,
    view_id: Option<String>,
) -> Result<Observation, String> {
    let response = match command {
        Some(command) => {
            let body = serde_json::to_string(&command_payload(&command, view_id.as_deref()))
                .map_err(|error| error.to_string())?;
            Request::post("/api/anonymous/commands")
                .header("Content-Type", "application/json")
                .body(body)
                .map_err(|error| error.to_string())?
                .send()
                .await
        }
        None => Request::get("/api/anonymous/observe").send().await,
    }
    .map_err(|error| error.to_string())?;

    if !response.ok() {
        let status = response.status();
        return Err(response
            .json::<ErrorResponse>()
            .await
            .map(|error| error.error)
            .unwrap_or_else(|_| format!("request failed: {status}")));
    }
    response.json().await.map_err(|error| error.to_string())
}

fn command_payload(command: &CommandRequest, view_id: Option<&str>) -> serde_json::Value {
    let mut payload = match command {
        CommandRequest::Look => json!({ "kind": "look" }),
        CommandRequest::Map => json!({ "kind": "map" }),
        CommandRequest::Inventory => json!({ "kind": "inventory" }),
        CommandRequest::Help => json!({ "kind": "help" }),
        CommandRequest::Move(direction) => {
            json!({ "kind": "move", "direction": direction.as_str() })
        }
        CommandRequest::Inspect(id) => json!({ "kind": "inspect", "target": { "id": id } }),
        CommandRequest::Read(id) => json!({ "kind": "read", "target": { "id": id } }),
    };
    if let Some(view_id) = view_id
        && let Some(payload) = payload.as_object_mut()
    {
        payload.insert("viewId".to_owned(), json!(view_id));
    }
    payload
}

fn parse_command(input: &str, observation: Option<&Observation>) -> Option<CommandRequest> {
    let trimmed = input.trim();
    let normalized = trimmed.trim_start_matches('/').trim();
    let (name, rest) = normalized
        .split_once(char::is_whitespace)
        .map_or((normalized, ""), |(name, rest)| (name, rest.trim()));
    match name.to_ascii_lowercase().as_str() {
        "look" => Some(CommandRequest::Look),
        "map" => Some(CommandRequest::Map),
        "inventory" => Some(CommandRequest::Inventory),
        "help" => Some(CommandRequest::Help),
        "go" => parse_direction(rest).map(CommandRequest::Move),
        "inspect" => resolve_entity(rest, observation).map(CommandRequest::Inspect),
        "read" => resolve_entity(rest, observation).map(CommandRequest::Read),
        direction => parse_direction(direction).map(CommandRequest::Move),
    }
}

fn parse_direction(input: &str) -> Option<Direction> {
    match input.to_ascii_lowercase().as_str() {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        _ => None,
    }
}

fn resolve_entity(input: &str, observation: Option<&Observation>) -> Option<String> {
    let observation = observation?;
    if input.is_empty() {
        return single_readable_entity(observation);
    }
    let normalized = input.to_ascii_lowercase();
    observation
        .entities
        .iter()
        .find(|entity| {
            entity.id.eq_ignore_ascii_case(input)
                || entity.name.to_ascii_lowercase().contains(&normalized)
        })
        .map(|entity| entity.id.clone())
}

fn single_readable_entity(observation: &Observation) -> Option<String> {
    let readable = observation
        .entities
        .iter()
        .filter(|entity| entity.actions.iter().any(|action| action == "read"))
        .collect::<Vec<_>>();
    (readable.len() == 1).then(|| readable[0].id.clone())
}

fn render_observation(observation: &Observation) -> String {
    let mut lines = Vec::new();
    lines.push(observation.title.clone());
    if !observation.ascii_art.is_empty() {
        lines.extend(observation.ascii_art.clone());
    }
    lines.push(observation.description.clone());
    for event in &observation.events {
        match event {
            Event::Message { text } => lines.push(text.clone()),
            Event::Move { direction } => lines.push(format!("Moved {}.", direction.as_str())),
        }
    }
    let entity_line = observation
        .entities
        .iter()
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if !entity_line.is_empty() {
        lines.push(format!("Visible: {entity_line}"));
    }
    let command_line = available_command_labels(observation).join(", ");
    if !command_line.is_empty() {
        lines.push(format!("Available: {command_line}"));
    }
    lines.join("\n")
}

fn initial_terminal_history(observation: &Observation) -> Vec<TerminalLine> {
    vec![
        TerminalLine::Prompt(format!("{PROMPT} /look")),
        TerminalLine::Output(render_observation(observation)),
        TerminalLine::Output(
            "First real session:\nssh -T hinemos.ai\n/read agreement\n/agree\n/enter workers\n/position list"
                .to_owned(),
        ),
    ]
}

fn available_command_labels(observation: &Observation) -> Vec<String> {
    let mut commands = vec!["/look".to_owned(), "/map".to_owned(), "/help".to_owned()];
    for exit in &observation.exits {
        commands.push(format!("/go {}", exit.direction.as_str()));
    }
    commands
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

fn render_line(line: &TerminalLine) -> Html {
    match line {
        TerminalLine::Prompt(text) => {
            html! { <pre class="terminal-line terminal-prompt">{text}</pre> }
        }
        TerminalLine::Output(text) => html! { <pre class="terminal-line">{text}</pre> },
        TerminalLine::Error(text) => {
            html! { <pre class="terminal-line terminal-error">{text}</pre> }
        }
    }
}

fn trim_history(mut history: Vec<TerminalLine>) -> Vec<TerminalLine> {
    const MAX_LINES: usize = 8;
    if history.len() > MAX_LINES {
        history.drain(0..history.len() - MAX_LINES);
    }
    history
}

fn focus_input_ref(input_ref: &NodeRef) {
    if let Some(input) = input_ref.cast::<HtmlInputElement>() {
        let _ = input.focus();
    }
}

fn terminal_size_from_root(root: &Element) -> TerminalSize {
    TerminalSize {
        cols: terminal_attr(root, "data-terminal-cols", DEFAULT_TERMINAL_COLS, 48, 90),
        rows: terminal_attr(root, "data-terminal-rows", DEFAULT_TERMINAL_ROWS, 12, 28),
    }
}

fn terminal_attr(root: &Element, name: &str, default: u16, min: u16, max: u16) -> u16 {
    root.get_attribute(name)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| (*value >= min) && (*value <= max))
        .unwrap_or(default)
}

fn terminal_style(size: TerminalSize) -> String {
    let screen_height = f32::from(size.rows) * 0.9;
    format!(
        "--terminal-width: {}ch; --terminal-screen-height: {screen_height:.2}rem;",
        size.cols
    )
}

fn main() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(root) = document.get_element_by_id("world-sketch") else {
        return;
    };
    let size = terminal_size_from_root(&root);
    let _ = root.set_attribute("style", &terminal_style(size));
    yew::Renderer::<WorldSketch>::with_root_and_props(root, WorldSketchProps { size }).render();
}
