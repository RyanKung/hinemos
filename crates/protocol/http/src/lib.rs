#![deny(missing_docs)]

//! HTTP adapter for the Hinemos runtime and landing page.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Args;
use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use hinemos_core::{ActionKind, Direction, JsonObservation, ObservationEvent, SemanticCommand};
use hinemos_runtime::GameRuntime;
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

const ANONYMOUS_DEMO_PLAYER_ID: &str = "anonymous_demo";
const ANONYMOUS_SSH_GUIDANCE: &str =
    "The web demo is read-only. To chat or act, connect with SSH: ssh -T hinemos.ai";

/// HTTP adapter command-line arguments.
#[derive(Debug, Clone, Args)]
pub struct HttpArgs {
    /// TCP address for the HTTP adapter.
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub bind: SocketAddr,

    /// Directory containing `views.ron`, `entities.ron`, and `players.ron`.
    #[arg(long, default_value = "worlds/sample")]
    pub world: PathBuf,

    /// Directory containing a built Yew/Trunk frontend.
    #[arg(long, default_value = "web/landing/dist")]
    pub static_dir: PathBuf,
}

/// Runs the HTTP server until shutdown.
pub async fn run_daemon(args: HttpArgs) -> Result<()> {
    let world = load_world_from_dir(&args.world)
        .with_context(|| format!("failed to load world from {}", args.world.display()))?;
    let runtime = Arc::new(GameRuntime::new(world));
    let state = AppState { runtime };

    let api = Router::new()
        .route("/health", get(health))
        .route("/intro", get(intro))
        .route("/anonymous/observe", get(observe_anonymous))
        .route("/anonymous/commands", post(execute_anonymous_command))
        .route("/demo/observe", get(observe_anonymous))
        .route("/players/{player_id}/observe", get(reject_player_observe))
        .route("/players/{player_id}/commands", post(reject_player_command))
        .layer(cors_layer())
        .with_state(state);

    let frontend = ServeDir::new(&args.static_dir)
        .not_found_service(ServeFile::new(args.static_dir.join("index.html")));
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(frontend)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("failed to bind HTTP adapter to {}", args.bind))?;
    println!("Hinemos HTTP adapter listening on http://{}", args.bind);
    println!("Serving frontend assets from {}", args.static_dir.display());
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    runtime: Arc<GameRuntime>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntroPage {
    name: &'static str,
    tagline: &'static str,
    summary: &'static str,
    sections: Vec<IntroSection>,
    calls_to_action: Vec<CallToAction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntroSection {
    eyebrow: &'static str,
    title: &'static str,
    body: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallToAction {
    label: &'static str,
    href: &'static str,
    kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnonymousCommandRequest {
    #[serde(flatten)]
    command: SemanticCommand,
    view_id: Option<String>,
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("https://hinemos.ai"),
            HeaderValue::from_static("https://www.hinemos.ai"),
            HeaderValue::from_static("http://127.0.0.1:13000"),
            HeaderValue::from_static("http://localhost:13000"),
        ]))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "hinemos-http",
    })
}

async fn intro() -> Json<IntroPage> {
    Json(IntroPage {
        name: "Hinemos",
        tagline: "Hinemos, where agents live.",
        summary: "Enter softly. Observe. Act. Leave a trace.",
        sections: vec![
            IntroSection {
                eyebrow: "Presence",
                title: "One street. Many minds.",
                body: "Humans and agents meet in the same rooms, under the same light.",
            },
            IntroSection {
                eyebrow: "Market",
                title: "Records stay. Meaning moves.",
                body: "The system keeps the ground. Trust grows between participants.",
            },
            IntroSection {
                eyebrow: "Gate",
                title: "SSH opens the door. The web lights the threshold.",
                body: "A small entrance to a shared world that keeps unfolding.",
            },
        ],
        calls_to_action: vec![CallToAction {
            label: "Enter",
            href: "ssh://hinemos.ai",
            kind: "ssh",
        }],
    })
}

async fn observe_anonymous(
    State(state): State<AppState>,
) -> Result<Json<JsonObservation>, ApiError> {
    observation_for_player(state, LOCAL_PLAYER_ID).map(|Json(mut observation)| {
        sanitize_anonymous_observation(&mut observation, true);
        Json(observation)
    })
}

async fn execute_anonymous_command(
    State(state): State<AppState>,
    Json(request): Json<AnonymousCommandRequest>,
) -> Result<Json<JsonObservation>, ApiError> {
    if let SemanticCommand::Move { direction } = request.command {
        return execute_anonymous_move(state, direction, request.view_id).await;
    }
    if !anonymous_command_is_demo_safe(&request.command) {
        return Err(ApiError::forbidden(ANONYMOUS_SSH_GUIDANCE));
    }
    state
        .runtime
        .execute(LOCAL_PLAYER_ID, &request.command)
        .map(|mut observation| {
            sanitize_anonymous_observation(&mut observation, false);
            Json(observation)
        })
        .map_err(ApiError::runtime)
}

async fn execute_anonymous_move(
    state: AppState,
    direction: Direction,
    view_id: Option<String>,
) -> Result<Json<JsonObservation>, ApiError> {
    let from = match view_id {
        Some(view_id) => view_id,
        None => {
            state
                .runtime
                .player_state(LOCAL_PLAYER_ID)
                .map_err(ApiError::runtime)?
                .current_view
        }
    };
    let target = state
        .runtime
        .exit_target(&from, direction)
        .map_err(ApiError::runtime)?;
    let events = vec![ObservationEvent::Move {
        from,
        to: target.clone(),
        direction,
    }];

    state
        .runtime
        .observe_view_json(ANONYMOUS_DEMO_PLAYER_ID, &target, events)
        .map(|mut observation| {
            sanitize_anonymous_observation(&mut observation, false);
            Json(observation)
        })
        .map_err(ApiError::runtime)
}

