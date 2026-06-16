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
use hinemos_core::{JsonObservation, SemanticCommand};
use hinemos_runtime::GameRuntime;
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

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
        .route("/players/{player_id}/observe", get(observe_player))
        .route("/players/{player_id}/commands", post(execute_command))
        .route("/demo/observe", get(observe_demo_player))
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

async fn observe_demo_player(
    State(state): State<AppState>,
) -> Result<Json<JsonObservation>, ApiError> {
    observation_for_player(state, LOCAL_PLAYER_ID)
}

async fn observe_player(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
) -> Result<Json<JsonObservation>, ApiError> {
    observation_for_player(state, &player_id)
}

async fn execute_command(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
    Json(command): Json<SemanticCommand>,
) -> Result<Json<JsonObservation>, ApiError> {
    state
        .runtime
        .execute(&player_id, &command)
        .map(Json)
        .map_err(ApiError::runtime)
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}
