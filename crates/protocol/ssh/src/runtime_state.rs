//! Atomic runtime snapshot shared by SSH sessions and admin reloads.

use hinemos_admin_protocol::AdminStatus;
use hinemos_core::{JsonObservation, PlayerState, SemanticCommand, WorldState};
use hinemos_runtime::{Chrome, GameRuntime, ReloadError, RuntimeError, SlashParseError};
use tokio::sync::RwLock;

#[derive(Debug)]
pub(crate) struct RuntimeHandle {
    state: RwLock<RuntimeState>,
}

impl RuntimeHandle {
    pub(crate) fn new_with_grid_origin(world: WorldState, grid_origin_view_id: String) -> Self {
        Self {
            state: RwLock::new(RuntimeState::new_with_grid_origin(
                world,
                grid_origin_view_id,
            )),
        }
    }

    pub(crate) async fn chrome(&self) -> Chrome {
        self.state.read().await.chrome.clone()
    }

    pub(crate) async fn observe_json(
        &self,
        player_id: &str,
    ) -> Result<JsonObservation, RuntimeError> {
        self.state
            .read()
            .await
            .runtime
            .observe_json(player_id, Vec::new())
    }

    pub(crate) async fn execute(
        &self,
        player_id: &str,
        command: &SemanticCommand,
    ) -> Result<(JsonObservation, PlayerState), RuntimeError> {
        let state = self.state.read().await;
        let observation = state.runtime.execute(player_id, command)?;
        let player = state.runtime.player_state(player_id)?;
        Ok((observation, player))
    }

    pub(crate) async fn set_or_create_player(
        &self,
        saved_player: Option<PlayerState>,
        player_id: &str,
        template_player_id: &str,
    ) -> Result<(), RuntimeError> {
        let state = self.state.write().await;
        if let Some(player) = saved_player {
            state.runtime.set_player_state(player)
        } else {
            state
                .runtime
                .ensure_player_from_template(player_id, template_player_id)
        }
    }

    pub(crate) async fn player_state(&self, player_id: &str) -> Result<PlayerState, RuntimeError> {
        self.state.read().await.runtime.player_state(player_id)
    }

    pub(crate) async fn set_player_state(&self, player: PlayerState) -> Result<(), RuntimeError> {
        self.state.write().await.runtime.set_player_state(player)
    }

    pub(crate) async fn reload_from_world_dir_preserving_players(
        &self,
        dir: impl Into<std::path::PathBuf>,
        grid_origin_view_id: String,
    ) -> Result<(), ReloadError> {
        let dir = dir.into();
        let mut state = self.state.write().await;
        let runtime = state
            .runtime
            .reload_from_world_dir_preserving_players_with_grid_origin(dir, grid_origin_view_id)?;
        let world = runtime.world()?;
        *state = RuntimeState {
            chrome: Chrome::with_world(&world),
            runtime,
        };
        Ok(())
    }

    pub(crate) async fn world_counts(&self) -> Result<WorldCounts, RuntimeError> {
        let state = self.state.read().await;
        let world = state.runtime.world()?;
        Ok(WorldCounts {
            view_count: world.views.len(),
            entity_count: world.entities.len(),
            player_count: world.players.len(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldCounts {
    pub(crate) view_count: usize,
    pub(crate) entity_count: usize,
    pub(crate) player_count: usize,
}

impl WorldCounts {
    pub(crate) fn into_status(self, session_count: usize, user_count: usize) -> AdminStatus {
        AdminStatus {
            session_count,
            user_count,
            view_count: self.view_count,
            entity_count: self.entity_count,
            player_count: self.player_count,
        }
    }
}

#[derive(Debug)]
struct RuntimeState {
    runtime: GameRuntime,
    chrome: Chrome,
}

impl RuntimeState {
    fn new_with_grid_origin(world: WorldState, grid_origin_view_id: String) -> Self {
        let chrome = Chrome::with_world(&world);
        let runtime = GameRuntime::new_with_grid_origin(world, grid_origin_view_id);
        Self { runtime, chrome }
    }
}

pub(crate) fn parse_command(
    chrome: &Chrome,
    observation: Option<&JsonObservation>,
    input: &str,
) -> Result<SemanticCommand, SlashParseError> {
    chrome.parse_player_input_with_observation(input, observation)
}
