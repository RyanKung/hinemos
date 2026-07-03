#![deny(missing_docs)]

//! HTTP adapter for the Hinemos runtime and landing page.

use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Args;
use hinemos_app::AppService;
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
    let runtime = Arc::new(load_runtime_from_world_dir(&args.world)?);
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

fn load_runtime_from_world_dir(world_dir: &FsPath) -> Result<GameRuntime> {
    let world = load_world_from_dir(world_dir)
        .with_context(|| format!("failed to load world from {}", world_dir.display()))?;
    let app_config = AppService::<()>::load_world_app_config(world_dir)?;
    Ok(GameRuntime::new_with_grid_origin(
        world,
        app_config.admission_view_id,
    )?)
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
    if let SemanticCommand::Move { direction } = &request.command {
        return execute_anonymous_move(state, *direction, request.view_id).await;
    }
    if !anonymous_command_is_demo_safe(&request.command) {
        return Err(ApiError::forbidden(ANONYMOUS_SSH_GUIDANCE));
    }
    let observation = if let Some(view_id) = request.view_id {
        state.runtime.execute_read_only_at_view(
            ANONYMOUS_DEMO_PLAYER_ID,
            &view_id,
            &request.command,
        )
    } else {
        state.runtime.execute(LOCAL_PLAYER_ID, &request.command)
    };
    observation
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::extract::{Path as AxumPath, State};
    use hinemos_core::sample_world::LOCAL_PLAYER_ID;
    use hinemos_core::{Direction, EntityRef, ObservationEvent, SemanticCommand};

    use super::*;

    fn test_state() -> AppState {
        let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../worlds/sample");
        AppState {
            runtime: Arc::new(load_runtime_from_world_dir(&world_dir).expect("load runtime")),
        }
    }

    fn copy_sample_world_with_meta(meta: &str) -> PathBuf {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../worlds/sample");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "hinemos-http-world-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp world dir");
        for file in ["views.ron", "entities.ron", "players.ron", "rooms.ron"] {
            fs::copy(sample.join(file), dir.join(file)).expect("copy sample world file");
        }
        fs::write(dir.join("meta.ron"), meta).expect("write temp meta");
        dir
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
    async fn anonymous_read_only_command_uses_supplied_view_id() {
        let state = test_state();
        let mut player = state
            .runtime
            .player_state(LOCAL_PLAYER_ID)
            .expect("local player state");
        player.current_view = "grid_road_xp1_y0".to_owned();
        state
            .runtime
            .set_player_state(player)
            .expect("move local player away from origin");

        let Json(observation) = execute_anonymous_command(
            State(state),
            Json(AnonymousCommandRequest {
                command: SemanticCommand::Read {
                    target: EntityRef::new("cyber_scroll_board"),
                },
                view_id: Some("arrival_street".to_owned()),
            }),
        )
        .await
        .expect("read-only command should use supplied view");

        assert_eq!(observation.view_id, "arrival_street");
        assert!(observation.events.iter().any(|event| matches!(
            event,
            ObservationEvent::Message { text } if text.contains("Admission Agreement")
        )));
    }

    #[test]
    fn runtime_loader_uses_metadata_for_generated_grid_origin() {
        let world_dir = copy_sample_world_with_meta(
            r#"(
                admission_view_id: "west_main_street",
                admission_board_entity_id: "cyber_scroll_board",
                agreement_version: "2026-06-03",
            )"#,
        );
        let runtime = load_runtime_from_world_dir(&world_dir).expect("load runtime with meta");
        let mut player = runtime
            .player_state(LOCAL_PLAYER_ID)
            .expect("local player state");
        player.current_view = "grid_road_xp1_y0".to_owned();
        runtime.set_player_state(player).expect("set player state");

        let road = runtime
            .observe_json(LOCAL_PLAYER_ID, Vec::new())
            .expect("grid observation");
        let west = road
            .exits
            .iter()
            .find(|exit| exit.direction == Direction::West)
            .expect("west exit");
        let observation = runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Move {
                    direction: Direction::West,
                },
            )
            .expect("move to configured origin");

        assert_eq!(west.label.as_deref(), Some("West Hinemos Blvd"));
        assert_eq!(observation.view_id, "west_main_street");
        assert_eq!(observation.title, "West Hinemos Blvd");
        assert_eq!(
            observation
                .exits
                .iter()
                .filter_map(|exit| exit.label.as_deref())
                .collect::<Vec<_>>(),
            vec!["North 1 Rd.", "South 1 Rd.", "West 1 Rd.", "East 1 Rd."]
        );
        assert!(
            observation
                .ascii_art
                .join("\n")
                .contains("+----+----+----+----+")
        );
        assert!(!observation.ascii_art.join("\n").contains("shuttered"));
        assert!(
            !observation
                .exits
                .iter()
                .any(|exit| exit.label.as_deref() == Some("wilderness"))
        );
        let north = runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Move {
                    direction: Direction::North,
                },
            )
            .expect("configured origin keeps generated north exit");
        assert_eq!(north.view_id, "grid_road_x0_yp1");

        fs::remove_dir_all(world_dir).expect("remove temp world dir");
    }

    #[test]
    fn runtime_loader_rejects_missing_generated_grid_origin() {
        let world_dir = copy_sample_world_with_meta(
            r#"(
                admission_view_id: "missing_origin",
                admission_board_entity_id: "cyber_scroll_board",
                agreement_version: "2026-06-03",
            )"#,
        );

        let err = load_runtime_from_world_dir(&world_dir).expect_err("missing origin should fail");

        assert!(err.to_string().contains("view not found: missing_origin"));
        fs::remove_dir_all(world_dir).expect("remove temp world dir");
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
