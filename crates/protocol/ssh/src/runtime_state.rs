//! Atomic runtime snapshot shared by SSH sessions and admin reloads.

use tokio::sync::RwLock;
use xagora_core::{JsonObservation, PlayerState, SemanticCommand, WorldState};
use xagora_runtime::{Chrome, GameRuntime, ReloadError, RuntimeError, SlashParseError};

#[derive(Debug)]
pub(crate) struct RuntimeHandle {
    state: RwLock<RuntimeState>,
}

impl RuntimeHandle {
    pub(crate) fn new(world: WorldState) -> Self {
        Self {
            state: RwLock::new(RuntimeState::new(world)),
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

    pub(crate) async fn reload_from_world_dir_preserving_players(
        &self,
        dir: impl Into<std::path::PathBuf>,
    ) -> Result<(), ReloadError> {
        let dir = dir.into();
        let mut state = self.state.write().await;
        let runtime = state
            .runtime
            .reload_from_world_dir_preserving_players(dir)?;
        let world = runtime.world()?;
        *state = RuntimeState {
            chrome: Chrome::with_world(&world),
            runtime,
        };
        Ok(())
    }
}

#[derive(Debug)]
struct RuntimeState {
    runtime: GameRuntime,
    chrome: Chrome,
}

impl RuntimeState {
    fn new(world: WorldState) -> Self {
        let chrome = Chrome::with_world(&world);
        let runtime = GameRuntime::new(world);
        Self { runtime, chrome }
    }
}

pub(crate) fn parse_command(
    chrome: &Chrome,
    input: &str,
) -> Result<SemanticCommand, SlashParseError> {
    chrome.parse_command(input)
}