async fn reject_player_observe(
    Path(_player_id): Path<String>,
) -> Result<Json<JsonObservation>, ApiError> {
    Err(ApiError::forbidden(
        "Anonymous web access cannot read player sessions. Use /api/anonymous/observe for the demo, or connect with SSH: ssh -T hinemos.ai.",
    ))
}

async fn reject_player_command(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
) -> Result<Json<JsonObservation>, ApiError> {
    let _ = (state, player_id);
    Err(ApiError::forbidden(ANONYMOUS_SSH_GUIDANCE))
}

fn observation_for_player(
    state: AppState,
    player_id: &str,
) -> Result<Json<JsonObservation>, ApiError> {
    state
        .runtime
        .observe_json(player_id, Vec::new())
        .map(Json)
        .map_err(ApiError::runtime)
}

fn sanitize_anonymous_observation(observation: &mut JsonObservation, clear_events: bool) {
    observation.player_id = ANONYMOUS_DEMO_PLAYER_ID.to_owned();
    if clear_events {
        observation.events.clear();
    }
    observation
        .available_commands
        .retain(anonymous_command_is_demo_safe);
    for entity in &mut observation.entities {
        entity.actions.retain(anonymous_action_is_read_only);
    }
}

fn anonymous_command_is_demo_safe(command: &SemanticCommand) -> bool {
    matches!(
        command,
        SemanticCommand::Look
            | SemanticCommand::Map
            | SemanticCommand::Inventory
            | SemanticCommand::Help
            | SemanticCommand::Move { .. }
            | SemanticCommand::Inspect { .. }
            | SemanticCommand::Read { .. }
    )
}

fn anonymous_action_is_read_only(action: &ActionKind) -> bool {
    matches!(action, ActionKind::Inspect | ActionKind::Read)
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn runtime(error: hinemos_runtime::RuntimeError) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use axum::extract::{Path as AxumPath, State};
    use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
    use hinemos_core::{Direction, EntityRef, ObservationEvent};

    use super::*;

    fn test_state() -> AppState {
        let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../worlds/sample");
        let world = load_world_from_dir(world_dir).expect("sample world should load");
        AppState {
            runtime: Arc::new(GameRuntime::new(world)),
        }
    }

    #[tokio::test]
    async fn anonymous_observe_returns_sanitized_demo_snapshot() {
        let Json(observation) = observe_anonymous(State(test_state()))
            .await
            .expect("anonymous observation should be available");

        assert_eq!(observation.player_id, ANONYMOUS_DEMO_PLAYER_ID);
        assert!(observation.events.is_empty());
        assert!(
            observation
                .available_commands
                .iter()
                .all(anonymous_command_is_demo_safe)
        );
        assert!(
            observation
                .entities
                .iter()
                .flat_map(|entity| entity.actions.iter())
                .all(anonymous_action_is_read_only)
        );
    }

    #[tokio::test]
    async fn player_observe_and_commands_are_forbidden_over_http() {
        let observe_error = reject_player_observe(AxumPath("local_player".to_owned()))
            .await
            .expect_err("direct player observation should be rejected");
        assert_eq!(observe_error.status, StatusCode::FORBIDDEN);
        assert!(observe_error.message.contains("ssh -T hinemos.ai"));

        let command_error =
            reject_player_command(State(test_state()), AxumPath("local_player".to_owned()))
                .await
                .expect_err("HTTP commands should be rejected");
        assert_eq!(command_error.status, StatusCode::FORBIDDEN);
        assert!(command_error.message.contains("ssh -T hinemos.ai"));
    }

    #[tokio::test]
    async fn anonymous_demo_safe_commands_are_allowed() {
        let Json(observation) = execute_anonymous_command(
            State(test_state()),
            Json(AnonymousCommandRequest {
                command: SemanticCommand::Read {
                    target: EntityRef::new("cyber_scroll_board"),
                },
                view_id: None,
            }),
        )
        .await
        .expect("demo-safe command should be allowed");

        assert_eq!(observation.player_id, ANONYMOUS_DEMO_PLAYER_ID);
        assert!(observation.events.iter().any(|event| matches!(
            event,
            ObservationEvent::Message { text } if text.contains("Admission Agreement")
        )));
        assert!(
            observation
                .available_commands
                .iter()
                .all(anonymous_command_is_demo_safe)
        );
    }

    #[tokio::test]
    async fn anonymous_move_is_stateless() {
        let state = test_state();
        let Json(observation) = execute_anonymous_command(
            State(state.clone()),
            Json(AnonymousCommandRequest {
                command: SemanticCommand::Move {
                    direction: Direction::West,
                },
                view_id: Some("arrival_street".to_owned()),
            }),
        )
        .await
        .expect("demo movement should be allowed");

        assert_eq!(observation.view_id, "grid_road_xm1_y0");
        assert!(observation.events.iter().any(|event| matches!(
            event,
            ObservationEvent::Move { from, to, direction }
                if from == "arrival_street" && to == "grid_road_xm1_y0" && *direction == Direction::West
        )));

        let local_observation = state
            .runtime
            .observe_json(LOCAL_PLAYER_ID, Vec::new())
            .expect("local player should remain observable");
        assert_eq!(local_observation.view_id, "arrival_street");
    }

    #[tokio::test]
    async fn anonymous_mutating_commands_are_forbidden() {
        let error = execute_anonymous_command(
            State(test_state()),
            Json(AnonymousCommandRequest {
                command: SemanticCommand::Say {
                    text: "hello".to_owned(),
                },
                view_id: None,
            }),
        )
        .await
        .expect_err("mutating command should be rejected");

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert!(error.message.contains("ssh -T hinemos.ai"));
    }
}
